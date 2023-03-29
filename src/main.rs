use std::env;
use std::io;
use std::path::PathBuf;
use std::process;
use std::str::FromStr;
use std::time::Duration; 

use ini::Ini;
use control::Control;
use control::ControlOutput;
use control::Function;
use pwm::PWMDevice;
use pwm::Polarity;
use sensor::SensorDevice;

mod signal;
mod sensor;
mod pwm;
mod control;


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

    fn parse<'a>(ini: &'a Ini, field: &'static str) -> io::Result<&'a str> {
        ini.get_from::<String>(None, field).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, field))
    }

    fn parse_value<T>(ini: &Ini, field: &'static str) -> io::Result<T> 
    where 
        T: FromStr, 
        <T as FromStr>::Err: std::error::Error
    {
        let s = Application::parse(ini, field)?;
        s.parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {}={}", e, field, s)))
    }

    pub fn new(ini: Ini) -> io::Result<Self> {
        let watch = Application::parse(&ini, "watch")?;
        let sensor = SensorDevice::new(watch)?;
        log::info!("sensor initialized: path={}", watch);
        let execute = Application::parse(&ini, "execute")?;
        let instance = 0;
        let pwm_frequency = Application::parse_value(&ini, "pwm_frequency")?;
        let pwm = PWMDevice::new(execute, instance)?;
        log::info!("pwm initialized: path={}/pwm{}, pwm_frequency={}", execute, instance, pwm_frequency);
        let f = Function::new(
            Application::parse_value(&ini, "stop_temperature")?,
            Application::parse_value(&ini, "start_temperature")?,
            Application::parse_value(&ini, "high_temperature")?,
            Application::parse_value(&ini, "min_duty_cycle")?,
            Application::parse_value(&ini, "max_duty_cycle")?,
        )
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        log::info!("control initialized: function={}", &f);
        let lag_time_cycle = Application::parse_value(&ini, "lag_time_cycle")?;
        let control = Control::new(f, lag_time_cycle);
        let interval = Application::parse_value(&ini, "interval")?;
        let max_speed_time_cycle = Application::parse_value(&ini, "max_speed_time_cycle")?;
        log::info!("control initialized: interval={}ms, lag_time_cycle={}, max_speed_time_cycle={}",interval, lag_time_cycle, max_speed_time_cycle);
        Ok(
            Self {
                sensor,
                pwm,
                frequency: pwm_frequency,
                on: false,
                control,
                interval: Duration::from_millis(interval),
                max_speed_time_cycle: max_speed_time_cycle,
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

    simple_logger::SimpleLogger::new().env().with_local_timestamps().init().unwrap();

    let (mut args, path) = {
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
                match Ini::load_from_file(cfg_path.as_str()) {
                    Ok(cfg) => (cfg, cfg_path),
                    Err(e) => {
                        log::error!("failed to load configuration: {:?}", e);
                        process::exit(1);
                    }
                }
            }
        }
    };
    let mut app = match Application::new(args) {
        Ok(app) => app,
        Err(e) => {
            log::error!("failed to create application: {:?}", e);
            process::exit(1);
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
