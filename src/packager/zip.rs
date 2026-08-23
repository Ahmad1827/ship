use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub fn compress_directory<P: AsRef<Path>, Q: AsRef<Path>>(source_dir: P, zip_path: Q) -> Result<()> {
    let source_dir = source_dir.as_ref();
    let zip_path = zip_path.as_ref();

    let file = File::create(zip_path).with_context(|| format!("Failed to create zip file: {:?}", zip_path))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    let mut buffer = Vec::new();

    for entry in WalkDir::new(source_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let relative_path = path.strip_prefix(source_dir)?;

        if path.is_file() {
            let path_str = relative_path.to_str().context("Invalid UTF-8 in file path")?;
            zip.start_file(path_str, options)?;
            let mut f = File::open(path)?;
            buffer.clear();
            f.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
        } else if !relative_path.as_os_str().is_empty() {
            let path_str = relative_path.to_str().context("Invalid UTF-8 in directory path")?;
            zip.add_directory(path_str, options)?;
        }
    }

    zip.finish()?;
    Ok(())
}