use std::fmt;
use std::fmt::Display;
use std::fs::File;
use std::io::Error as IOError;
use std::io::ErrorKind as IOErrorKind;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;


#[derive(Debug, Clone)]
pub enum Polarity {
    Normal,
}

impl Display for Polarity {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Polarity::Normal => write!(f, "normal"),
        }
    }
}


#[derive(Debug, Clone)]
pub struct PWMDevice {
    instance_period_path: PathBuf,
    instance_duty_cycle_path: PathBuf,
    instance_polarity_path: PathBuf,
    instance_enable_path: PathBuf,
}

impl PWMDevice {
    
    pub fn new(device: impl AsRef<Path>, instance: u32) -> Result<Self, IOError> {
        let path = device.as_ref();
        let instance_path = path.join(format!("pwm{}", instance));
        let is_exist = match instance_path.try_exists() {
            Ok(true) => true,
            Ok(false) => false,
            Err(e) => false,
        };
        if !is_exist {
            let mut ofile = File::options().write(true).open(path.join("export"))?;
            write!(ofile, "{}", instance)?;
        }

        let instance_period_path = instance_path.join("period");
        if !instance_period_path.try_exists()? {
            return Err(IOError::new(IOErrorKind::NotFound, format!("{}", instance_period_path.display())));
        }
        let instance_duty_cycle_path = instance_path.join("duty_cycle");
        if !instance_duty_cycle_path.try_exists()? {
            return Err(IOError::new(IOErrorKind::NotFound, format!("{}", instance_duty_cycle_path.display())));
        }
        let instance_polarity_path = instance_path.join("polarity");
        if !instance_polarity_path.try_exists()? {
            return Err(IOError::new(IOErrorKind::NotFound, format!("{}", instance_polarity_path.display())));
        }
        let instance_enable_path = instance_path.join("enable");
        if !instance_enable_path.try_exists()? {
            return Err(IOError::new(IOErrorKind::NotFound, format!("{}", instance_enable_path.display())));
        }
        Ok(
            PWMDevice {
                instance_period_path,
                instance_duty_cycle_path,
                instance_polarity_path,
                instance_enable_path,
            }
        )
    }

    pub fn set_period(&mut self, period: u32) -> Result<(), IOError> {
        let mut ofile = File::options().write(true).open(&self.instance_period_path)?;
        write!(ofile, "{}", period)?;
        Ok(())
    }

    pub fn set_duty_cycle(&mut self, duty_cycle: u32) -> Result<(), IOError> {
        let mut ofile = File::options().write(true).open(&self.instance_duty_cycle_path)?;
        write!(ofile, "{}", duty_cycle)?;
        Ok(())
    }

    pub fn set_polarity(&mut self, polarity: Polarity) -> Result<(), IOError> {
        let mut ofile = File::options().write(true).open(&self.instance_polarity_path)?;
        write!(ofile, "{}", polarity)?;
        Ok(())
    }

    pub fn set_enable(&mut self, enable: bool) -> Result<(), IOError> {
        let mut ofile = File::options().write(true).open(&self.instance_enable_path)?;
        if enable {
            write!(ofile, "1")?;
        } else {
            write!(ofile, "0")?;
        }
        Ok(())
    }
}