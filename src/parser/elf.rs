use anyhow::{Context, Result};
use goblin::elf::Elf;
use std::fs;
use std::path::Path;

pub fn get_imports<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
    let buffer = fs::read(&path).with_context(|| format!("Failed to read binary: {:?}", path.as_ref()))?;
    let elf = Elf::parse(&buffer).with_context(|| "Failed to parse ELF binary")?;

    let mut imports = Vec::new();
    for library in elf.libraries {
        imports.push(library.to_string());
    }
    Ok(imports)
}