//! Authenticode signing of Windows executables via osslsigncode.

use anyhow::{anyhow, Context, Result};
use colored::*;
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::project::SignConfig;

/// Checks everything signing needs before any build work starts, so a missing tool or
/// password fails in a second instead of after a full build.
pub fn preflight(config: &SignConfig) -> Result<()> {
    let found = Command::new("osslsigncode")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok();
    if !found {
        return Err(anyhow!("[sign] is configured but osslsigncode is not installed (apt install osslsigncode)"));
    }
    if !config.pkcs12.is_file() {
        return Err(anyhow!("Signing certificate not found: {:?}", config.pkcs12));
    }
    if env::var_os(&config.password_env).is_none() {
        return Err(anyhow!(
            "Signing password missing: set the {} environment variable",
            config.password_env
        ));
    }
    Ok(())
}

/// Signs `exe` in place.
pub fn sign_file(config: &SignConfig, exe: &Path, description: &str) -> Result<()> {
    let password = env::var(&config.password_env)
        .with_context(|| format!("Signing password missing: set {}", config.password_env))?;
    let signed = exe.with_extension("signed.tmp");

    let mut cmd = Command::new("osslsigncode");
    cmd.arg("sign")
        .arg("-pkcs12").arg(&config.pkcs12)
        // Piped through stdin so the password never shows up in the process list,
        // the way `-pass <password>` would.
        .arg("-readpass").arg("/dev/stdin")
        .arg("-n").arg(description)
        .arg("-h").arg("sha256")
        .arg("-in").arg(exe)
        .arg("-out").arg(&signed)
        .stdin(Stdio::piped())
        .stdout(Stdio::null());
    if !config.timestamp_url.is_empty() {
        cmd.arg("-ts").arg(&config.timestamp_url);
    }

    let mut child = cmd.spawn().context("Failed to run osslsigncode")?;
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().context("osslsigncode stdin unavailable")?;
        stdin.write_all(password.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        let _ = fs::remove_file(&signed);
        return Err(anyhow!("osslsigncode failed to sign {:?}", exe));
    }

    fs::rename(&signed, exe)?;
    println!(
        "  {} Signed: {}",
        "✔".green(),
        exe.file_name().unwrap_or_default().to_string_lossy().bright_white()
    );
    Ok(())
}
