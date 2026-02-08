use std::fs::File;
use std::io::{Error, Read};

/// Given file path, and load configuration file.
pub(crate) fn load_file(file: &str) -> Result<String, Error> {
    let mut content = String::new();
    let mut fh = File::open(file)?;
    fh.read_to_string(&mut content)?;
    Ok(content)
}
