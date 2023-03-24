use std::fs::File;
use std::io::Error as IOError;
use std::io::ErrorKind as IOErrorKind;
use std::io::Read;
use std::path::PathBuf;
use std::path::Path;

#[derive(Debug)]
pub struct SensorDevice {
    path_temp: PathBuf,
    path_offset: Option<PathBuf>,
}

impl SensorDevice {

    const FACTOR: f32 = 1000.0;

    pub fn new(device: impl AsRef<Path>) -> Result<Self, IOError> {
        let path = device.as_ref();
        let path_temp = path.join("temp");
        if !path_temp.try_exists()? {
            return Err(IOError::new(IOErrorKind::NotFound, format!("{}", path_temp.display())));
        }
        let path_offset = {
            let path_offset = path.join("offset");
            match path_offset.try_exists() {
                Ok(true) => Some(path_offset),
                Ok(false) => None,
                Err(e) => None,
            }
        };
        Ok(
            SensorDevice {
                path_temp,
                path_offset,
            }
        )
    }

    pub fn get(&self) -> Result<f32, IOError> {
        let mut buf = [0u8; 8];
        let temp = {
            let mut ifile = File::open(&self.path_temp)?;
            let len = ifile.read(&mut buf)?;
            if len == 0 {
                return Err(IOError::new(IOErrorKind::UnexpectedEof, "empty file: temp"));
            }
            if len > 8 {
                return Err(IOError::new(IOErrorKind::InvalidData, "too long file: temp"));
            }
            let (temp, i) = Self::parse(&buf[..len]);
            if i == 0 {
                return Err(IOError::new(IOErrorKind::InvalidData, "invalid file: temp"));
            }
            temp
        };
        let offset = {
            if let Some(path_offset) = self.path_offset.as_ref() {
                let mut ifile = File::open(path_offset)?;
                let len = ifile.read(&mut buf)?;
                if len == 0 {
                    return Err(IOError::new(IOErrorKind::UnexpectedEof, "empty file: offset"));
                }
                if len > 8 {
                    return Err(IOError::new(IOErrorKind::InvalidData, "too long file: offset"));
                }
                let (offset, i) = Self::parse(&buf[..len]);
                if i == 0 {
                    return Err(IOError::new(IOErrorKind::InvalidData, "invalid file: offset"));
                }
                offset
            } else {
                0
            }
        };
        Ok( (temp - offset) as f32 / Self::FACTOR )
    }

    fn parse(buf: &[u8]) -> (u32, usize) {
        let mut i = 0;
        let mut num = 0;
        while i < buf.len() {
            let c = buf[i];
            if c < b'0' || c > b'9' {
                break;
            }
            num = num * 10 + (c - b'0') as u32;
            i += 1;
        }
        (num, i)
    }
}