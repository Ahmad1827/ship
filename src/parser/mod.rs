pub mod elf;
pub mod pe;

use anyhow::{anyhow, Result};
use std::fs;
use std::path::Path;

pub enum BinaryFormat {
    Pe,
    Elf,
}

pub fn detect_format<P: AsRef<Path>>(path: P) -> Result<BinaryFormat> {
    let buffer = fs::read(&path)?;
    if buffer.len() < 4 {
        return Err(anyhow!("File is too small to be an executable"));
    }

    if buffer.starts_with(b"MZ") {
        Ok(BinaryFormat::Pe)
    } else if buffer.starts_with(b"\x7fELF") {
        Ok(BinaryFormat::Elf)
    } else {
        Err(anyhow!("Unsupported binary format"))
    }
}

pub fn parse_dependencies<P: AsRef<Path>>(path: P) -> Result<(BinaryFormat, Vec<String>)> {
    let format = detect_format(&path)?;
    let deps = match format {
        BinaryFormat::Pe => pe::get_imports(&path)?,
        BinaryFormat::Elf => elf::get_imports(&path)?,
    };
    Ok((format, deps))
}