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

    /// Path to the sensor device
    #[clap(short = 'i', long)]
    watch: PathBuf,

    /// Path to the pwm device
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


pub struct PWMDeviceWrapper {
    pwm: PWMDevice,
    is_enabled: bool,
    frequency: u32,
}

impl PWMDeviceWrapper {

    pub fn new(pwm: PWMDevice, frequency: u32) -> Self {
        Self { pwm, is_enabled: false, frequency }
    }

    fn initialize(&mut self, initial: f32) -> io::Result<()> {
        // assume initial is in (0, 1)
        self.pwm.set_period(self.frequency)?;
        self.pwm.set_polarity(Polarity::Normal)?;
        self.is_enabled = true;
        self.pwm.set_duty_cycle(self.calc_duty_cycle(initial))?;
        self.pwm.set_enable(self.is_enabled)?;
        println!("fan launch: duty_ratio={}", initial);
        Ok(())
    }

    fn change_speed(&mut self, duty_ratio: f32, temperature: f32) -> io::Result<()> {
        // assume initial is in (0, 1)
        self.pwm.set_duty_cycle(self.calc_duty_cycle(duty_ratio))?;
        if !self.is_enabled {
            self.is_enabled = true;
            self.pwm.set_enable(self.is_enabled)?;
            println!("fan start: temperature={}C, duty_ratio={}", temperature, duty_ratio);
        }
        Ok(())
    }

    fn stop(&mut self, temperature: f32) -> io::Result<()> {
        if self.is_enabled {
            self.is_enabled = false;
            self.pwm.set_enable(self.is_enabled)?;
            println!("fan stop: temperature={}C", temperature);
        }
        Ok(())
    }

    fn terminate(&mut self) -> io::Result<()> {
        if self.is_enabled {
            self.is_enabled = false;
            self.pwm.set_enable(self.is_enabled)?;
            println!("fan terminate");
        }
        Ok(())
    }

    fn calc_duty_cycle(&self, duty_ratio: f32) -> u32 {
        f32::max(duty_ratio * self.frequency as f32, 0.0) as u32
    }
}


pub enum AppLoopState {
    Launch(usize),
    MaxSpeed(usize),
    Normal,
}


fn run_normal(sensor: &SensorDevice, pwm: &mut PWMDeviceWrapper, control: &mut Control) -> io::Result<()> {
    let temperature = sensor.get()?;
    let output = control.update(temperature);
    match output {
        ControlOutput::Off => {
            pwm.stop(temperature)?;
        }
        ControlOutput::Change(duty_ratio) => {
            pwm.change_speed(duty_ratio, temperature)?;
        }
        ControlOutput::Keep => {
            // do nothing
        }
    }
    Ok(())
}

fn main() {

    let (sensor, mut pwm, mut control, interval, max_speed_time_cycle) = {

        let args = Args::parse();
        let sensor = SensorDevice::new(args.watch).expect("sensor device error");
        let pwm = PWMDevice::new(args.execute, 0).expect("pwm device error");
        let pwm = PWMDeviceWrapper::new(pwm, args.pwm_frequency);
        let f = Function::new(
            args.stop_temperature, 
            args.start_temperature, 
            args.high_temperature, 
            args.min_duty_cycle,
            args.max_duty_cycle,
        );
        let control = Control::new(f, args.lag_time_cycle);
        (sensor, pwm, control, Duration::from_millis(args.interval), args.max_speed_time_cycle)
    };

    unsafe { signal::register(&[libc::SIGINT, libc::SIGTERM, libc::SIGUSR1, libc::SIGUSR2]) };

    pwm.initialize(control.min_duty_cycle()).unwrap();

    let mut state = AppLoopState::Launch(control.lag_time_cycle());

    while let Ok(signum) = unsafe { signal::wait(interval) } {
        match signum {
            libc::SIGINT => {
                println!("receive SIGINT to terminate");
                pwm.terminate().unwrap();
                break;
            }
            libc::SIGTERM => {
                println!("receive SIGTERM to terminate");
                pwm.terminate().unwrap();
                break;
            }
            libc::SIGUSR1 => {
                println!("receive SIGUSR1 to maximum fan speed for {} cycles", max_speed_time_cycle);
                state = AppLoopState::MaxSpeed(max_speed_time_cycle);
                pwm.change_speed(control.max_duty_cycle(), sensor.get().unwrap()).unwrap();
            }
            libc::SIGUSR2 => {
                println!("receive SIGUSR2");
            }
            0 => {
                match &mut state {
                    AppLoopState::Launch(cycle) => {
                        if *cycle > 0 {
                            *cycle -= 1;
                        } else {
                            state = AppLoopState::Normal;
                        }
                    }
                    AppLoopState::MaxSpeed(cycle) => {
                        if *cycle > 0 {
                            *cycle -= 1;
                        } else {
                            state = AppLoopState::Normal;
                            run_normal(&sensor, &mut pwm, &mut control).unwrap();
                        }
                    }
                    AppLoopState::Normal => {
                        run_normal(&sensor, &mut pwm, &mut control).unwrap();
                    }
                }
            }
            _ => {
                unreachable!("Unknown signal: {}", signum);
            }
        }
    }
}
