use std::io;
use std::path::PathBuf;
use std::time::Duration; 

use clap::Parser;
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


#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {

    /// Path to the sensor device; like "/sys/class/thermal/thermal_zone0"
    #[clap(short = 'i', long)]
    watch: PathBuf,

    /// Path to the pwm device; like "/sys/devices/platform/fd8b0010.pwm/pwm/pwmchip1"
    #[clap(short = 'o', long)]
    execute: PathBuf,

    /// Interval between temperature checks, in milliseconds
    #[clap(short = 'd', long, default_value = "5000")]
    interval: u64,

    /// Time before the pwm change when temperature drop, in times of interval
    #[clap(short = 'G', long, default_value = "32")]
    max_speed_time_cycle: usize,

    /// Time before the pwm change when temperature drop, in times of interval
    #[clap(short = 'w', long, default_value = "8")]
    lag_time_cycle: usize,

    /// Temperature to stop the pwm, in degrees Celsius
    #[clap(short = '0', long, default_value = "35.0")]
    stop_temperature: f32,

    /// Temperature to start the pwm, in degrees Celsius
    #[clap(short = '1', long, default_value = "40.0")]
    start_temperature: f32,

    /// Temperature when pwm should reach maximum, in degrees Celsius
    #[clap(short = '2', long, default_value = "70.0")]
    high_temperature: f32,

    /// Minimum duty cycle, in (0, 1)
    #[clap(short = 'L', long, default_value = "0.5")]
    min_duty_cycle: f32,

    /// Maximum duty cycle, in (0, 1)
    #[clap(short = 'U', long, default_value = "0.9")]
    max_duty_cycle: f32,

    /// PWM frequency, in Hz
    #[clap(short = 'f', long, default_value = "10000")]
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

    pub fn new(args: Args) -> io::Result<Self> {
        let sensor = SensorDevice::new(args.watch.as_path())?;
        log::info!("sensor initialized: path={}", args.watch.display());
        let pwm = PWMDevice::new(args.execute.as_path(), 0)?;
        log::info!("pwm initialized: path={}/pwm{}", args.execute.display(), 0);
        let f = Function::new(
            args.stop_temperature, 
            args.start_temperature, 
            args.high_temperature, 
            args.min_duty_cycle,
            args.max_duty_cycle,
        );
        log::info!("control initialized: function={}", &f);
        let control = Control::new(f, args.lag_time_cycle);
        log::info!("control initialized: interval={}ms, lag_time_cycle={}, max_speed_time_cycle={}", args.interval, args.lag_time_cycle, args.max_speed_time_cycle);
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

    simple_logger::init_with_env().unwrap();

    let args = Args::parse();
    let mut app = Application::new(args).unwrap();

    unsafe { signal::register(&[libc::SIGINT, libc::SIGTERM, libc::SIGUSR1, libc::SIGUSR2]) };

    app.initial().unwrap();

    while let Ok(signum) = unsafe { signal::wait(app.interval) } {
        match signum {
            libc::SIGINT => {
                log::debug!("receive SIGINT to terminate");
                app.terminate().unwrap();
                break;
            }
            libc::SIGTERM => {
                log::debug!("receive SIGTERM to terminate");
                app.terminate().unwrap();
                break;
            }
            libc::SIGUSR2 => {
                log::debug!("receive SIGUSR2 to maximum fan speed");
                app.run_max_speed().unwrap();
            }
            libc::SIGUSR1 => {
                log::debug!("receive SIGUSR1");
            }
            0 => {
                app.run().unwrap();
            }
            _ => {
                unreachable!("Unknown signal: {}", signum);
            }
        }
    }
}
