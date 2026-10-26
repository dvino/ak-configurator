//! Bounded local directory I/O for the firmware settings backup format.
//!
//! Layout and publication order are adapted from `tools/installer/settings_backup.py`
//! at firmware revision `bef20f89d3da01d770767919bc7c8fc3c3a1a808`. This module
//! handles local bytes only: it does not capture a device or perform HID I/O.
use crate::ak820_settings_backup::{
    canonical, sha256, BackupResult, ImageIdentity, ValidatedArchive, FILES, FORMAT, FORMAT_V2,
};
use crate::ak820_settings_model::{validate_historical_archive, validate_profile_archive};
use crate::app::ak820_beta1_profiles::{historical_settings_identity, matrix_profile, ProfileId};
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[cfg(test)]
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

const MANIFEST: &str = "manifest.json";
const MANIFEST_LIMIT: usize = 65_536;
const ENTRY_COUNT: usize = FILES.len() + 1;
const HISTORY_LIMIT: usize = 128;
const HISTORY_ENTRY_LIMIT: usize = 1024;

/// A fully validated immutable backup. The underlying archive and file bytes
/// are shared by clones and cannot be modified through this API.
#[derive(Clone)]
pub(crate) struct LoadedBackup {
    archive: Arc<ValidatedArchive>,
    manifest_raw: Arc<Vec<u8>>,
    files: Arc<BTreeMap<String, Vec<u8>>>,
    source_profile: Option<ProfileId>,
}
impl LoadedBackup {
    pub(crate) fn archive(&self) -> &ValidatedArchive {
        &self.archive
    }
    pub(crate) fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
    pub(crate) fn source_profile(&self) -> Option<ProfileId> {
        self.source_profile
    }
    pub(crate) fn nvm(&self) -> &[u8] {
        &self.files["saved-nvm.bin"]
    }
    pub(crate) fn manifest_raw(&self) -> &[u8] {
        &self.manifest_raw
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BackupHistoryEntry {
    pub(crate) name: String,
    pub(crate) path: std::path::PathBuf,
    pub(crate) created_utc: String,
    pub(crate) source_profile: Option<ProfileId>,
    pub(crate) schema: u32,
    pub(crate) manifest_sha256: String,
}
#[derive(Clone, Debug)]
pub(crate) struct BackupHistory {
    pub(crate) entries: Vec<BackupHistoryEntry>,
    pub(crate) invalid_skipped: usize,
    pub(crate) warning: Option<String>,
}

fn invalid() -> String {
    "Invalid or inaccessible backup directory".into()
}

fn allowed_name(name: &std::ffi::OsStr) -> bool {
    name == MANIFEST || FILES.iter().any(|(entry, _)| name == *entry)
}

#[cfg(target_os = "windows")]
#[path = "ak820_backup_windows.rs"]
mod windows;

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod anchored {
    use super::*;
    use std::ffi::{CStr, CString, OsStr, OsString};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Component;

    // macOS mode bits do not remove inherited extended ACL grants. Strip
    // ACLs through the owned descriptor, then verify the empty ACL before any
    // private bytes are written. Parent ACLs are checked without modifying them.
    #[cfg(target_os = "macos")]
    mod privacy {
        use super::*;
        use std::ffi::c_void;
        extern "C" {
            fn acl_init(count: i32) -> *mut c_void;
            fn acl_free(value: *mut c_void) -> i32;
            fn acl_get_fd_np(fd: i32, kind: i32) -> *mut c_void;
            fn acl_set_fd_np(fd: i32, acl: *mut c_void, kind: i32) -> i32;
            fn acl_valid(acl: *mut c_void) -> i32;
            fn acl_get_entry(acl: *mut c_void, entry: i32, out: *mut *mut c_void) -> i32;
        }
        struct Acl(*mut c_void);
        impl Drop for Acl {
            fn drop(&mut self) {
                unsafe {
                    acl_free(self.0);
                }
            }
        }
        pub(super) fn empty(file: &File) -> BackupResult<()> {
            let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), 0x100) };
            if acl.is_null() {
                return if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
                    Ok(())
                } else {
                    Err(invalid())
                };
            }
            let acl = Acl(acl);
            if unsafe { acl_valid(acl.0) } != 0 {
                return Err(invalid());
            }
            let mut entry = std::ptr::null_mut();
            if unsafe { acl_get_entry(acl.0, 0, &mut entry) } == 0 {
                return Err("Backup paths with extended ACL grants are unsupported".into());
            }
            if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINVAL) {
                return Err(invalid());
            }
            Ok(())
        }
        pub(super) fn strip(file: &File) -> BackupResult<()> {
            let acl = Acl(unsafe { acl_init(0) });
            if acl.0.is_null() || unsafe { acl_set_fd_np(file.as_raw_fd(), acl.0, 0x100) } != 0 {
                return Err(invalid());
            }
            empty(file)
        }
        #[cfg(test)]
        mod tests {
            use super::*;
            #[test]
            fn inherited_acl_grants_are_removed_before_private_writes() {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().join("empty-private-member");
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .unwrap();
                let status = std::process::Command::new("/bin/chmod")
                    .args(["+a", "everyone allow read,write"])
                    .arg(&path)
                    .status()
                    .unwrap();
                assert!(status.success());
                assert!(empty(&file).is_err());
                strip(&file).unwrap();
                empty(&file).unwrap();
                assert_eq!(file.metadata().unwrap().len(), 0);
            }
        }
    }
    fn name(value: &OsStr) -> BackupResult<CString> {
        CString::new(value.as_bytes()).map_err(|_| invalid())
    }
    pub(super) fn open_at(dir: &File, entry: &OsStr, flags: i32, mode: u32) -> BackupResult<File> {
        let entry = name(entry)?;
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                entry.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                // openat is variadic: Darwin mode_t is u16 and must undergo
                // C's integer promotion before crossing the ABI boundary.
                mode as libc::c_uint,
            )
        };
        if fd < 0 {
            Err(invalid())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    pub(super) fn directory(path: &Path) -> BackupResult<File> {
        // Traverse every component from an already opened root/cwd. Never follow
        // a symlink in an ancestor, and never re-resolve an anchored directory.
        let mut dir =
            File::open(if path.is_absolute() { "/" } else { "." }).map_err(|_| invalid())?;
        for component in path.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(part) => {
                    dir = open_at(&dir, part, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
                }
                _ => return Err(invalid()),
            }
        }
        Ok(dir)
    }
    pub(super) fn regular(dir: &File, entry: &str, maximum: usize) -> BackupResult<Vec<u8>> {
        // NONBLOCK prevents a swapped FIFO from hanging before fstat rejects it.
        let file = open_at(dir, OsStr::new(entry), libc::O_RDONLY, 0)?;
        let metadata = file.metadata().map_err(|_| invalid())?;
        if !metadata.is_file() || metadata.len() > maximum as u64 {
            return Err(invalid());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(maximum as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid())?;
        if bytes.len() > maximum || bytes.len() as u64 != metadata.len() {
            return Err(invalid());
        }
        Ok(bytes)
    }
    pub(super) fn read(dir: &File) -> BackupResult<(Vec<u8>, BTreeMap<String, Vec<u8>>)> {
        // A distinct open description resets the directory offset on each read.
        let scan = open_at(dir, OsStr::new("."), libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        use std::os::fd::IntoRawFd;
        let fd = scan.into_raw_fd();
        let stream = unsafe { libc::fdopendir(fd) };
        if stream.is_null() {
            unsafe {
                libc::close(fd);
            }
            return Err(invalid());
        }
        struct Stream(*mut libc::DIR);
        impl Drop for Stream {
            fn drop(&mut self) {
                unsafe {
                    libc::closedir(self.0);
                }
            }
        }
        let stream = Stream(stream);
        let mut names = std::collections::BTreeSet::new();
        loop {
            // readdir signals both EOF and failure with null; preserve failures.
            #[cfg(target_os = "linux")]
            let errno = unsafe { libc::__errno_location() };
            #[cfg(target_os = "macos")]
            let errno = unsafe { libc::__error() };
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            unsafe {
                *errno = 0;
            }
            let item = unsafe { libc::readdir(stream.0) };
            if item.is_null() {
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                if unsafe { *errno } != 0 {
                    return Err(invalid());
                }
                break;
            }
            let raw = unsafe { CStr::from_ptr((*item).d_name.as_ptr()) }.to_bytes();
            if raw == b"." || raw == b".." {
                continue;
            }
            let entry = OsStr::from_bytes(raw);
            if !allowed_name(entry) || !names.insert(raw.to_vec()) || names.len() > ENTRY_COUNT {
                return Err(invalid());
            }
        }
        if names.len() != ENTRY_COUNT {
            return Err(invalid());
        }
        let manifest = regular(dir, MANIFEST, MANIFEST_LIMIT)?;
        let mut files = BTreeMap::new();
        for (entry, maximum) in FILES {
            files.insert(entry.to_owned(), regular(dir, entry, maximum)?);
        }
        if regular(dir, MANIFEST, MANIFEST_LIMIT)? != manifest {
            return Err("Backup manifest changed during read".into());
        }
        Ok((manifest, files))
    }
    pub(super) fn children(dir: &File) -> BackupResult<Vec<(OsString, bool)>> {
        let scan = open_at(dir, OsStr::new("."), libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        use std::os::fd::IntoRawFd;
        let fd = scan.into_raw_fd();
        let stream = unsafe { libc::fdopendir(fd) };
        if stream.is_null() {
            unsafe {
                libc::close(fd);
            }
            return Err(invalid());
        }
        struct Stream(*mut libc::DIR);
        impl Drop for Stream {
            fn drop(&mut self) {
                unsafe {
                    libc::closedir(self.0);
                }
            }
        }
        let stream = Stream(stream);
        let mut out = Vec::new();
        loop {
            #[cfg(target_os = "linux")]
            let errno = unsafe { libc::__errno_location() };
            #[cfg(target_os = "macos")]
            let errno = unsafe { libc::__error() };
            unsafe {
                *errno = 0;
            }
            let item = unsafe { libc::readdir(stream.0) };
            if item.is_null() {
                if unsafe { *errno } != 0 {
                    return Err(invalid());
                }
                break;
            }
            let raw = unsafe { CStr::from_ptr((*item).d_name.as_ptr()) }.to_bytes();
            if raw == b"." || raw == b".." {
                continue;
            }
            if out.len() >= HISTORY_ENTRY_LIMIT {
                return Err("Backup history contains too many entries".into());
            }
            let name = OsStr::from_bytes(raw).to_os_string();
            let is_dir = open_at(dir, &name, libc::O_RDONLY | libc::O_DIRECTORY, 0).is_ok();
            out.push((name, is_dir));
        }
        Ok(out)
    }
    pub(super) struct Stage {
        pub parent: File,
        pub dir: File,
        pub name: CString,
        published: bool,
    }
    impl Stage {
        pub(super) fn new(parent: File) -> BackupResult<Self> {
            use std::os::unix::fs::MetadataExt;
            use std::sync::atomic::{AtomicU64, Ordering};
            // A directory writable by another user could substitute the stage
            // name before publication. Require an owner-controlled parent.
            #[cfg(target_os = "macos")]
            privacy::empty(&parent)?;
            let metadata = parent.metadata().map_err(|_| invalid())?;
            if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
                return Err("Backup destination parent must be owned by the current user and not writable by group or others".into());
            }
            static NEXT: AtomicU64 = AtomicU64::new(0);
            for _ in 0..128 {
                let entry = CString::new(format!(
                    ".ak820-pending-{}-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|_| invalid())?
                        .as_nanos(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ))
                .unwrap();
                if unsafe { libc::mkdirat(parent.as_raw_fd(), entry.as_ptr(), 0o700) } != 0 {
                    continue;
                }
                // Install cleanup immediately: a restrictive umask can make the
                // newly created directory impossible to open even for its owner.
                struct Pending<'a> {
                    parent: &'a File,
                    name: &'a CStr,
                    armed: bool,
                }
                impl Drop for Pending<'_> {
                    fn drop(&mut self) {
                        if self.armed {
                            unsafe {
                                libc::unlinkat(
                                    self.parent.as_raw_fd(),
                                    self.name.as_ptr(),
                                    libc::AT_REMOVEDIR,
                                );
                            }
                        }
                    }
                }
                let mut pending = Pending {
                    parent: &parent,
                    name: &entry,
                    armed: true,
                };
                let dir = open_at(
                    &parent,
                    OsStr::from_bytes(entry.to_bytes()),
                    libc::O_RDONLY | libc::O_DIRECTORY,
                    0,
                ).map_err(|_| "Backup staging directory cannot be opened; restrictive owner-bit umasks are unsupported".to_owned())?;
                // Explicitly set permissions even under unusual process umasks.
                if unsafe { libc::fchmod(dir.as_raw_fd(), 0o700) } != 0 {
                    return Err(invalid());
                }
                #[cfg(target_os = "macos")]
                privacy::strip(&dir)?;
                pending.armed = false;
                drop(pending);
                return Ok(Self {
                    parent,
                    dir,
                    name: entry,
                    published: false,
                });
            }
            Err(invalid())
        }
        pub(super) fn write(&self, entry: &str, bytes: &[u8]) -> BackupResult<()> {
            let mut file = open_at(
                &self.dir,
                OsStr::new(entry),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
                0o600,
            )?;
            // Creation permissions are filtered by umask. Normalize through the
            // already-open exclusive file handle before writing private bytes.
            if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0 {
                return Err(invalid());
            }
            #[cfg(target_os = "macos")]
            privacy::strip(&file)?;
            file.write_all(bytes).map_err(|_| invalid())?;
            file.sync_all().map_err(|_| invalid())
        }
        pub(super) fn publish(&mut self, target: &OsStr) -> BackupResult<()> {
            let target = name(target)?;
            #[cfg(target_os = "linux")]
            let result = unsafe {
                libc::syscall(
                    libc::SYS_renameat2,
                    self.parent.as_raw_fd(),
                    self.name.as_ptr(),
                    self.parent.as_raw_fd(),
                    target.as_ptr(),
                    libc::RENAME_NOREPLACE,
                )
            };
            #[cfg(target_os = "macos")]
            let result = unsafe {
                libc::renameatx_np(
                    self.parent.as_raw_fd(),
                    self.name.as_ptr(),
                    self.parent.as_raw_fd(),
                    target.as_ptr(),
                    libc::RENAME_EXCL,
                )
            };
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            let result = -1;
            if result != 0 {
                return Err(invalid());
            }
            self.published = true;
            Ok(())
        }
    }
    impl Drop for Stage {
        fn drop(&mut self) {
            if self.published {
                return;
            }
            for entry in std::iter::once(MANIFEST).chain(FILES.iter().map(|(entry, _)| *entry)) {
                if let Ok(entry) = CString::new(entry) {
                    unsafe {
                        libc::unlinkat(self.dir.as_raw_fd(), entry.as_ptr(), 0);
                    }
                }
            }
            unsafe {
                libc::unlinkat(
                    self.parent.as_raw_fd(),
                    self.name.as_ptr(),
                    libc::AT_REMOVEDIR,
                );
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn read_directory(path: &Path) -> BackupResult<(Vec<u8>, BTreeMap<String, Vec<u8>>)> {
    anchored::read(&anchored::directory(path)?)
}
#[cfg(target_os = "windows")]
fn read_directory(path: &Path) -> BackupResult<(Vec<u8>, BTreeMap<String, Vec<u8>>)> {
    windows::read(&windows::directory(path)?)
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn read_directory(_path: &Path) -> BackupResult<(Vec<u8>, BTreeMap<String, Vec<u8>>)> {
    Err("Backup directory I/O is unsupported on this platform: anchored no-follow handles are unavailable".into())
}

/// Read and validate an exact firmware-compatible backup directory.
/// All file names and lengths are bounded before allocation; errors omit paths
/// and file contents because ROM and macro bytes are private.
pub fn read_validated_archive(path: &Path, profile: ProfileId) -> BackupResult<ValidatedArchive> {
    let (manifest, files) = read_directory(path)?;
    validate_profile_archive(&manifest, &files, profile, None)
        .map_err(|_| "Backup validation failed".to_owned())
}

const PROFILES: [ProfileId; 10] = [
    ProfileId::Main,
    ProfileId::Game,
    ProfileId::Mmo,
    ProfileId::Typing,
    ProfileId::Code,
    ProfileId::Pointer,
    ProfileId::Onehand,
    ProfileId::Creative,
    ProfileId::Shortcuts,
    ProfileId::Multios,
];

/// Reopen and fully validate a current-profile or exact historical backup.
/// The optional digest pins the manifest across catalog listing and use.
pub(crate) fn read_backup_auto(
    path: &Path,
    expected_manifest: Option<&str>,
) -> BackupResult<LoadedBackup> {
    let (raw, files) = read_directory(path)?;
    validate_loaded_bytes(raw, files, expected_manifest)
}

fn validate_loaded_bytes(
    raw: Vec<u8>,
    files: BTreeMap<String, Vec<u8>>,
    expected_manifest: Option<&str>,
) -> BackupResult<LoadedBackup> {
    let manifest: Value = serde_json::from_slice(&raw).map_err(|_| invalid())?;
    let fields = manifest.as_object().ok_or_else(invalid)?;
    let text_field = |key: &str| fields.get(key).and_then(Value::as_str).ok_or_else(invalid);
    let schema = fields
        .get("nvm_schema")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(invalid)?;
    let ihx = text_field("source_ihx_sha256")?;
    let code = text_field("code_sha256")?;
    let definition = text_field("definition_sha256")?;
    let mut found: Option<(ValidatedArchive, Option<ProfileId>)> = None;
    for profile in PROFILES {
        let Some(id) = matrix_profile(profile).settings_identity.as_ref() else {
            continue;
        };
        if id.ihx_sha256 == ihx
            && id.code_sha256 == code
            && id.definition_sha256 == definition
            && id.nvm_schema == schema
        {
            let archive = validate_profile_archive(&raw, &files, profile, expected_manifest)
                .map_err(|_| "Backup validation failed".to_owned())?;
            found = Some((archive, Some(profile)));
            break;
        }
    }
    if found.is_none() {
        let historical = historical_settings_identity(ihx, code, schema)
            .ok_or_else(|| "Backup validation failed".to_owned())?;
        let identity = ImageIdentity {
            ihx_sha256: historical.ihx_sha256.into(),
            code_sha256: historical.code_sha256.into(),
            definition_sha256: definition.into(),
            schema,
        };
        let archive = validate_historical_archive(&raw, &files, &identity, expected_manifest)
            .map_err(|_| "Backup validation failed".to_owned())?;
        found = Some((archive, None));
    }
    let (archive, source_profile) = found.expect("validated branch sets result");
    Ok(LoadedBackup {
        archive: Arc::new(archive),
        manifest_raw: Arc::new(raw),
        files: Arc::new(files),
        source_profile,
    })
}

/// Scan immediate child directories only. Every advertised candidate has
/// passed complete archive validation; malformed entries are counted.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn scan_backup_history(root: &Path) -> BackupResult<BackupHistory> {
    let root_dir = anchored::directory(root)?;
    scan_open_history(root, &root_dir)
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn scan_open_history(root: &Path, root_dir: &File) -> BackupResult<BackupHistory> {
    let children = anchored::children(root_dir)?;
    let mut entries = Vec::new();
    let mut invalid_skipped = 0usize;
    let mut backups_seen = 0usize;
    for (name, is_dir) in children {
        if !is_dir {
            continue;
        }
        backups_seen += 1;
        if backups_seen > HISTORY_LIMIT {
            return Err("Backup history exceeds the 128 backup limit".into());
        }
        let path = root.join(&name);
        let loaded = anchored::open_at(&root_dir, &name, libc::O_RDONLY | libc::O_DIRECTORY, 0)
            .and_then(|child| anchored::read(&child))
            .and_then(|(raw, files)| validate_loaded_bytes(raw, files, None));
        match loaded {
            Ok(loaded) => {
                let name = name.to_string_lossy().into_owned();
                let created_utc = loaded.archive().manifest["created_utc"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned();
                entries.push(BackupHistoryEntry {
                    name,
                    path,
                    created_utc,
                    source_profile: loaded.source_profile(),
                    schema: loaded.archive().source.schema,
                    manifest_sha256: loaded.archive().manifest_sha256.clone(),
                });
            }
            Err(_) => invalid_skipped += 1,
        }
    }
    Ok(BackupHistory {
        entries,
        invalid_skipped,
        warning: (invalid_skipped > 0).then(|| {
            format!(
                "Skipped {invalid_skipped} invalid backup entr{}",
                if invalid_skipped == 1 { "y" } else { "ies" }
            )
        }),
    })
}
#[cfg(target_os = "windows")]
pub(crate) fn scan_backup_history(root: &Path) -> BackupResult<BackupHistory> {
    windows::history(root)
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) fn scan_backup_history(_root: &Path) -> BackupResult<BackupHistory> {
    Err("Backup directory I/O is unsupported on this platform: anchored no-follow handles are unavailable".into())
}

fn make_manifest(profile: ProfileId, files: &BTreeMap<String, Vec<u8>>) -> BackupResult<Vec<u8>> {
    let identity = matrix_profile(profile)
        .settings_identity
        .as_ref()
        .ok_or_else(|| "Profile has no qualified settings identity".to_owned())?;
    let schema = identity.nvm_schema;
    let records: serde_json::Map<String, Value> = files
        .iter()
        .map(|(name, bytes)| {
            (
                name.clone(),
                json!({"size": bytes.len(), "sha256": sha256(bytes)}),
            )
        })
        .collect();
    let manifest = json!({
        "format": if schema == 0x082003 { FORMAT_V2 } else { FORMAT },
        "created_utc": chrono::Utc::now().to_rfc3339(),
        "device": {"vid": 0x320f, "pid": 0x505b, "vial_uid": "0470ff9001b915d3"},
        "definition_sha256": identity.definition_sha256,
        "nvm_schema": schema,
        "source_build_id": schema,
        "source_ihx_sha256": identity.ihx_sha256,
        "code_sha256": identity.code_sha256,
        "counts": {"layers": 6, "physical_keys": 83, "assignments": 498,
            "encoder_assignments": 12, "macros": 16, "macro_capacity": 8192},
        "scope": "saved supported settings and exposed RGB values; not all current RAM",
        "files": records,
    });
    canonical(&manifest)
}

fn check_map(files: &BTreeMap<String, Vec<u8>>) -> BackupResult<()> {
    if files.len() != FILES.len() {
        return Err("Unexpected backup files".into());
    }
    for (name, limit) in FILES {
        let data = files.get(name).ok_or("Missing backup file")?;
        if data.len() > limit {
            return Err("Oversized backup file".into());
        }
    }
    Ok(())
}

/// Publish only a complete, catalog-validated map to a previously absent path.
/// The entire directory appears in one no-replace rename; failures clean the
/// private staging directory and leave an existing destination untouched.
pub fn publish_validated_archive(
    path: &Path,
    profile: ProfileId,
    files: &BTreeMap<String, Vec<u8>>,
) -> BackupResult<ValidatedArchive> {
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (path, profile, files);
        Err("Backup directory I/O is unsupported on this platform: anchored no-follow handles are unavailable".into())
    }
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        #[cfg(target_os = "windows")]
        use windows as anchored;
        check_map(files)?;
        let manifest = make_manifest(profile, files)?;
        validate_profile_archive(&manifest, files, profile, None)
            .map_err(|_| "Backup validation failed".to_owned())?;
        let target = path.file_name().ok_or_else(invalid)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        // The destination parent must exist; creating ancestor paths would require
        // a separate anchored creation contract.
        let mut stage = anchored::Stage::new(anchored::directory(parent)?)?;
        for (entry, bytes) in files {
            stage.write(entry, bytes)?;
        }
        stage.write(MANIFEST, &manifest)?;
        let (stored_manifest, stored_files) = anchored::read(&stage.dir)?;
        let verified = validate_profile_archive(&stored_manifest, &stored_files, profile, None)
            .map_err(|_| "Backup validation failed".to_owned())?;
        #[cfg(not(target_os = "windows"))]
        stage.dir.sync_all().map_err(|_| invalid())?;
        stage.publish(target)?;
        #[cfg(not(target_os = "windows"))]
        stage.parent.sync_all().map_err(|_| invalid())?;
        Ok(verified)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    fn envelope(dir: &Path) {
        fs::write(dir.join(MANIFEST), b"{}").unwrap();
        for (name, _) in FILES {
            fs::write(dir.join(name), b"x").unwrap();
        }
    }
    #[test]
    fn bounded_reader_rejects_extra_symlink_fifo_and_oversized_members() {
        let temp = tempfile::tempdir().unwrap();
        envelope(temp.path());
        assert!(read_directory(temp.path()).is_ok());
        fs::write(temp.path().join("extra"), b"x").unwrap();
        assert!(read_directory(temp.path()).is_err());
        fs::remove_file(temp.path().join("extra")).unwrap();
        let (entry, limit) = FILES[0];
        fs::write(temp.path().join(entry), vec![0; limit + 1]).unwrap();
        assert!(read_directory(temp.path()).is_err());
        fs::remove_file(temp.path().join(entry)).unwrap();
        symlink(MANIFEST, temp.path().join(entry)).unwrap();
        assert!(read_directory(temp.path()).is_err());
        fs::remove_file(temp.path().join(entry)).unwrap();
        use std::os::unix::ffi::OsStrExt;
        let fifo = std::ffi::CString::new(temp.path().join(entry).as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(read_directory(temp.path()).is_err());
    }
    #[test]
    fn history_is_immediate_bounded_and_skips_invalid_archives() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("history");
        fs::create_dir(&root).unwrap();
        let candidate = root.join("candidate");
        fs::create_dir(&candidate).unwrap();
        envelope(&candidate);
        let nested = candidate.join("nested");
        fs::create_dir(&nested).unwrap();
        envelope(&nested);
        fs::write(root.join("ordinary-file"), b"x").unwrap();
        let history = scan_backup_history(&root).unwrap();
        assert!(history.entries.is_empty());
        assert_eq!(history.invalid_skipped, 1);
        assert_eq!(
            history.warning.as_deref(),
            Some("Skipped 1 invalid backup entry")
        );
        fs::remove_dir_all(&candidate).unwrap();
        for i in 0..=HISTORY_LIMIT {
            fs::create_dir(root.join(format!("d{i}"))).unwrap();
        }
        assert!(scan_backup_history(&root)
            .unwrap_err()
            .contains("128 backup limit"));
    }
    #[test]
    fn history_scan_stays_anchored_after_root_rename_and_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("history");
        fs::create_dir(&root).unwrap();
        let original_entry = root.join("original-candidate");
        fs::create_dir(&original_entry).unwrap();
        envelope(&original_entry);
        let anchored_root = anchored::directory(&root).unwrap();
        let moved = temp.path().join("moved-history");
        fs::rename(&root, &moved).unwrap();
        fs::create_dir(&root).unwrap();
        let replacement_entry = root.join("replacement-candidate");
        fs::create_dir(&replacement_entry).unwrap();
        envelope(&replacement_entry);
        fs::write(replacement_entry.join(MANIFEST), b"{\"replacement\":true}").unwrap();
        let original_child = anchored::open_at(
            &anchored_root,
            std::ffi::OsStr::new("original-candidate"),
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )
        .unwrap();
        let (anchored_manifest, _) = anchored::read(&original_child).unwrap();
        let (replacement_manifest, _) = read_directory(&replacement_entry).unwrap();
        assert_eq!(anchored_manifest, b"{}");
        assert_eq!(replacement_manifest, b"{\"replacement\":true}");
        assert_ne!(anchored_manifest, replacement_manifest);
        let history = scan_open_history(&root, &anchored_root).unwrap();
        assert_eq!(history.invalid_skipped, 1);
        assert!(history.entries.is_empty());
    }
    #[test]
    fn automatic_loader_rejects_malformed_and_pinned_manifest_changes() {
        let temp = tempfile::tempdir().unwrap();
        envelope(temp.path());
        let pinned = "00".repeat(32);
        assert!(read_backup_auto(temp.path(), Some(&pinned)).is_err());
        assert!(read_backup_auto(temp.path(), None).is_err());
    }
    #[test]
    fn rejects_symlink_ancestors_and_keeps_open_directory_after_rename() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original");
        fs::create_dir(&original).unwrap();
        envelope(&original);
        symlink(&original, temp.path().join("link")).unwrap();
        assert!(read_directory(&temp.path().join("link")).is_err());
        let dir = anchored::directory(&original).unwrap();
        fs::rename(&original, temp.path().join("moved")).unwrap();
        fs::create_dir(&original).unwrap();
        assert!(anchored::read(&dir).is_ok());
        assert!(read_directory(&original).is_err());
    }
    #[test]
    fn staging_permissions_and_reread_are_explicit() {
        let temp = tempfile::tempdir().unwrap();
        let stage = anchored::Stage::new(anchored::directory(temp.path()).unwrap()).unwrap();
        assert_eq!(
            stage.dir.metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );
        stage.write(MANIFEST, b"first").unwrap();
        assert_eq!(
            anchored::regular(&stage.dir, MANIFEST, MANIFEST_LIMIT).unwrap(),
            b"first"
        );
        let file = anchored::open_at(
            &stage.dir,
            std::ffi::OsStr::new(MANIFEST),
            libc::O_RDONLY,
            0,
        )
        .unwrap();
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        assert!(stage.write(MANIFEST, b"overwrite").is_err());
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn restrictive_umasks_are_isolated_and_leave_no_stage_leaks() {
        const CHILD: &str = "AK820_BACKUP_UMASK_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            use std::os::unix::process::CommandExt;
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command.args([
                "--exact",
                "ak820_backup_files::tests::restrictive_umasks_are_isolated_and_leave_no_stage_leaks",
                "--nocapture",
            ]).env(CHILD, "1");
            // A privileged test process would hide missing owner permissions.
            if unsafe { libc::geteuid() } == 0 {
                command.gid(65534).uid(65534);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
            return;
        }
        assert_ne!(unsafe { libc::geteuid() }, 0);
        for mask in [0o000, 0o200, 0o400, 0o700] {
            let temp = tempfile::tempdir().unwrap();
            let parent = anchored::directory(temp.path()).unwrap();
            let old_mask = unsafe { libc::umask(mask) };
            let result = anchored::Stage::new(parent);
            unsafe {
                libc::umask(old_mask);
            }
            if mask & 0o400 != 0 {
                let error = match result {
                    Err(error) => error,
                    Ok(_) => panic!("Owner-read masked stage unexpectedly opened"),
                };
                assert!(error.contains("restrictive owner-bit umasks are unsupported"));
                assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
                continue;
            }
            let mut stage = result.unwrap();
            assert_eq!(
                stage.dir.metadata().unwrap().permissions().mode() & 0o777,
                0o700
            );
            // Keep owner-write masked during file creation, independently of
            // directory initialization, and check mandatory local reread.
            let old_mask = unsafe { libc::umask(0o600) };
            stage.write(MANIFEST, b"manifest").unwrap();
            for (name, _) in FILES {
                stage
                    .write(
                        name,
                        if name == ".gitignore" {
                            b"*\n"
                        } else {
                            b"private"
                        },
                    )
                    .unwrap();
            }
            unsafe {
                libc::umask(old_mask);
            }
            for name in std::iter::once(MANIFEST).chain(FILES.iter().map(|(name, _)| *name)) {
                let file =
                    anchored::open_at(&stage.dir, std::ffi::OsStr::new(name), libc::O_RDONLY, 0)
                        .unwrap();
                assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
            }
            let (manifest, files) = anchored::read(&stage.dir).unwrap();
            assert_eq!(manifest, b"manifest");
            assert!(files.iter().all(|(name, bytes)| bytes.as_slice()
                == if name == ".gitignore" {
                    &b"*\n"[..]
                } else {
                    &b"private"[..]
                }));
            let target = temp.path().join("published");
            assert!(!target.exists());
            stage.publish(target.file_name().unwrap()).unwrap();
            drop(stage);
            assert_eq!(read_directory(&target).unwrap(), (manifest, files));
            assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
        }
    }
    #[test]
    fn failed_staging_removes_only_its_private_files() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("sentinel"), b"keep").unwrap();
        {
            let stage = anchored::Stage::new(anchored::directory(temp.path()).unwrap()).unwrap();
            stage.write(MANIFEST, b"first").unwrap();
            let mut changed = anchored::open_at(
                &stage.dir,
                std::ffi::OsStr::new(MANIFEST),
                libc::O_WRONLY | libc::O_TRUNC,
                0,
            )
            .unwrap();
            changed.write_all(b"changed").unwrap();
            assert_eq!(
                anchored::regular(&stage.dir, MANIFEST, MANIFEST_LIMIT).unwrap(),
                b"changed"
            );
        }
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
        assert_eq!(fs::read(temp.path().join("sentinel")).unwrap(), b"keep");
    }
    #[test]
    fn staging_rejects_a_parent_writable_by_other_users() {
        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(anchored::Stage::new(anchored::directory(temp.path()).unwrap()).is_err());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn publication_never_replaces_empty_nonempty_file_or_link_targets() {
        let temp = tempfile::tempdir().unwrap();
        for kind in 0..4 {
            let target = temp.path().join(format!("target-{kind}"));
            match kind {
                0 => fs::create_dir(&target).unwrap(),
                1 => {
                    fs::create_dir(&target).unwrap();
                    fs::write(target.join("sentinel"), b"keep").unwrap();
                }
                2 => fs::write(&target, b"keep").unwrap(),
                _ => symlink("missing", &target).unwrap(),
            }
            let mut stage =
                anchored::Stage::new(anchored::directory(temp.path()).unwrap()).unwrap();
            stage.write(MANIFEST, b"stage").unwrap();
            assert!(stage.publish(target.file_name().unwrap()).is_err());
            match kind {
                0 => assert_eq!(fs::read_dir(&target).unwrap().count(), 0),
                1 => assert_eq!(fs::read(target.join("sentinel")).unwrap(), b"keep"),
                2 => assert_eq!(fs::read(&target).unwrap(), b"keep"),
                _ => assert_eq!(fs::read_link(&target).unwrap(), Path::new("missing")),
            }
        }
    }
}
