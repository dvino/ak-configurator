//! Windows archive I/O. All descendant operations use NT RootDirectory handles;
//! path strings are never resolved again after opening the selected directory.
//! Contracts: NtCreateFile / OBJECT_ATTRIBUTES / NtQueryDirectoryFile /
//! FILE_RENAME_INFORMATION (Microsoft WDK), CreateFile sharing (Microsoft SDK).
use super::*;
use std::ffi::{c_void, OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Component, Prefix};
use std::ptr::{null, null_mut};
type Handle = *mut c_void;
#[repr(C)]
struct UnicodeString {
    len: u16,
    max: u16,
    data: *mut u16,
}
#[repr(C)]
struct ObjectAttributes {
    len: u32,
    root: Handle,
    name: *mut UnicodeString,
    attributes: u32,
    security: *mut c_void,
    qos: *mut c_void,
}
#[repr(C)]
struct IoStatus {
    status: usize,
    information: usize,
}
#[repr(C)]
struct FileInfo {
    attributes: u32,
    creation: [u32; 2],
    access: [u32; 2],
    write: [u32; 2],
    volume: u32,
    size_high: u32,
    size_low: u32,
    links: u32,
    index_high: u32,
    index_low: u32,
}
#[link(name = "ntdll")]
extern "system" {
    fn NtCreateFile(
        out: *mut Handle,
        access: u32,
        attrs: *mut ObjectAttributes,
        io: *mut IoStatus,
        allocation: *const i64,
        attributes: u32,
        sharing: u32,
        disposition: u32,
        options: u32,
        ea: *const c_void,
        ea_len: u32,
    ) -> i32;
    fn NtQueryDirectoryFile(
        file: Handle,
        event: Handle,
        apc: *const c_void,
        context: *const c_void,
        io: *mut IoStatus,
        buffer: *mut c_void,
        length: u32,
        class: u32,
        single: u8,
        name: *const UnicodeString,
        restart: u8,
    ) -> i32;
    fn NtSetInformationFile(
        file: Handle,
        io: *mut IoStatus,
        information: *const c_void,
        length: u32,
        class: u32,
    ) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetFileInformationByHandle(file: Handle, info: *mut FileInfo) -> i32;
    fn GetCurrentProcess() -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
    fn LocalFree(memory: Handle) -> Handle;
}
#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(
        token: Handle,
        class: u32,
        info: *mut c_void,
        length: u32,
        needed: *mut u32,
    ) -> i32;
    fn GetSecurityInfo(
        handle: Handle,
        kind: u32,
        info: u32,
        owner: *mut *mut c_void,
        group: *mut *mut c_void,
        dacl: *mut *mut c_void,
        sacl: *mut *mut c_void,
        sd: *mut *mut c_void,
    ) -> u32;
    fn GetSecurityDescriptorDacl(
        sd: *const c_void,
        present: *mut i32,
        acl: *mut *mut c_void,
        defaulted: *mut i32,
    ) -> i32;
    fn GetSecurityDescriptorOwner(
        sd: *const c_void,
        owner: *mut *mut c_void,
        defaulted: *mut i32,
    ) -> i32;
    fn GetSecurityDescriptorControl(
        sd: *const c_void,
        control: *mut u16,
        revision: *mut u32,
    ) -> i32;
    fn EqualSid(a: *const c_void, b: *const c_void) -> i32;
    fn ConvertSidToStringSidW(sid: *const c_void, string: *mut *mut u16) -> i32;
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        string: *const u16,
        revision: u32,
        sd: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
}
struct Local(Handle);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn wide(value: &OsStr) -> BackupResult<Vec<u16>> {
    let out: Vec<_> = value.encode_wide().collect();
    if out.is_empty() || out.len() > 32766 || out.contains(&0) {
        return Err(invalid());
    }
    Ok(out)
}
fn component(value: &OsStr) -> BackupResult<Vec<u16>> {
    let out = wide(value)?;
    // Reject Win32 aliases, alternate streams, separators and DOS device names.
    if out.iter().any(|c| matches!(*c, 47 | 92 | 58))
        || matches!(out.last(), Some(32 | 46))
        || value == "."
        || value == ".."
    {
        return Err(invalid());
    }
    let stem = value
        .to_string_lossy()
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit())
    {
        return Err(invalid());
    }
    Ok(out)
}
#[derive(Debug, PartialEq, Eq)]
enum OpenError {
    Collision,
    Failed,
}
impl From<String> for OpenError {
    fn from(_: String) -> Self {
        Self::Failed
    }
}
fn classify_open_failure(status: i32) -> OpenError {
    if status as u32 == 0xc0000035 {
        OpenError::Collision
    } else {
        OpenError::Failed
    }
}
fn open(
    parent: &File,
    entry: &OsStr,
    directory: bool,
    create: bool,
    security: *mut c_void,
    delete: bool,
) -> BackupResult<File> {
    open_internal(parent, entry, directory, create, security, delete).map_err(|_| invalid())
}
fn open_internal(
    parent: &File,
    entry: &OsStr,
    directory: bool,
    create: bool,
    security: *mut c_void,
    delete: bool,
) -> Result<File, OpenError> {
    let mut text = component(entry)?;
    let mut name = UnicodeString {
        len: (text.len() * 2) as u16,
        max: (text.len() * 2) as u16,
        data: text.as_mut_ptr(),
    };
    let mut attrs = ObjectAttributes {
        len: std::mem::size_of::<ObjectAttributes>() as u32,
        root: parent.as_raw_handle(),
        name: &mut name,
        attributes: 0x40,
        security,
        qos: null_mut(),
    }; // OBJ_CASE_INSENSITIVE; each name has exactly one component
    let mut io = IoStatus {
        status: 0,
        information: 0,
    };
    let mut handle = null_mut();
    let access = 0x00100000
        | 0x00020000
        | 0x80
        | 1
        | if create && !directory { 0x00120116 } else { 0 }
        | if delete { 0x10000 } else { 0 };
    // Synchronous handle, final reparse point opened itself, expected object type.
    let options = 0x20 | 0x00200000 | if directory { 1 } else { 0x40 };
    let result = unsafe {
        NtCreateFile(
            &mut handle,
            access,
            &mut attrs,
            &mut io,
            null(),
            0,
            if directory { 3 } else { 1 },
            if create { 2 } else { 1 },
            options,
            null(),
            0,
        )
    };
    if result < 0 {
        return Err(classify_open_failure(result));
    }
    let file = unsafe { File::from_raw_handle(handle) };
    let valid = file
        .metadata()
        .map(|metadata| {
            metadata.file_attributes() & 0x400 == 0
                && metadata.is_dir() == directory
                && (directory || metadata.is_file())
        })
        .unwrap_or(false);
    if !valid {
        // A successful exclusive directory create owns this exact empty object.
        // Dispose through its DELETE-capable handle before returning failure;
        // never attempt cleanup through its potentially stale path/name.
        if create && directory && delete {
            let _ = remove(&file);
        }
        return Err(OpenError::Failed);
    }
    Ok(file)
}
pub(super) fn directory(path: &Path) -> BackupResult<File> {
    // Resolve relative input once, then traverse from a local drive root. UNC,
    // device paths and drive-relative paths have no accepted namespace contract.
    if !path.is_absolute()
        && path
            .components()
            .any(|part| matches!(part, Component::Prefix(_) | Component::RootDir))
    {
        return Err(invalid());
    }
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(|_| invalid())?.join(path)
    };
    let mut parts = full.components();
    let drive = match parts.next() {
        Some(Component::Prefix(p)) => match p.kind() {
            Prefix::Disk(d) => d,
            _ => return Err(invalid()),
        },
        _ => return Err(invalid()),
    };
    if parts.next() != Some(Component::RootDir) {
        return Err(invalid());
    }
    let root = format!("{}:\\", char::from(drive));
    let mut dir = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(0x02000000 | 0x00200000)
        .open(root)
        .map_err(|_| invalid())?;
    if dir.metadata().map_err(|_| invalid())?.file_attributes() & 0x400 != 0 {
        return Err(invalid());
    }
    for part in parts {
        match part {
            Component::Normal(name) => dir = open(&dir, name, true, false, null_mut(), false)?,
            Component::CurDir => {}
            _ => return Err(invalid()),
        }
    }
    Ok(dir)
}
fn names(dir: &File, limit: usize) -> BackupResult<Vec<OsString>> {
    let mut out = Vec::new();
    let mut buffer = vec![0u64; 8192]; // aligned, fixed 64 KiB directory response
    let mut restart = 1;
    loop {
        let mut io = IoStatus {
            status: 0,
            information: 0,
        };
        let status = unsafe {
            NtQueryDirectoryFile(
                dir.as_raw_handle(),
                null_mut(),
                null(),
                null(),
                &mut io,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                12,
                1,
                null(),
                restart,
            )
        }; // FileNamesInformation
        restart = 0;
        if status as u32 == 0x80000006 {
            break;
        } // STATUS_NO_MORE_FILES
        if status < 0 || io.information < 12 || io.information > buffer.len() * 8 {
            return Err(invalid());
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), io.information) };
        let length = u32::from_ne_bytes(bytes[8..12].try_into().unwrap()) as usize;
        if length % 2 != 0 || length > bytes.len() - 12 {
            return Err(invalid());
        }
        let units: Vec<u16> = bytes[12..12 + length]
            .chunks_exact(2)
            .map(|p| u16::from_ne_bytes([p[0], p[1]]))
            .collect();
        let name = OsString::from_wide(&units);
        if name == "." || name == ".." {
            continue;
        }
        if out.len() >= limit {
            return Err("Backup directory contains too many entries".into());
        }
        component(&name)?;
        if out.contains(&name) {
            return Err(invalid());
        }
        out.push(name);
    }
    Ok(out)
}
pub(super) fn regular(dir: &File, entry: &str, maximum: usize) -> BackupResult<Vec<u8>> {
    let file = open(dir, OsStr::new(entry), false, false, null_mut(), false)?;
    let mut info: FileInfo = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0
        || info.links != 1
    {
        return Err(invalid());
    }
    let size = (u64::from(info.size_high) << 32) | u64::from(info.size_low);
    if size > maximum as u64 {
        return Err(invalid());
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() as u64 != size {
        return Err(invalid());
    }
    Ok(bytes)
}
pub(super) fn read(dir: &File) -> BackupResult<(Vec<u8>, BTreeMap<String, Vec<u8>>)> {
    let entries = names(dir, ENTRY_COUNT)?;
    if entries.len() != ENTRY_COUNT || entries.iter().any(|name| !allowed_name(name)) {
        return Err(invalid());
    }
    let manifest = regular(dir, MANIFEST, MANIFEST_LIMIT)?;
    let mut files = BTreeMap::new();
    for (entry, limit) in FILES {
        files.insert(entry.into(), regular(dir, entry, limit)?);
    }
    if regular(dir, MANIFEST, MANIFEST_LIMIT)? != manifest {
        return Err("Backup manifest changed during read".into());
    }
    Ok((manifest, files))
}
pub(super) fn history(root: &Path) -> BackupResult<BackupHistory> {
    let root_dir = directory(root)?;
    let mut entries = Vec::new();
    let mut invalid_skipped = 0;
    let mut seen = 0;
    for name in names(&root_dir, HISTORY_ENTRY_LIMIT)? {
        let child = match open(&root_dir, &name, true, false, null_mut(), false) {
            Ok(child) => child,
            Err(_) => {
                invalid_skipped += 1;
                continue;
            }
        };
        seen += 1;
        if seen > HISTORY_LIMIT {
            return Err("Backup history exceeds the 128 backup limit".into());
        }
        match read(&child).and_then(|(raw, files)| validate_loaded_bytes(raw, files, None)) {
            Ok(loaded) => entries.push(BackupHistoryEntry {
                name: name.to_string_lossy().into(),
                path: root.join(name),
                created_utc: loaded.archive().manifest["created_utc"]
                    .as_str()
                    .unwrap_or("")
                    .into(),
                source_profile: loaded.source_profile(),
                schema: loaded.archive().source.schema,
                manifest_sha256: loaded.archive().manifest_sha256.clone(),
            }),
            Err(_) => invalid_skipped += 1,
        }
    }
    Ok(BackupHistory {
        entries,
        invalid_skipped,
        warning: (invalid_skipped > 0)
            .then(|| format!("Skipped {invalid_skipped} invalid backup entries")),
    })
}
fn private_descriptor() -> BackupResult<Local> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), 8, &mut token) } == 0 {
        return Err(invalid());
    }
    struct Token(Handle);
    impl Drop for Token {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    let token = Token(token);
    let mut needed = 0;
    unsafe {
        GetTokenInformation(token.0, 1, null_mut(), 0, &mut needed);
    }
    if needed == 0 || needed > 65536 {
        return Err(invalid());
    }
    let mut data = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
    if unsafe { GetTokenInformation(token.0, 1, data.as_mut_ptr().cast(), needed, &mut needed) }
        == 0
    {
        return Err(invalid());
    }
    let sid = data[0] as *const c_void;
    let mut text = null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(invalid());
    }
    let sid_text = Local(text.cast());
    let mut length = 0;
    while length < 256 && unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    if length == 256 {
        return Err(invalid());
    }
    let sid = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
        .map_err(|_| invalid())?;
    drop(sid_text);
    // Explicit owner and protected DACL. OI/CI propagate only these two grants.
    let sddl = format!("O:{sid}D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;{sid})");
    let sddl: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut sd = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), 1, &mut sd, null_mut())
    } == 0
    {
        return Err(invalid());
    }
    Ok(Local(sd))
}
// Check the on-disk ACL rather than assuming the filesystem implemented SDDL.
fn verify_private(file: &File, expected: &Local, protected: bool) -> BackupResult<()> {
    let (mut owner, mut actual_acl, mut actual_sd) = (null_mut(), null_mut(), null_mut());
    if unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            1,
            5,
            &mut owner,
            null_mut(),
            &mut actual_acl,
            null_mut(),
            &mut actual_sd,
        )
    } != 0
    {
        return Err(invalid());
    }
    let actual_sd = Local(actual_sd);
    let (mut expected_owner, mut expected_acl) = (null_mut(), null_mut());
    let (mut defaulted, mut present) = (0, 0);
    let (mut control, mut revision) = (0, 0);
    if unsafe { GetSecurityDescriptorOwner(expected.0, &mut expected_owner, &mut defaulted) } == 0
        || unsafe { EqualSid(owner, expected_owner) } == 0
        || unsafe {
            GetSecurityDescriptorDacl(expected.0, &mut present, &mut expected_acl, &mut defaulted)
        } == 0
        || present == 0
        || actual_acl.is_null()
        || expected_acl.is_null()
        || unsafe { GetSecurityDescriptorControl(actual_sd.0, &mut control, &mut revision) } == 0
        || (protected && control & 0x1000 == 0)
    {
        return Err(invalid());
    }
    // Compare ACE grants, allowing only inheritance bookkeeping flags to differ.
    unsafe {
        let actual = actual_acl.cast::<u8>();
        let expected = expected_acl.cast::<u8>();
        let actual_size = u16::from_le_bytes([*actual.add(2), *actual.add(3)]) as usize;
        let expected_size = u16::from_le_bytes([*expected.add(2), *expected.add(3)]) as usize;
        if actual_size != expected_size || actual_size < 8 {
            return Err(invalid());
        }
        let mut a = std::slice::from_raw_parts(actual, actual_size).to_vec();
        let mut b = std::slice::from_raw_parts(expected, expected_size).to_vec();
        let count = u16::from_le_bytes([a[4], a[5]]) as usize;
        if count != 2 || a[4..6] != b[4..6] {
            return Err(invalid());
        }
        let mut offset = 8;
        for _ in 0..count {
            if offset + 4 > a.len() {
                return Err(invalid());
            }
            let length = u16::from_le_bytes([a[offset + 2], a[offset + 3]]) as usize;
            if length < 4 || offset + length > a.len() {
                return Err(invalid());
            }
            // Children may inherit OI/CI differently, but no deny/grant bits or
            // SID/mask may differ. All other ACE flags must remain identical.
            a[offset + 1] &= !0x13;
            b[offset + 1] &= !0x13;
            offset += length;
        }
        if a != b {
            return Err(invalid());
        }
    }
    Ok(())
}
#[repr(C)]
struct Rename {
    replace: u32,
    root: Handle,
    length: u32,
    name: [u16; 1],
}
fn rename(file: &File, parent: &File, name: &OsStr) -> BackupResult<()> {
    let name = component(name)?;
    let offset = std::mem::offset_of!(Rename, name);
    let size = (offset + name.len() * 2).max(std::mem::size_of::<Rename>());
    let mut buffer = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<Rename>();
    unsafe {
        (*info).replace = 0;
        (*info).root = parent.as_raw_handle();
        (*info).length = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*info).name).cast(),
            name.len(),
        );
    }
    let mut io = IoStatus {
        status: 0,
        information: 0,
    };
    if unsafe { NtSetInformationFile(file.as_raw_handle(), &mut io, info.cast(), size as u32, 10) }
        < 0
    {
        return Err(invalid());
    }
    Ok(())
}
fn remove(file: &File) -> BackupResult<()> {
    let delete: u8 = 1;
    let mut io = IoStatus {
        status: 0,
        information: 0,
    };
    if unsafe {
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut io,
            (&delete as *const u8).cast(),
            1,
            13,
        )
    } < 0
    {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn delete_validated(path: &Path, expected_manifest: &str) -> BackupResult<()> {
    let name = path.file_name().ok_or_else(invalid)?;
    let parent = directory(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let dir = open(&parent, name, true, false, null_mut(), true)?;
    let (manifest, files) = read(&dir)?;
    super::validate_loaded_bytes(manifest.clone(), files.clone(), Some(expected_manifest))?;
    for (entry, bytes) in &files {
        if regular(&dir, entry, bytes.len())? != *bytes {
            return Err("Backup changed before deletion".into());
        }
    }
    if regular(&dir, MANIFEST, MANIFEST_LIMIT)? != manifest {
        return Err("Backup changed before deletion".into());
    }
    for name in FILES
        .iter()
        .map(|(name, _)| *name)
        .chain(std::iter::once(MANIFEST))
    {
        let file = open(&dir, OsStr::new(name), false, false, null_mut(), true)?;
        remove(&file)
            .map_err(|_| "Backup deletion stopped; inspect the remaining folder".to_owned())?;
    }
    remove(&dir)
        .map_err(|_| "Backup files were removed but the folder could not be removed".to_owned())
}
pub(super) struct Stage {
    pub parent: File,
    pub dir: File,
    published: bool,
}
impl Stage {
    pub(super) fn new(parent: File) -> BackupResult<Self> {
        let sd = private_descriptor()?;
        // Kernel chooses an absent name atomically; collisions never reopen it.
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let name = format!(
                ".ak820-pending-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| invalid())?
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let dir = match open_internal(&parent, OsStr::new(&name), true, true, sd.0, true) {
                Ok(dir) => dir,
                Err(OpenError::Collision) => continue,
                Err(OpenError::Failed) => return Err(invalid()),
            };
            {
                let stage = Self {
                    parent,
                    dir,
                    published: false,
                };
                verify_private(&stage.dir, &sd, true)?;
                return Ok(stage);
            }
        }
        Err(invalid())
    }
    pub(super) fn write(&self, name: &str, bytes: &[u8]) -> BackupResult<()> {
        let sd = private_descriptor()?;
        let mut file = open(&self.dir, OsStr::new(name), false, true, sd.0, false)?;
        verify_private(&file, &sd, true)?;
        file.write_all(bytes).map_err(|_| invalid())?;
        file.sync_all().map_err(|_| invalid())
    }
    pub(super) fn publish(&mut self, target: &OsStr) -> BackupResult<()> {
        rename(&self.dir, &self.parent, target)?;
        self.published = true;
        Ok(())
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if self.published {
            return;
        }
        for name in std::iter::once(MANIFEST).chain(FILES.iter().map(|(name, _)| *name)) {
            if let Ok(file) = open(&self.dir, OsStr::new(name), false, false, null_mut(), true) {
                let _ = remove(&file);
            }
        }
        let _ = remove(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_name_collision_permits_another_stage_creation_attempt() {
        assert_eq!(
            classify_open_failure(0xc0000035u32 as i32),
            OpenError::Collision
        );
        for status in [
            0xc0000022u32,
            0xc0000185,
            0xc000003a,
            0xc0000043,
            0xc000007f,
        ] {
            assert_eq!(classify_open_failure(status as i32), OpenError::Failed);
        }
        assert_eq!(OpenError::from(invalid()), OpenError::Failed);
    }
    fn envelope(path: &Path) {
        std::fs::write(path.join(MANIFEST), b"{}").unwrap();
        for (name, _) in FILES {
            std::fs::write(path.join(name), b"x").unwrap();
        }
    }
    #[test]
    fn bounded_members_reject_hardlinks_extra_entries_and_oversize() {
        let temp = tempfile::tempdir().unwrap();
        envelope(temp.path());
        assert!(read(&directory(temp.path()).unwrap()).is_ok());
        std::fs::write(temp.path().join("extra"), b"x").unwrap();
        assert!(read(&directory(temp.path()).unwrap()).is_err());
        std::fs::remove_file(temp.path().join("extra")).unwrap();
        let (name, limit) = FILES[0];
        std::fs::write(temp.path().join(name), vec![0; limit + 1]).unwrap();
        assert!(read(&directory(temp.path()).unwrap()).is_err());
        std::fs::remove_file(temp.path().join(name)).unwrap();
        std::fs::hard_link(temp.path().join(MANIFEST), temp.path().join(name)).unwrap();
        assert!(read(&directory(temp.path()).unwrap()).is_err());
    }
    #[test]
    fn locked_directory_and_member_handles_block_rename_and_writes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("archive");
        std::fs::create_dir(&path).unwrap();
        envelope(&path);
        let dir = directory(&path).unwrap();
        assert!(std::fs::rename(&path, temp.path().join("moved")).is_err());
        let file = open(&dir, OsStr::new(MANIFEST), false, false, null_mut(), false).unwrap();
        assert!(std::fs::write(path.join(MANIFEST), b"modified").is_err());
        assert!(std::fs::remove_file(path.join(MANIFEST)).is_err());
        drop(file);
        assert!(read(&dir).is_ok());
    }
    #[test]
    fn staging_acl_reread_cleanup_and_no_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("existing");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("sentinel"), b"keep").unwrap();
        {
            let mut stage = Stage::new(directory(temp.path()).unwrap()).unwrap();
            verify_private(&stage.dir, &private_descriptor().unwrap(), true).unwrap();
            stage.write(MANIFEST, b"{}").unwrap();
            for (name, _) in FILES {
                stage.write(name, b"x").unwrap();
            }
            assert!(stage.write(MANIFEST, b"overwrite").is_err());
            assert!(read(&stage.dir).is_ok());
            assert!(stage.publish(OsStr::new("existing")).is_err());
        }
        assert_eq!(std::fs::read(target.join("sentinel")).unwrap(), b"keep");
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
        let mut stage = Stage::new(directory(temp.path()).unwrap()).unwrap();
        stage.write(MANIFEST, b"{}").unwrap();
        for (name, _) in FILES {
            stage.write(name, b"x").unwrap();
        }
        stage.publish(OsStr::new("published")).unwrap();
        drop(stage);
        assert!(read(&directory(&temp.path().join("published")).unwrap()).is_ok());
    }
    #[test]
    fn symlink_ancestors_and_members_are_rejected_when_creation_is_permitted() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        envelope(&target);
        // Symlink creation needs Developer Mode or a suitable privilege. Return
        // only on that explicit privilege error; all other setup failures fail.
        if let Err(error) = std::os::windows::fs::symlink_dir(&target, temp.path().join("link")) {
            assert_eq!(error.raw_os_error(), Some(1314));
            return;
        }
        assert!(directory(&temp.path().join("link")).is_err());
        let name = FILES[0].0;
        std::fs::remove_file(target.join(name)).unwrap();
        std::os::windows::fs::symlink_file(target.join(MANIFEST), target.join(name)).unwrap();
        assert!(read(&directory(&target).unwrap()).is_err());
    }
    #[test]
    fn unsafe_components_and_namespaces_are_rejected() {
        for name in ["..", "a:b", "a\\b", "name.", "name ", "NUL", "con.txt"] {
            assert!(component(OsStr::new(name)).is_err());
        }
        for path in [r"\\server\share\archive", r"\\?\C:\archive", r"C:archive"] {
            assert!(directory(Path::new(path)).is_err());
        }
    }
}
