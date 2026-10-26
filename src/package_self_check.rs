// SPDX-License-Identifier: GPL-3.0-or-later
//! Executable/package smoke probe. This module has no device or network path.
use crate::ak820_settings_backup::sha256;
use crate::app::ak820_beta1_profiles as beta;
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::PathBuf;

const USAGE: &str = "Usage: ak-configurator --self-check [--output NEW_JSON_FILE]";

fn parse_request(args: &[OsString]) -> Result<Option<Option<PathBuf>>, &'static str> {
    if !args.iter().skip(1).any(|arg| arg == "--self-check") {
        return Ok(None);
    }
    match &args[1..] {
        [flag] if flag == "--self-check" => Ok(Some(None)),
        [flag, output, path]
            if flag == "--self-check"
                && output == "--output"
                && !path.is_empty()
                && path != OsStr::new("--self-check") =>
        {
            Ok(Some(Some(PathBuf::from(path))))
        }
        _ => Err(USAGE),
    }
}

fn report() -> Result<serde_json::Value, String> {
    let mut profiles = Vec::new();
    for image in beta::profiles() {
        let id = image.profile;
        let (ihx, map) = crate::ak820_release::bundled_sources(id)
            .map_err(|error| format!("{}: {error}", id.as_str()))?;
        let defaults = crate::ak820_release::bundled_defaults(id)
            .map_err(|error| format!("{} defaults: {error}", id.as_str()))?;
        if defaults.len() != 0x2800 || image.settings_schema != 0x082003 {
            return Err(format!("Unexpected settings contract for {}", id.as_str()));
        }
        profiles.push(serde_json::json!({
            "profile": id.as_str(),
            "ihx_sha256": sha256(ihx.as_bytes()),
            "map_sha256": sha256(map.as_bytes()),
            "default_settings_sha256": sha256(&defaults),
        }));
    }
    if profiles.len() != 10 || beta::module_descriptions().profiles.len() != 10 {
        return Err("Incomplete bundled profile/help catalog".into());
    }
    let icon = crate::app_icon::egui_icon(32);
    if icon.rgba.len() != 32 * 32 * 4 || icon.width != 32 || icon.height != 32 {
        return Err("Invalid bundled application icon".into());
    }
    let architecture = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => return Err("Unsupported package architecture".into()),
    };
    if !matches!(std::env::consts::OS, "linux" | "windows" | "macos") {
        return Err("Unsupported package operating system".into());
    }
    Ok(serde_json::json!({
        "format": "ak-configurator-self-check-1",
        "application_version": env!("CARGO_PKG_VERSION"),
        "platform": format!("{}-{architecture}", std::env::consts::OS),
        "firmware_release": beta::BETA_RELEASE,
        "catalog_sha256": beta::SOURCE_CATALOG_SHA256,
        "profiles": profiles,
        "hardware_access": false,
        "network_access": false,
        "status": "passed",
    }))
}

fn publish(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let mut file =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| "Cannot create self-check report")?;
    file.write_all(bytes)
        .map_err(|_| "Cannot write self-check report")?;
    file.as_file()
        .sync_all()
        .map_err(|_| "Cannot sync self-check report")?;
    file.persist_noclobber(path)
        .map_err(|_| "Cannot publish self-check report; output must not exist")?;
    Ok(())
}

pub(crate) fn run_if_requested(args: impl IntoIterator<Item = OsString>) -> Option<i32> {
    let args = args.into_iter().collect::<Vec<_>>();
    let request = match parse_request(&args) {
        Ok(None) => return None,
        Ok(Some(request)) => request,
        Err(error) => {
            eprintln!("{error}");
            return Some(2);
        }
    };
    let result = report().and_then(|value| {
        let mut bytes =
            serde_json::to_vec_pretty(&value).map_err(|_| "Cannot encode self-check report")?;
        bytes.push(b'\n');
        if let Some(path) = request {
            publish(&path, &bytes)
        } else {
            std::io::stdout()
                .lock()
                .write_all(&bytes)
                .map_err(|_| "Cannot write self-check report".into())
        }
    });
    Some(match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Package self-check failed: {error}");
            1
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn self_check_cannot_be_combined_with_device_or_gui_commands() {
        assert_eq!(parse_request(&args(&["app", "--demo-ak820"])), Ok(None));
        assert_eq!(
            parse_request(&args(&["app", "--self-check"])),
            Ok(Some(None))
        );
        for request in [
            vec!["app", "--self-check", "--ak820-helper"],
            vec!["app", "--self-check", "--output"],
            vec!["app", "--self-check", "--output", "", "--self-check"],
            vec!["app", "--minimized", "--self-check"],
        ] {
            assert!(parse_request(&args(&request)).is_err());
        }
    }

    #[test]
    fn self_check_validates_all_bundled_public_sources_without_hardware() {
        let result = report().unwrap();
        assert_eq!(result["profiles"].as_array().unwrap().len(), 10);
        assert_eq!(result["hardware_access"], false);
        assert_eq!(result["network_access"], false);
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/ak820-beta2-defaults.json"))
                .unwrap();
        for profile in result["profiles"].as_array().unwrap() {
            assert_eq!(
                profile["default_settings_sha256"],
                expected[profile["profile"].as_str().unwrap()]
            );
        }
    }

    #[test]
    fn report_publication_never_replaces_an_existing_file() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("report.json");
        publish(&output, b"original\n").unwrap();
        assert!(publish(&output, b"replacement\n").is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"original\n");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
