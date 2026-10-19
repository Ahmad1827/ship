//! Uploads to itch.io through butler.

use anyhow::{anyhow, Context, Result};
use colored::*;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::builder::TargetPlatform;
use crate::project::ItchConfig;

/// Resolves "user/game:channel" from the CLI value, then `[itch]`, defaulting the channel
/// to the platform name so itch.io tags the upload correctly.
pub fn resolve_target(cli: Option<&str>, config: Option<&ItchConfig>, platform: TargetPlatform) -> Result<String> {
    let target = cli
        .map(str::to_string)
        .or_else(|| config.and_then(|c| c.target.clone()))
        .ok_or_else(|| anyhow!("--push needs a target: --push user/game, or target under [itch] in ship.toml"))?;

    let (project, channel) = match target.split_once(':') {
        Some((project, channel)) => (project.to_string(), Some(channel.to_string())),
        None => (target.clone(), None),
    };
    // Catches a path being taken as the target, e.g. `ship --push ./game.exe`.
    let is_slug = |part: &str| {
        !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    let parts: Vec<&str> = project.split('/').collect();
    if parts.len() != 2 || !parts.iter().all(|part| is_slug(part)) {
        return Err(anyhow!("Invalid itch.io target '{}': expected user/game[:channel]", target));
    }

    let channel = channel
        .or_else(|| config.and_then(|c| c.channel.clone()))
        .unwrap_or_else(|| match platform {
            TargetPlatform::Windows => "windows".into(),
            TargetPlatform::Linux => "linux".into(),
        });
    Ok(format!("{}:{}", project, channel))
}

/// Fails early when butler isn't installed.
pub fn preflight() -> Result<()> {
    let found = Command::new("butler")
        .arg("-V")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok();
    if !found {
        return Err(anyhow!(
            "--push needs butler: install it from https://itch.io/docs/butler/ and run `butler login`"
        ));
    }
    Ok(())
}

pub fn push(path: &Path, target: &str, version: Option<&str>) -> Result<()> {
    println!("{} Pushing {} to {}...", "⇪".bright_yellow(), path.display(), target.bright_white());
    let mut cmd = Command::new("butler");
    cmd.arg("push").arg(path).arg(target);
    if let Some(version) = version {
        cmd.arg("--userversion").arg(version);
    }
    let status = cmd.status().context("Failed to run butler")?;
    if !status.success() {
        return Err(anyhow!("butler push failed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_resolution() {
        let config = ItchConfig { target: Some("me/game".into()), channel: None };
        let win = TargetPlatform::Windows;
        assert_eq!(resolve_target(None, Some(&config), win).unwrap(), "me/game:windows");
        assert_eq!(resolve_target(None, Some(&config), TargetPlatform::Linux).unwrap(), "me/game:linux");
        assert_eq!(resolve_target(Some("a/b:beta"), Some(&config), win).unwrap(), "a/b:beta");

        let with_channel = ItchConfig { target: Some("me/game".into()), channel: Some("win-demo".into()) };
        assert_eq!(resolve_target(None, Some(&with_channel), win).unwrap(), "me/game:win-demo");

        assert!(resolve_target(None, None, win).is_err());
        assert!(resolve_target(Some("./game.exe"), None, win).is_err());
        assert!(resolve_target(Some("game"), None, win).is_err());
    }
}
