use std::env;
use std::io;
use std::path::PathBuf;
use std::process;
use std::time::Duration; 

use control::Control;
use control::ControlOutput;
use control::Function;
use ini::FieldParseError;
use ini::Ini;
use pwm::PWMDevice;
use pwm::Polarity;
use sensor::SensorDevice;

mod signal;
mod sensor;
mod pwm;
mod control;
mod ini;


#[derive(Debug)]
struct Args {

    /// Path to the sensor device; like "/sys/class/thermal/thermal_zone0"
    watch: PathBuf,

    /// Path to the pwm device; like "/sys/devices/platform/fd8b0010.pwm/pwm/pwmchip1"
    execute: PathBuf,

    /// Interval between temperature checks, in milliseconds
    interval: u64,

    /// Time before the pwm change when temperature drop, in times of interval
    max_speed_time_cycle: usize,

    /// Time before the pwm change when temperature drop, in times of interval
    lag_time_cycle: usize,

    /// Temperature to stop the pwm, in degrees Celsius
    stop_temperature: f32,

    /// Temperature to start the pwm, in degrees Celsius
    start_temperature: f32,

    /// Temperature when pwm should reach maximum, in degrees Celsius
    high_temperature: f32,

    /// Minimum duty cycle, in (0, 1)
    min_duty_cycle: f32,

    /// Maximum duty cycle, in (0, 1)
    max_duty_cycle: f32,

    /// PWM frequency, in Hz
    pwm_frequency: u32,
}


impl Default for Args {
    fn default() -> Self {
        Self {
            watch: PathBuf::new(),
            execute: PathBuf::new(),
            interval: 5000,
            max_speed_time_cycle: 32,
            lag_time_cycle: 8,
            stop_temperature: 30.0,
            start_temperature: 40.0,
            high_temperature: 70.0,
            min_duty_cycle: 0.5,
            max_duty_cycle: 0.9,
            pwm_frequency: 10000,
        }
    }
}

impl Ini for Args {
    type Err = FieldParseError;

    fn callback(
        &mut self, 
        filename: &std::path::Path, 
        line: &str, 
        line_number: usize, 
        section: &str, 
        key: &str, 
        value: Option<&str>
    ) -> Result<(), Self::Err> {
        if section.is_empty() {
            match key {
                "watch" => self.watch = PathBuf::from(FieldParseError::parse(value, "watch")?),
                "execute" => self.execute = PathBuf::from(FieldParseError::parse(value, "execute")?),
                "interval" => self.interval = FieldParseError::parse_value(value, "interval")?,
                "max_speed_time_cycle" => self.max_speed_time_cycle = FieldParseError::parse_value(value, "max_speed_time_cycle")?,
                "lag_time_cycle" => self.lag_time_cycle = FieldParseError::parse_value(value, "lag_time_cycle")?,
                "stop_temperature" => self.stop_temperature = FieldParseError::parse_value(value, "stop_temperature")?,
                "start_temperature" => self.start_temperature = FieldParseError::parse_value(value, "start_temperature")?,
                "high_temperature" => self.high_temperature = FieldParseError::parse_value(value, "high_temperature")?,
                "min_duty_cycle" => self.min_duty_cycle = FieldParseError::parse_value(value, "min_duty_cycle")?,
                "max_duty_cycle" => self.max_duty_cycle = FieldParseError::parse_value(value, "max_duty_cycle")?,
                "pwm_frequency" => self.pwm_frequency = FieldParseError::parse_value(value, "pwm_frequency")?,
                _ => {}
            }
        }
        Ok(())
    }
}


struct Application {
    sensor: SensorDevice,
    pwm: PWMDevice,
    frequency: u32,
    on: bool,
    control: Control,
    interval: Duration,
    max_speed_time_cycle: usize,
    max_speed_remaining_cycle: usize,
}

impl Application {

    pub fn new_from_config(config: &str) -> io::Result<Self> {
        let mut args = Args::default();
        args.parse_from_file(config)?;
        Self::new(args)
    }

    pub fn new(args: Args) -> io::Result<Self> {
        let sensor = SensorDevice::new(args.watch.as_path())?;
        log::info!("sensor initialized: path={}", args.watch.as_path().display());
        let instance = 0;
        let pwm = PWMDevice::new(args.execute.as_path(), instance)?;
        log::info!("pwm initialized: path={}/pwm{}, pwm_frequency={}", args.execute.as_path().display(), instance, args.pwm_frequency);
        let f = Function::new(
            args.stop_temperature,
            args.start_temperature,
            args.high_temperature,
            args.min_duty_cycle,
            args.max_duty_cycle,
        )
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        log::info!("control initialized: function={}", &f);
        let control = Control::new(f, args.lag_time_cycle);
        log::info!("control initialized: interval={}ms, lag_time_cycle={}, max_speed_time_cycle={}",args.interval, args.lag_time_cycle, args.max_speed_time_cycle);
        Ok(
            Self {
                sensor,
                pwm,
                frequency: args.pwm_frequency,
                on: false,
                control,
                interval: Duration::from_millis(args.interval),
                max_speed_time_cycle: args.max_speed_time_cycle,
                max_speed_remaining_cycle: 0,
            }
        )
    }

