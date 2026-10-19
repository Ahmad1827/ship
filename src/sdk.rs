use anyhow::{anyhow, Context, Result};
use colored::*;
use std::env;
use std::fs::{self, File};
use std::io;
use std::path::PathBuf;
use std::process::Command;
use zip::ZipArchive;

/// A prebuilt SDK zip that gets downloaded once and cached under `~/.ship/<cache_name>`.
pub struct Sdk {
    pub label: &'static str,
    pub cache_name: &'static str,
    pub url: &'static str,
    /// Top-level folder inside the zip.
    pub root_dir: &'static str,
}

pub const SFML_MINGW: Sdk = Sdk {
    label: "SFML Windows (MinGW)",
    cache_name: "sfml-win",
    url: "https://github.com/SFML/SFML/releases/download/2.6.1/SFML-2.6.1-windows-gcc-13.1.0-mingw-64-bit.zip",
    root_dir: "SFML-2.6.1",
};

pub const SFML_MSVC: Sdk = Sdk {
    label: "SFML Windows (MSVC)",
    cache_name: "sfml-msvc",
    url: "https://github.com/SFML/SFML/releases/download/2.6.1/SFML-2.6.1-windows-vc17-64-bit.zip",
    root_dir: "SFML-2.6.1",
};

pub fn ship_dir() -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME directory not found")?;
    let path = PathBuf::from(home).join(".ship");
    fs::create_dir_all(&path)?;
    Ok(path)
}

/// Returns the cached SDK dir, downloading it first if needed.
///
/// Everything happens in a `.partial` scratch dir that is only renamed into place once the
/// download and extraction both succeed, so an interrupted fetch is retried next run instead
/// of leaving a broken SDK cached forever.
pub fn ensure(sdk: &Sdk) -> Result<PathBuf> {
    let ship_dir = ship_dir()?;
    let final_dir = ship_dir.join(sdk.cache_name);
    if final_dir.is_dir() {
        return Ok(final_dir);
    }

    println!("{} Auto-fetching {} SDK...", "⚡".bright_yellow(), sdk.label);

    let work_dir = ship_dir.join(format!("{}.partial", sdk.cache_name));
    if work_dir.exists() {
        fs::remove_dir_all(&work_dir)?;
    }
    fs::create_dir_all(&work_dir)?;

    let result = download_and_extract(sdk, &work_dir).and_then(|extracted| {
        fs::rename(&extracted, &final_dir)
            .with_context(|| format!("Failed to move SDK into {:?}", final_dir))
    });
    let _ = fs::remove_dir_all(&work_dir);
    result?;

    println!("{} {} SDK ready", "✔".green(), sdk.label);
    Ok(final_dir)
}

fn download_and_extract(sdk: &Sdk, work_dir: &PathBuf) -> Result<PathBuf> {
    let zip_path = work_dir.join("sdk.zip");

    // -f: treat HTTP errors as failures instead of saving the error page as the zip.
    let status = Command::new("curl")
        .arg("-fL")
        .arg("--retry").arg("2")
        .arg("-o").arg(&zip_path)
        .arg(sdk.url)
        .status()
        .context("Failed to run curl (is it installed?)")?;
    if !status.success() {
        return Err(anyhow!("Download failed for {} SDK from {}", sdk.label, sdk.url));
    }

    let file = File::open(&zip_path)?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("Downloaded {} SDK is not a valid zip", sdk.label))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(outpath) = entry.enclosed_name().map(|p| work_dir.join(p)) else { continue };

        if entry.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = File::create(&outpath)?;
            io::copy(&mut entry, &mut outfile)
                .with_context(|| format!("Failed extracting {:?}", outpath))?;
        }
    }

    let extracted = work_dir.join(sdk.root_dir);
    if !extracted.is_dir() {
        return Err(anyhow!("{} SDK zip did not contain expected folder '{}'", sdk.label, sdk.root_dir));
    }
    Ok(extracted)
}
