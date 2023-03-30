use std::error::Error as StdError;
use std::fmt;
use std::path::Path;
use std::io::Error as IOError;
use std::io::BufRead;
use std::io::BufReader;
use std::str::FromStr;


#[derive(Debug)]
pub struct FieldParseError {
    field: &'static str,
    missing: bool,
}

impl FieldParseError {
    
    pub fn parse<'a>(s: Option<&'a str>, field: &'static str) -> Result<&'a str, Self> {
        s.ok_or_else(|| Self { field, missing: true })
    }

    pub fn parse_value<'a, T>(s: Option<&'a str>, field: &'static str) -> Result<T, Self> 
    where 
        T: FromStr, 
        <T as FromStr>::Err: std::error::Error
    {
        let s = Self::parse(s, field)?;
        s.parse().map_err(|_e| Self { field, missing: false })
    }
}

impl fmt::Display for FieldParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FieldParseError: field '{}'", self.field)?;
        if self.missing {
            write!(f, " is missing")?;
        }
        Ok(())
    }
}

impl StdError for FieldParseError {

}

impl Into<IOError> for FieldParseError {

    fn into(self) -> IOError {
        IOError::new(std::io::ErrorKind::InvalidData, self)
    }
}




/// inspired by https://crates.io/crates/cini
pub trait Ini {
    /// The associated error which can be returned from parsing.
    type Err: Into<IOError>;

    /// The callback function that is called for every line parsed.
    fn callback(
        &mut self, 
        filename: &Path, 
        line: &str, 
        line_number: usize, 
        section: &str, 
        key: &str, 
        value: Option<&str>
    ) -> Result<(), Self::Err>;

    /// Parses a single line of an ini str.
    fn parse_line<'a>(
        &mut self,
        filename: &Path,
        line: &'a str,
        line_number: usize,
        mut section: String,
    ) -> Result<String, Self::Err> {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            return Ok(section);
        }

        if line.starts_with('[') && line.ends_with(']') {
            let header = &line[1..line.len() - 1];
            section = String::from(header);
        } else {
            let pair = split_pair(line);
            self.callback(filename, line, line_number, section.as_str(), pair.0, pair.1)?;
        }
        Ok(section)
    }

    /// Parses an ini file.
    fn parse_from_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), IOError> {
        let path = path.as_ref();
        let ifile = std::fs::File::open(path)?;
        let mut reader = BufReader::new(ifile);
        let mut section = String::new();
        let mut line = String::new();
        let mut line_number = 0;
        while reader.read_line(&mut line)? > 0 {
            line_number += 1;
            section = self.parse_line(path, line.as_str(), line_number, section).map_err(Self::Err::into)?;
            line.clear();
        }
        Ok(())
    }
}

fn split_pair(s: &str) -> (&str, Option<&str>) {
    let mut split = s.splitn(2, '=');
    (
        split.next().unwrap().trim_end(),
        split.next().map(|s| s.trim_start()),
    )
}