    pub fn initial(&mut self) -> io::Result<()> {
        self.pwm.set_period(self.frequency)?;
        self.pwm.set_polarity(Polarity::Normal)?;
        log::info!("fan initialized: frequency={}Hz, polarity={}", self.frequency, Polarity::Normal);
        let temperature = self.sensor.get()?;
        let output = self.control.update_force(temperature, self.control.min_duty_cycle());
        log::trace!("control status: temperature={:.2}°C, output={:?}", temperature, output);
        match output {
            ControlOutput::Off | ControlOutput::Keep => {
                unreachable!()
            }
            ControlOutput::Change(duty_cycle) => {
                if self.start_pwm(duty_cycle)? {
                    log::info!("fan launched at {:.2}°C with pwm-duty-ratio={:.2}%", temperature, duty_cycle * 100.0);
                }
            }
        }
        Ok(())
    }

    pub fn run(&mut self) -> io::Result<()> {
        if self.max_speed_remaining_cycle > 0 {
            self.max_speed_remaining_cycle -= 1;
        } else {
            let temperature = self.sensor.get()?;
            let output = self.control.update(temperature);
            log::trace!("control status: temperature={:.2}°C, output={:?}", temperature, output);
            match output {
                ControlOutput::Off => {
                    if self.stop_pwm()? {
                        log::info!("fan stopped at {:.2}°C", temperature);
                    }
                }
                ControlOutput::Change(duty_cycle) => {
                    if self.start_pwm(duty_cycle)? {
                        log::info!("fan started at {:.2}°C with pwm-duty-ratio={:.2}%", temperature, duty_cycle * 100.0);
                    } else {
                        log::debug!("fan changed at {:.2}°C with pwm-duty-ratio={:.2}%", temperature, duty_cycle * 100.0);
                    }
                }
                ControlOutput::Keep => {
                    // do nothing
                }
            }
        }
        Ok(())
    }

    pub fn run_max_speed(&mut self) -> io::Result<()> {
        let duty_cycle = self.control.max_duty_cycle();
        self.start_pwm(duty_cycle)?;
        self.max_speed_remaining_cycle = self.max_speed_time_cycle;
        log::info!("fan set to maximum speed for {} cycles with pwm-duty-ratio={:.2}%", self.max_speed_time_cycle, duty_cycle * 100.0);
        Ok(())
    }

    pub fn terminate(&mut self) -> io::Result<()> {
        self.stop_pwm()?;
        log::info!("fan terminated");
        Ok(())
    }

    fn stop_pwm(&mut self) -> io::Result<bool> {
        if self.on {
            self.pwm.set_enable(false)?;
            self.on = false;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn start_pwm(&mut self, duty_cycle: f32) -> io::Result<bool> {
        self.pwm.set_duty_cycle((duty_cycle * self.frequency as f32) as u32)?;
        if !self.on {
            self.pwm.set_enable(true)?;
            self.on = true;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}


fn get_log_level() -> log::LevelFilter {
    match std::env::var("RUST_LOG") {
        Ok(s) => {
            match s.to_lowercase().as_str() {
                "error" => log::LevelFilter::Error,
                "warn" => log::LevelFilter::Warn,
                "info" => log::LevelFilter::Info,
                "debug" => log::LevelFilter::Debug,
                "trace" => log::LevelFilter::Trace,
                _ => log::LevelFilter::Info,
            }
        }
        Err(_e) => log::LevelFilter::Info,
    }
}

fn main() {

    #[cfg(feature = "betterlog")]
    simple_logger::SimpleLogger::new().env().with_local_timestamps().init().unwrap();

    #[cfg(not(feature = "betterlog"))]
    simple_logger::SimpleLogger::new().env().init().unwrap();

    let (mut app, path) = {
        let cfg_path = env::args().nth(1).unwrap_or_else(|| String::from("fanctrl.conf"));
        match cfg_path.as_str() {
            "-v" | "--version" => {
                println!("{} {}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            "-h" | "--help" => {
                println!("{} {}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"));
                println!("{}", env!("CARGO_PKG_DESCRIPTION"));
                println!();
                println!("Usage:  {} [CONFIGURATION_FILE]", env!("CARGO_BIN_NAME"));
                process::exit(0);
            }
            _ => {
                match Application::new_from_config(cfg_path.as_str()) {
                    Ok(app) => (app, cfg_path),
                    Err(e) => {
                        log::error!("failed to create application: {:?}", e);
                        process::exit(1);
                    }
                }
            }
        }
    };

    unsafe { signal::register(&[libc::SIGINT, libc::SIGTERM, libc::SIGUSR1, libc::SIGUSR2]) };

    if let Err(e) = app.initial() {
        log::error!("failed to initialize: {:?}", e);
        process::exit(1);
    }

    while let Ok(signum) = unsafe { signal::wait(app.interval) } {
        match signum {
            libc::SIGINT => {
                log::debug!("receive SIGINT to terminate");
                if let Err(e) = app.terminate() {
                    log::error!("failed to terminate: {:?}", e);
                }
                break;
            }
            libc::SIGTERM => {
                log::debug!("receive SIGTERM to terminate");
                if let Err(e) = app.terminate() {
                    log::error!("failed to terminate: {:?}", e);
                }
                break;
            }
            libc::SIGUSR2 => {
                log::debug!("receive SIGUSR2 to maximum fan speed");
                if let Err(e) = app.run_max_speed() {
                    log::error!("failed to set fan speed to maximum: {:?}", e);
                }
            }
            libc::SIGUSR1 => {
                log::debug!("receive SIGUSR1");
            }
            0 => {
                if let Err(e) = app.run() {
                    log::error!("failed to run loop: {:?}", e);
                }
            }
            _ => {
                unreachable!("Unknown signal: {}", signum);
            }
        }
    }
}
