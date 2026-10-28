//! Process-wide isolation selected before any application services start.
//!
//! Never switch an already running process back to live access. TempDir owns
//! fresh storage; no preference, session token or cache is copied from a user.
use std::{ffi::OsString, path::PathBuf, sync::OnceLock};

struct Sandbox {
    root: tempfile::TempDir,
    smoke: bool,
}

static SANDBOX: OnceLock<Sandbox> = OnceLock::new();

pub(crate) fn isolated() -> bool {
    SANDBOX.get().is_some()
}

pub(crate) fn smoke_requested() -> bool {
    SANDBOX.get().is_some_and(|sandbox| sandbox.smoke)
}

pub(crate) fn config_dir() -> Option<PathBuf> {
    SANDBOX
        .get()
        .map(|sandbox| sandbox.root.path().join("config"))
}

pub(crate) fn initialize(args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let args: Vec<_> = args.into_iter().skip(1).collect();
    let entry_points = [
        "--self-check",
        "--entropy-hid-proxy",
        "--ak820-install-proxy",
        "--mcp-stdio",
        "--demo-ak820",
        "--runtime-smoke",
        "--ak820-helper",
        "--export-layout",
    ];
    if args
        .iter()
        .filter(|arg| entry_points.iter().any(|mode| arg == mode))
        .count()
        > 1
    {
        return Err("Application entry points cannot be combined".into());
    }
    if args
        .iter()
        .any(|arg| arg.to_string_lossy().starts_with("--mcp-"))
        && args
            .iter()
            .any(|arg| !arg.to_string_lossy().starts_with("--mcp-"))
    {
        return Err("MCP cannot be combined with GUI or private helper arguments".into());
    }
    let smoke = args.iter().any(|arg| arg == "--runtime-smoke");
    let demo = args.iter().any(|arg| arg == "--demo-ak820");
    if !smoke && !demo {
        return Ok(());
    }
    // Offline GUI modes are deliberately not combined with helper/export/MCP
    // modes. In particular a malformed smoke invocation cannot start live HID.
    if args.len() != 1 {
        return Err("Use --runtime-smoke or --demo-ak820 as the only argument".into());
    }
    let root = tempfile::Builder::new()
        .prefix("ak-configurator-offline-")
        .tempdir()
        .map_err(|_| "Cannot create isolated application storage")?;
    SANDBOX
        .set(Sandbox { root, smoke })
        .map_err(|_| "Application isolation was already initialized".to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn conflicting_offline_modes_fail_before_initialization() {
        for args in [
            vec!["app", "--runtime-smoke", "--ak820-helper"],
            vec!["app", "--demo-ak820", "--entropy-hid-proxy"],
            vec!["app", "--runtime-smoke", "--mcp-stdio"],
        ] {
            assert!(super::initialize(args.into_iter().map(Into::into)).is_err());
        }
        assert!(!super::isolated());
        assert!(crate::isolated_runtime());
    }
}
