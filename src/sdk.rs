use anyhow::{anyhow, Context, Result};
use colored::*;
use std::env;
use std::fs::{self, File};
use sha2::{Digest, Sha256};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::ZipArchive;

/// A prebuilt SDK zip that gets downloaded once and cached under `~/.ship/<cache_name>`.
pub struct Sdk {
    pub label: &'static str,
    pub cache_name: &'static str,
    pub url: &'static str,
    /// Top-level folder inside the zip.
    pub root_dir: &'static str,
    /// SHA-256 of the zip; a download that doesn't match is rejected.
    pub sha256: &'static str,
}

pub const SFML_MINGW: Sdk = Sdk {
    label: "SFML Windows (MinGW)",
    cache_name: "sfml-win",
    url: "https://github.com/SFML/SFML/releases/download/2.6.1/SFML-2.6.1-windows-gcc-13.1.0-mingw-64-bit.zip",
    root_dir: "SFML-2.6.1",
    sha256: "30b5531c80dbac8aa9c610202f2da9b2e1221138d05517f8fb7c17552e35c699",
};

pub const SFML_MSVC: Sdk = Sdk {
    label: "SFML Windows (MSVC)",
    cache_name: "sfml-msvc",
    url: "https://github.com/SFML/SFML/releases/download/2.6.1/SFML-2.6.1-windows-vc17-64-bit.zip",
    root_dir: "SFML-2.6.1",
    sha256: "00a5f16c7b577eb4d62c2212993e26b23d0a7ad2ff93a0ada2198ebf8e013f25",
};

/// A single prebuilt file (not a zip) cached under `~/.ship/tools/`.
pub struct Tool {
    pub label: &'static str,
    pub file_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
}

/// AppImage type-2 runtime, pinned to a dated release rather than "continuous".
pub const APPIMAGE_RUNTIME: Tool = Tool {
    label: "AppImage runtime",
    file_name: "appimage-runtime-x86_64-20251108",
    url: "https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64",
    sha256: "2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d",
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

/// Returns the cached tool, downloading and verifying it first if needed.
pub fn ensure_tool(tool: &Tool) -> Result<PathBuf> {
    let tools_dir = ship_dir()?.join("tools");
    fs::create_dir_all(&tools_dir)?;
    let final_path = tools_dir.join(tool.file_name);
    if final_path.is_file() {
        return Ok(final_path);
    }

    println!("{} Fetching {}...", "⚡".bright_yellow(), tool.label);
    let partial = tools_dir.join(format!("{}.partial", tool.file_name));
    let result = download_verified(tool.url, tool.sha256, &partial, tool.label)
        .and_then(|_| fs::rename(&partial, &final_path).map_err(Into::into));
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result?;
    Ok(final_path)
}

/// Downloads `url` to `dest` and checks it against the pinned SHA-256.
fn download_verified(url: &str, sha256: &str, dest: &Path, label: &str) -> Result<()> {
    // -f: treat HTTP errors as failures instead of saving the error page.
    let status = Command::new("curl")
        .arg("-fL")
        .arg("--retry").arg("2")
        .arg("-o").arg(dest)
        .arg(url)
        .status()
        .context("Failed to run curl (is it installed?)")?;
    if !status.success() {
        return Err(anyhow!("Download failed for {} from {}", label, url));
    }

    let actual = sha256_file(dest)?;
    if !actual.eq_ignore_ascii_case(sha256) {
        return Err(anyhow!(
            "Checksum mismatch for {} from {}\n  expected {}\n  got      {}",
            label, url, sha256, actual
        ));
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut file = File::open(path)?;
    io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect())
}

fn download_and_extract(sdk: &Sdk, work_dir: &PathBuf) -> Result<PathBuf> {
    let zip_path = work_dir.join("sdk.zip");
    download_verified(sdk.url, sdk.sha256, &zip_path, &format!("{} SDK", sdk.label))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vector() {
        let dir = std::env::temp_dir().join(format!("ship-sha-test-{}", std::process::id()));
        fs::write(&dir, b"abc").unwrap();
        let hash = sha256_file(&dir).unwrap();
        let _ = fs::remove_file(&dir);
        assert_eq!(hash, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
