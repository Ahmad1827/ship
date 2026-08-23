use anyhow::{Context, Result};
use goblin::pe::PE;
use std::fs;
use std::path::Path;

pub fn get_imports<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
    let buffer = fs::read(&path).with_context(|| format!("Failed to read binary: {:?}", path.as_ref()))?;
    let pe = PE::parse(&buffer).with_context(|| "Failed to parse PE binary")?;

    let mut imports = Vec::new();
    for import in pe.imports {
        imports.push(import.dll.to_string());
    }
    Ok(imports)
}