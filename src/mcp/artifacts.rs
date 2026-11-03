//! Private, bounded storage for future MCP artifacts. This module has no tool or device access.
//!
//! Until an adapter consumes this API, the types are deliberately kept internal.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

const SCHEMA: &str = "ak820.mcp.artifact/v1";
const MAX_BACKUP: usize = 16 * 1024 * 1024;
const MAX_PLAN: usize = 1024 * 1024;
const MAX_MANIFEST: usize = 64 * 1024;
const MAX_ARTIFACTS: usize = 100;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    Backup,
    Plan,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub schema: String,
    pub kind: Kind,
    pub created_utc: String,
    pub payload_length: u64,
    pub payload_sha256: String,
    pub source_code_sha256: String,
    pub source_release: String,
    pub qs_version: u32,
    pub complete: bool,
    pub plan_expiry_utc: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Durability {
    DirectorySync,
    DirectoryFlushUnsupported,
}

pub(crate) struct Source<'a> {
    pub code_sha256: &'a str,
    pub release: &'a str,
    pub qs_version: u32,
    pub complete: bool,
    pub plan_expiry_utc: Option<&'a str>,
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_utc(value: &str) -> bool {
    !value.ends_with("-00:00")
        && chrono::DateTime::parse_from_rfc3339(value)
            .is_ok_and(|time| time.offset().local_minus_utc() == 0)
}

fn valid_manifest(m: &Manifest) -> bool {
    m.schema == SCHEMA
        && is_hash(&m.payload_sha256)
        && is_hash(&m.source_code_sha256)
        && !m.source_release.is_empty()
        && is_utc(&m.created_utc)
        && match (m.kind, &m.plan_expiry_utc) {
            (Kind::Backup, None) => true,
            (Kind::Plan, Some(expiry)) => is_utc(expiry),
            _ => false,
        }
        && m.payload_length
            <= match m.kind {
                Kind::Backup => MAX_BACKUP as u64,
                Kind::Plan => MAX_PLAN as u64,
            }
}

// macOS private creation uses initial no-inherit ACLs; native behavior is unqualified.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod platform {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::ffi::{CStr, CString, OsStr};
    use std::fs::File;
    use std::io::{self, Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd, RawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::{Component, Path};

    fn invalid(message: &'static str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, message)
    }

    fn name(s: &OsStr) -> io::Result<CString> {
        CString::new(s.as_bytes()).map_err(|_| invalid("NUL in storage path"))
    }

    fn open_at(
        parent: RawFd,
        component: &OsStr,
        flags: i32,
        mode: libc::mode_t,
    ) -> io::Result<File> {
        #[cfg(target_os = "macos")]
        if flags & libc::O_CREAT != 0 {
            return macos_create_file(parent, component, flags, mode);
        }
        let component = name(component)?;
        // SAFETY: component is NUL terminated; the returned descriptor is owned by File.
        let fd = unsafe {
            libc::openat(
                parent,
                component.as_ptr(),
                flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                mode as libc::c_uint,
            )
        };
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            // SAFETY: openat returned a new owned descriptor.
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }

    fn dir_at(parent: RawFd, component: &OsStr, create: bool) -> io::Result<File> {
        if create {
            let component = name(component)?;
            #[cfg(target_os = "macos")]
            let result = macos_mkdir(parent, &component);
            #[cfg(not(target_os = "macos"))]
            let result = {
                // SAFETY: component is NUL terminated and parent is a live directory descriptor.
                if unsafe { libc::mkdirat(parent, component.as_ptr(), 0o700) } < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            };
            if let Err(error) = result {
                if error.kind() != io::ErrorKind::AlreadyExists {
                    return Err(error);
                }
            }
        }
        open_at(parent, component, libc::O_RDONLY | libc::O_DIRECTORY, 0)
    }

    #[cfg(target_os = "macos")]
    fn macos_create(
        parent: RawFd,
        component: &CStr,
        flags: Option<i32>,
        mode: libc::mode_t,
    ) -> io::Result<Option<File>> {
        use std::ffi::c_void;
        extern "C" {
            fn acl_init(count: i32) -> *mut c_void;
            fn acl_free(acl: *mut c_void) -> i32;
            fn acl_get_flagset_np(acl: *mut c_void, flags: *mut *mut c_void) -> i32;
            fn acl_add_flag_np(flags: *mut c_void, flag: i32) -> i32;
            fn filesec_init() -> *mut c_void;
            fn filesec_free(sec: *mut c_void);
            fn filesec_set_property(sec: *mut c_void, property: i32, value: *const c_void) -> i32;
            fn mkdirx_np(path: *const libc::c_char, sec: *mut c_void) -> i32;
            fn openx_np(path: *const libc::c_char, flags: i32, sec: *mut c_void) -> i32;
        }
        let component = component.to_owned();
        if component.to_bytes().is_empty()
            || component.to_bytes() == b"."
            || component.to_bytes() == b".."
            || component.to_bytes().contains(&b'/')
        {
            return Err(invalid("invalid creation component"));
        }
        // A local APFS restriction excludes remote/server-authorized inheritance.
        // It does not qualify APFS or replace native privacy testing.
        let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
        if unsafe { libc::fstatfs(parent, stat.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let stat = unsafe { stat.assume_init() };
        let fs = unsafe { CStr::from_ptr(stat.f_fstypename.as_ptr()) };
        if fs.to_bytes() != b"apfs" || stat.f_flags & libc::MNT_LOCAL as u32 == 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "private creation requires local APFS",
            ));
        }
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    type Fchdir = unsafe extern "C" fn(i32) -> i32;
                    let symbol =
                        unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"pthread_fchdir_np".as_ptr()) };
                    if symbol.is_null() {
                        return Err(io::Error::new(
                            io::ErrorKind::Unsupported,
                            "thread-relative directory unavailable",
                        ));
                    }
                    let fchdir: Fchdir = unsafe { std::mem::transmute(symbol) };
                    if unsafe { fchdir(parent) } != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    // This thread terminates after the operation, including on failure.
                    let acl = unsafe { acl_init(0) };
                    if acl.is_null() {
                        return Err(io::Error::last_os_error());
                    }
                    let sec = unsafe { filesec_init() };
                    if sec.is_null() {
                        let error = io::Error::last_os_error();
                        unsafe { acl_free(acl) };
                        return Err(error);
                    }
                    let result = (|| {
                        let mut acl_flags = std::ptr::null_mut();
                        if unsafe { acl_get_flagset_np(acl, &mut acl_flags) } != 0
                            || unsafe { acl_add_flag_np(acl_flags, 1 << 17) } != 0
                            || unsafe {
                                filesec_set_property(sec, 4, (&mode as *const libc::mode_t).cast())
                            } != 0
                            || unsafe {
                                filesec_set_property(sec, 5, (&acl as *const *mut c_void).cast())
                            } != 0
                        {
                            return Err(io::Error::last_os_error());
                        }
                        if let Some(flags) = flags {
                            let fd = unsafe {
                                openx_np(
                                    component.as_ptr(),
                                    flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_EXCL,
                                    sec,
                                )
                            };
                            if fd < 0 {
                                return Err(io::Error::last_os_error());
                            }
                            Ok(Some(unsafe { File::from_raw_fd(fd) }))
                        } else {
                            if unsafe { mkdirx_np(component.as_ptr(), sec) } != 0 {
                                return Err(io::Error::last_os_error());
                            }
                            Ok(None)
                        }
                    })();
                    unsafe {
                        filesec_free(sec);
                        acl_free(acl);
                    }
                    result
                })
                .join()
                .map_err(|_| invalid("private creation thread failed"))?
        })
    }

    #[cfg(target_os = "macos")]
    fn macos_mkdir(parent: RawFd, component: &CStr) -> io::Result<()> {
        macos_create(parent, component, None, 0o700).map(|_| ())
    }

    #[cfg(target_os = "macos")]
    fn macos_create_file(
        parent: RawFd,
        component: &OsStr,
        flags: i32,
        mode: libc::mode_t,
    ) -> io::Result<File> {
        let component = name(component)?;
        match macos_create(parent, &component, Some(flags), mode) {
            Ok(Some(file)) => Ok(file),
            Ok(None) => Err(invalid("private file was not opened")),
            Err(error)
                if error.kind() == io::ErrorKind::AlreadyExists && flags & libc::O_EXCL == 0 =>
            {
                open_at(
                    parent,
                    OsStr::from_bytes(component.to_bytes()),
                    flags & !libc::O_CREAT,
                    mode,
                )
            }
            Err(error) => Err(error),
        }
    }

    // Darwin mode bits do not exclude additional ACL grants. Reject them before
    // reading or writing private data; never modify an existing directory's ACL.
    #[cfg(target_os = "macos")]
    fn verify_no_extended_acl(file: &File) -> io::Result<()> {
        use std::ffi::c_void;
        extern "C" {
            fn acl_get_fd_np(fd: i32, kind: i32) -> *mut c_void;
            fn acl_valid(acl: *mut c_void) -> i32;
            fn acl_get_entry(acl: *mut c_void, entry: i32, out: *mut *mut c_void) -> i32;
            fn acl_free(acl: *mut c_void) -> i32;
        }
        struct Acl(*mut c_void);
        impl Drop for Acl {
            fn drop(&mut self) {
                // SAFETY: acl_get_fd_np returned this owned ACL.
                unsafe { acl_free(self.0) };
            }
        }
        // SAFETY: the file descriptor is live and ACL_TYPE_EXTENDED is 0x100.
        let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), 0x100) };
        if acl.is_null() {
            return if io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            };
        }
        let acl = Acl(acl);
        // SAFETY: acl is a live ACL and entry points to writable storage.
        if unsafe { acl_valid(acl.0) } != 0 {
            return Err(invalid("invalid artifact ACL"));
        }
        let mut entry = std::ptr::null_mut();
        let result = unsafe { acl_get_entry(acl.0, 0, &mut entry) };
        if result == 0 || io::Error::last_os_error().raw_os_error() != Some(libc::EINVAL) {
            return Err(invalid("artifact has extended ACL entries"));
        }
        Ok(())
    }

    fn private_dir(dir: &File) -> io::Result<()> {
        let m = dir.metadata()?;
        // SAFETY: geteuid has no preconditions.
        if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o7777 != 0o700 {
            return Err(invalid("artifact directory is not private"));
        }
        #[cfg(target_os = "macos")]
        verify_no_extended_acl(dir)?;
        Ok(())
    }

    fn private_file(file: &File) -> io::Result<()> {
        let m = file.metadata()?;
        // SAFETY: geteuid has no preconditions.
        if !m.is_file()
            || m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o7777 != 0o600
            || m.nlink() != 1
        {
            return Err(invalid("artifact file is not private"));
        }
        #[cfg(target_os = "macos")]
        verify_no_extended_acl(file)?;
        Ok(())
    }

    fn validate_id(id: &str) -> io::Result<()> {
        if id.len() != 32
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("invalid artifact ID"));
        }
        Ok(())
    }

    fn hex(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            out.push(DIGITS[(b >> 4) as usize] as char);
            out.push(DIGITS[(b & 15) as usize] as char);
        }
        out
    }

    fn digest(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    fn random_id() -> io::Result<String> {
        let mut random = open_at(
            libc::AT_FDCWD,
            OsStr::new("/dev/urandom"),
            libc::O_RDONLY,
            0,
        )?;
        let mut bytes = [0u8; 16];
        random.read_exact(&mut bytes)?;
        Ok(hex(&bytes))
    }

    fn unlink(parent: RawFd, component: &str, directory: bool) -> io::Result<()> {
        let component = name(OsStr::new(component))?;
        // SAFETY: component is NUL terminated and parent is a live directory descriptor.
        if unsafe {
            libc::unlinkat(
                parent,
                component.as_ptr(),
                if directory { libc::AT_REMOVEDIR } else { 0 },
            )
        } < 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn rename(parent: RawFd, from: &str, to: &str) -> io::Result<()> {
        let from = name(OsStr::new(from))?;
        let to = name(OsStr::new(to))?;
        // SAFETY: both names are NUL terminated and relative to the owned directory.
        if unsafe { libc::renameat(parent, from.as_ptr(), parent, to.as_ptr()) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn entries(dir: &File, limit: usize) -> io::Result<(Vec<String>, bool)> {
        // Open a new file description so readdir does not advance the Store's directory offset.
        let duplicate = open_at(
            dir.as_raw_fd(),
            OsStr::new("."),
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        let fd = std::os::fd::IntoRawFd::into_raw_fd(duplicate);
        // SAFETY: fd is a newly owned directory descriptor; fdopendir takes ownership on success.
        let stream = unsafe { libc::fdopendir(fd) };
        if stream.is_null() {
            let error = io::Error::last_os_error();
            // SAFETY: fdopendir failed and did not consume fd.
            unsafe { libc::close(fd) };
            return Err(error);
        }
        let result = (|| {
            let mut names = Vec::new();
            loop {
                // readdir uses errno to distinguish end-of-directory from failure.
                #[cfg(any(target_os = "linux", target_os = "android"))]
                unsafe {
                    *libc::__errno_location() = 0;
                }
                #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
                unsafe {
                    *libc::__error() = 0;
                }
                // SAFETY: stream is valid until closed; the entry is consumed before the next call.
                let entry = unsafe { libc::readdir(stream) };
                if entry.is_null() {
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() != Some(0) {
                        return Err(error);
                    }
                    break;
                }
                // SAFETY: POSIX dirent d_name is NUL terminated.
                let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }
                    .to_str()
                    .map_err(|_| invalid("non-UTF-8 storage entry"))?;
                if name != "." && name != ".." {
                    if names.len() == limit {
                        return Ok((names, true));
                    }
                    names.push(name.to_owned());
                }
            }
            Ok((names, false))
        })();
        // SAFETY: stream is valid and has not been closed.
        unsafe { libc::closedir(stream) };
        result
    }

    /// The root descriptor anchors all subsequent operations; caller paths are never accepted.
    pub(crate) struct Store {
        root: File,
        private_ancestors: Vec<File>,
    }

    struct Lock(File);
    impl Drop for Lock {
        fn drop(&mut self) {
            // SAFETY: the file descriptor remains open until after this drop.
            unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
        }
    }

    impl Store {
        pub(crate) fn open() -> io::Result<Self> {
            let base = dirs::data_local_dir()
                .ok_or_else(|| invalid("per-user local data directory unavailable"))?;
            Self::open_in(&base)
        }

        fn open_in(base: &Path) -> io::Result<Self> {
            if !base.is_absolute() {
                return Err(invalid("local data path must be absolute"));
            }
            let mut dir = File::open("/")?;
            for part in base.components() {
                match part {
                    Component::RootDir => {}
                    Component::Normal(part) => {
                        // On macOS do not create the local-data base: its parents
                        // are outside this store and may carry inheritable ACLs.
                        let next = dir_at(dir.as_raw_fd(), part, cfg!(not(target_os = "macos")))?;
                        dir.sync_all()?;
                        dir = next;
                    }
                    _ => return Err(invalid("invalid local data path")),
                }
            }
            #[cfg(target_os = "macos")]
            {
                let base_metadata = dir.metadata()?;
                // The local-data base is outside this store; unlike private
                // children it may be searchable, but must not be writable by
                // another user who could race ACL inheritance during creation.
                if base_metadata.uid() != unsafe { libc::geteuid() }
                    || base_metadata.mode() & 0o022 != 0
                {
                    return Err(invalid("local data directory is not user controlled"));
                }
                verify_no_extended_acl(&dir)?;
            }
            let mut private_ancestors = Vec::new();
            for (index, part) in ["ak-configurator", "mcp-artifacts", "v1"]
                .iter()
                .enumerate()
            {
                // Refuse an ACL-bearing parent before creating a private child;
                // otherwise inherited grants can leave an inaccessible new directory.
                #[cfg(target_os = "macos")]
                verify_no_extended_acl(&dir)?;
                let next = dir_at(dir.as_raw_fd(), OsStr::new(part), true)?;
                private_dir(&next)?;
                dir.sync_all()?;
                if index > 0 {
                    private_ancestors.push(dir);
                }
                dir = next;
            }
            Ok(Self {
                root: dir,
                private_ancestors,
            })
        }

        fn lock(&self) -> io::Result<Lock> {
            for dir in &self.private_ancestors {
                private_dir(dir)?;
            }
            private_dir(&self.root)?;
            let file = open_at(
                self.root.as_raw_fd(),
                OsStr::new(".lock"),
                libc::O_RDWR | libc::O_CREAT | libc::O_NONBLOCK,
                0o600,
            )?;
            private_file(&file)?;
            // SAFETY: file is an open descriptor; nonblocking prevents an indefinite wait.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Lock(file))
        }

        fn inventory_locked(&self) -> io::Result<Vec<String>> {
            let mut ids = Vec::new();
            // One extra slot accounts for .lock wherever it appears in the directory.
            // At most 100 IDs are returned; repeated explicit eviction makes further IDs visible.
            for entry in entries(&self.root, MAX_ARTIFACTS + 1)?.0 {
                if entry == ".lock" {
                    continue;
                }
                validate_id(&entry)?;
                let dir = dir_at(self.root.as_raw_fd(), OsStr::new(&entry), false)?;
                private_dir(&dir)?;
                ids.push(entry);
                if ids.len() == MAX_ARTIFACTS {
                    break;
                }
            }
            ids.sort_unstable();
            Ok(ids)
        }

        pub(crate) fn durability(&self) -> Durability {
            Durability::DirectorySync
        }

        /// Includes incomplete artifacts, so an operator can explicitly evict after a crash.
        pub(crate) fn inventory(&self) -> io::Result<Vec<String>> {
            let _lock = self.lock()?;
            self.inventory_locked()
        }

        pub(crate) fn put(
            &self,
            kind: Kind,
            payload: &[u8],
            source: Source<'_>,
        ) -> io::Result<(String, Manifest)> {
            self.put_with_id(kind, payload, source, &random_id()?, None)
        }

        fn put_with_id(
            &self,
            kind: Kind,
            payload: &[u8],
            source: Source<'_>,
            id: &str,
            fault: Option<usize>,
        ) -> io::Result<(String, Manifest)> {
            validate_id(id)?;
            let limit = match kind {
                Kind::Backup => MAX_BACKUP,
                Kind::Plan => MAX_PLAN,
            };
            if payload.len() > limit {
                return Err(invalid("artifact payload exceeds limit"));
            }
            let manifest = Manifest {
                schema: SCHEMA.to_owned(),
                kind,
                created_utc: chrono::Utc::now().to_rfc3339(),
                payload_length: payload.len() as u64,
                payload_sha256: digest(payload),
                source_code_sha256: source.code_sha256.to_owned(),
                source_release: source.release.to_owned(),
                qs_version: source.qs_version,
                complete: source.complete,
                plan_expiry_utc: source.plan_expiry_utc.map(str::to_owned),
            };
            if !valid_manifest(&manifest) {
                return Err(invalid("invalid artifact metadata"));
            }
            let encoded = serde_json::to_vec(&manifest).map_err(io::Error::other)?;
            if encoded.len() > MAX_MANIFEST {
                return Err(invalid("artifact manifest exceeds limit"));
            }
            let _lock = self.lock()?;
            if self.inventory_locked()?.len() >= MAX_ARTIFACTS {
                return Err(invalid("artifact store is full"));
            }
            let id_name = name(OsStr::new(id))?;
            #[cfg(target_os = "macos")]
            macos_mkdir(self.root.as_raw_fd(), &id_name)?;
            #[cfg(not(target_os = "macos"))]
            // SAFETY: id_name is a validated single component and root is an open directory.
            if unsafe { libc::mkdirat(self.root.as_raw_fd(), id_name.as_ptr(), 0o700) } < 0 {
                return Err(io::Error::last_os_error());
            }
            let result = (|| {
                let dir = dir_at(self.root.as_raw_fd(), OsStr::new(id), false)?;
                private_dir(&dir)?;
                Self::fault(fault, 0)?;
                Self::write_file(
                    &dir,
                    ".payload.tmp",
                    "payload.bin",
                    payload,
                    fault == Some(1),
                )?;
                Self::fault(fault, 2)?;
                Self::write_file(
                    &dir,
                    ".manifest.tmp",
                    "manifest.json",
                    &encoded,
                    fault == Some(3),
                )?;
                Self::fault(fault, 4)?;
                self.root.sync_all()?;
                Ok(())
            })();
            if let Err(error) = result {
                if let Err(cleanup) = self.remove_owned(id) {
                    return Err(io::Error::other(format!(
                        "storage failure: {error}; incomplete artifact cleanup failed: {cleanup}"
                    )));
                }
                return Err(error);
            }
            Ok((id.to_owned(), manifest))
        }

        fn fault(fault: Option<usize>, step: usize) -> io::Result<()> {
            if fault == Some(step) {
                #[cfg(test)]
                if std::env::var_os("AK_C02_CRASH_CHILD").is_some() {
                    std::process::exit(88);
                }
                return Err(io::Error::other("injected storage fault"));
            }
            Ok(())
        }

        fn write_file(
            dir: &File,
            temp: &str,
            final_name: &str,
            data: &[u8],
            fault: bool,
        ) -> io::Result<()> {
            let mut file = open_at(
                dir.as_raw_fd(),
                OsStr::new(temp),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
                0o600,
            )?;
            private_file(&file)?;
            if fault {
                file.write_all(&data[..data.len() / 2])?;
                #[cfg(test)]
                if std::env::var_os("AK_C02_CRASH_CHILD").is_some() {
                    std::process::exit(88);
                }
                return Err(io::Error::from_raw_os_error(libc::ENOSPC));
            }
            file.write_all(data)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            rename(dir.as_raw_fd(), temp, final_name)?;
            dir.sync_all()
        }

        fn remove_owned(&self, id: &str) -> io::Result<()> {
            let dir = dir_at(self.root.as_raw_fd(), OsStr::new(id), false)?;
            private_dir(&dir)?;
            let (names, more) = entries(&dir, 4)?;
            if more
                || names.iter().any(|entry| {
                    !matches!(
                        entry.as_str(),
                        ".payload.tmp" | "payload.bin" | ".manifest.tmp" | "manifest.json"
                    )
                })
            {
                return Err(invalid("unexpected file in artifact"));
            }
            for entry in names {
                unlink(dir.as_raw_fd(), &entry, false)?;
            }
            dir.sync_all()?;
            unlink(self.root.as_raw_fd(), id, true)?;
            self.root.sync_all()
        }

        pub(crate) fn evict(&self, id: &str) -> io::Result<()> {
            validate_id(id)?;
            let _lock = self.lock()?;
            self.remove_owned(id)
        }

        pub(crate) fn read(&self, id: &str) -> io::Result<(Manifest, Vec<u8>)> {
            validate_id(id)?;
            let _lock = self.lock()?;
            let dir = dir_at(self.root.as_raw_fd(), OsStr::new(id), false)?;
            private_dir(&dir)?;
            let manifest_bytes = Self::read_file(&dir, "manifest.json", MAX_MANIFEST)?;
            let manifest: Manifest =
                serde_json::from_slice(&manifest_bytes).map_err(io::Error::other)?;
            if !valid_manifest(&manifest) {
                return Err(invalid("invalid artifact manifest"));
            }
            let limit = match manifest.kind {
                Kind::Backup => MAX_BACKUP,
                Kind::Plan => MAX_PLAN,
            };
            let payload = Self::read_file(&dir, "payload.bin", limit)?;
            if payload.len() as u64 != manifest.payload_length
                || digest(&payload) != manifest.payload_sha256
            {
                return Err(invalid("artifact payload length or digest mismatch"));
            }
            Ok((manifest, payload))
        }

        fn read_file(dir: &File, name: &str, limit: usize) -> io::Result<Vec<u8>> {
            let file = open_at(
                dir.as_raw_fd(),
                OsStr::new(name),
                libc::O_RDONLY | libc::O_NONBLOCK,
                0,
            )?;
            private_file(&file)?;
            let mut bytes = Vec::new();
            file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > limit {
                return Err(invalid("artifact file exceeds limit"));
            }
            Ok(bytes)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::unix::fs::{symlink, PermissionsExt};

        fn source() -> Source<'static> {
            Source {
                code_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                release: "synthetic",
                qs_version: 3,
                complete: true,
                plan_expiry_utc: None,
            }
        }
        fn store() -> (tempfile::TempDir, Store) {
            let temp = tempfile::tempdir().unwrap();
            let store = Store::open_in(temp.path()).unwrap();
            (temp, store)
        }
        fn root(temp: &tempfile::TempDir) -> std::path::PathBuf {
            temp.path().join("ak-configurator/mcp-artifacts/v1")
        }

        #[test]
        fn mcp_artifacts_round_trip_truncation_and_eviction() {
            let (temp, store) = store();
            let (id, manifest) = store.put(Kind::Backup, b"synthetic", source()).unwrap();
            assert_eq!(id.len(), 32);
            assert_eq!(
                manifest.payload_sha256,
                "b3cc0475bb78a5026098858e9889acf666d31062d513d303314eca31d36e72f2"
            );
            let (_, bytes) = store.read(&id).unwrap();
            assert_eq!(bytes, b"synthetic");
            std::fs::write(root(&temp).join(&id).join("payload.bin"), b"short").unwrap();
            assert!(store.read(&id).is_err());
            store.evict(&id).unwrap();
            assert!(store.read(&id).is_err());
        }

        #[test]
        fn mcp_artifacts_traversal_and_symlinks_fail_closed() {
            let (temp, store) = store();
            assert!(store.read("../payload.bin").is_err());
            assert!(store.evict("../anything").is_err());
            let (id, _) = store.put(Kind::Backup, b"fixture", source()).unwrap();
            let payload = root(&temp).join(&id).join("payload.bin");
            std::fs::remove_file(&payload).unwrap();
            symlink(temp.path().join("victim"), &payload).unwrap();
            assert!(store.read(&id).is_err());
            let alias = temp.path().join("alias");
            symlink(temp.path().join("ak-configurator"), &alias).unwrap();
            assert!(Store::open_in(&alias).is_err());
        }

        #[test]
        fn mcp_artifacts_limits_collision_and_no_auto_eviction() {
            let (temp, store) = store();
            let id = "0123456789abcdef0123456789abcdef";
            store
                .put_with_id(Kind::Backup, b"first", source(), id, None)
                .unwrap();
            assert_eq!(
                store
                    .put_with_id(Kind::Backup, b"second", source(), id, None)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::AlreadyExists
            );
            assert_eq!(store.read(id).unwrap().1, b"first");
            assert!(store
                .put(Kind::Backup, &vec![0; MAX_BACKUP + 1], source())
                .is_err());
            let mut plan = source();
            plan.plan_expiry_utc = Some("2030-01-01T00:00:00Z");
            assert!(store.put(Kind::Plan, &vec![0; MAX_PLAN + 1], plan).is_err());
            for i in 1..MAX_ARTIFACTS {
                let dir = root(&temp).join(format!("{i:032x}"));
                std::fs::create_dir(&dir).unwrap();
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
            assert!(store.put(Kind::Backup, b"full", source()).is_err());
            assert_eq!(store.read(id).unwrap().1, b"first");
        }

        #[test]
        fn mcp_artifacts_bounded_inventory_allows_explicit_overflow_recovery() {
            let (temp, store) = store();
            for i in 0..105 {
                let dir = root(&temp).join(format!("{i:032x}"));
                std::fs::create_dir(&dir).unwrap();
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
            let initial = store.inventory().unwrap();
            assert_eq!(initial.len(), MAX_ARTIFACTS);
            assert!(store.put(Kind::Backup, b"full", source()).is_err());
            for id in initial {
                store.evict(&id).unwrap();
            }
            assert_eq!(store.inventory().unwrap().len(), 5);
        }

        #[test]
        fn mcp_artifacts_faults_remove_only_new_incomplete_artifact() {
            let (_temp, store) = store();
            let (existing, _) = store.put(Kind::Backup, b"preserve", source()).unwrap();
            for step in 0..=4 {
                let id = format!("{:032x}", step + 1);
                assert!(store
                    .put_with_id(
                        Kind::Backup,
                        b"synthetic fault payload",
                        source(),
                        &id,
                        Some(step)
                    )
                    .is_err());
                assert!(store.read(&id).is_err());
                assert_eq!(store.read(&existing).unwrap().1, b"preserve");
            }
            assert_eq!(store.inventory().unwrap(), vec![existing]);
        }

        #[test]
        fn mcp_artifacts_rejects_non_utc_plan_expiry() {
            let (_temp, store) = store();
            let mut plan = source();
            plan.plan_expiry_utc = Some("2030-01-01T00:00:00+01:00");
            assert!(store.put(Kind::Plan, b"plan", plan).is_err());
            let mut unknown_offset = source();
            unknown_offset.plan_expiry_utc = Some("2030-01-01T00:00:00-00:00");
            assert!(store.put(Kind::Plan, b"plan", unknown_offset).is_err());
            assert!(store.inventory().unwrap().is_empty());
            let (id, mut manifest) = store.put(Kind::Backup, b"fixture", source()).unwrap();
            manifest.created_utc = "2030-01-01T00:00:00+01:00".to_owned();
            let path = root(&_temp).join(&id).join("manifest.json");
            std::fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
            assert!(store.read(&id).is_err());
        }

        #[test]
        fn mcp_artifacts_exact_payload_limits_and_plan_metadata() {
            let (_temp, store) = store();
            let mut plan = source();
            plan.plan_expiry_utc = Some("2030-01-01T00:00:00Z");
            let data = vec![42; MAX_PLAN];
            let (id, _) = store.put(Kind::Plan, &data, plan).unwrap();
            let (manifest, actual) = store.read(&id).unwrap();
            assert_eq!(manifest.kind, Kind::Plan);
            assert_eq!(
                manifest.plan_expiry_utc.as_deref(),
                Some("2030-01-01T00:00:00Z")
            );
            assert_eq!(actual, data);
            let backup = vec![11; MAX_BACKUP];
            let (backup_id, _) = store.put(Kind::Backup, &backup, source()).unwrap();
            assert_eq!(store.read(&backup_id).unwrap().1, backup);
        }

        #[test]
        fn mcp_artifacts_hardlink_rejected_and_unknown_file_preserved() {
            let (temp, store) = store();
            let (id, _) = store.put(Kind::Backup, b"fixture", source()).unwrap();
            let dir = root(&temp).join(&id);
            let payload = dir.join("payload.bin");
            let external_link = temp.path().join("external-link");
            std::fs::hard_link(&payload, &external_link).unwrap();
            assert!(store.read(&id).is_err());
            std::fs::remove_file(external_link).unwrap();
            std::fs::write(dir.join("unknown"), b"keep").unwrap();
            assert!(store.evict(&id).is_err());
            assert!(payload.exists());
            assert_eq!(std::fs::read(dir.join("unknown")).unwrap(), b"keep");
        }

        #[test]
        fn mcp_artifacts_fifo_is_rejected_without_waiting_for_a_writer() {
            let (temp, store) = store();
            let (id, _) = store.put(Kind::Backup, b"fixture", source()).unwrap();
            let payload = root(&temp).join(&id).join("payload.bin");
            std::fs::remove_file(&payload).unwrap();
            let path = name(payload.as_os_str()).unwrap();
            // SAFETY: path is NUL terminated; the test owns its temporary directory.
            assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            assert!(store.read(&id).is_err());
        }

        #[test]
        fn mcp_artifacts_crash_child() {
            let Some(base) = std::env::var_os("AK_C02_CRASH_CHILD") else {
                return;
            };
            let step: usize = std::env::var("AK_C02_CRASH_STEP").unwrap().parse().unwrap();
            let store = Store::open_in(Path::new(&base)).unwrap();
            let _ = store.put_with_id(
                Kind::Backup,
                b"child synthetic",
                source(),
                "fedcba9876543210fedcba9876543210",
                Some(step),
            );
            panic!("injected crash was not reached");
        }

        #[test]
        fn mcp_artifacts_crash_inventory_allows_explicit_recovery() {
            let (temp, store) = store();
            let id = "fedcba9876543210fedcba9876543210";
            for step in 0..=4 {
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "mcp::artifacts::platform::tests::mcp_artifacts_crash_child",
                    ])
                    .env("AK_C02_CRASH_CHILD", temp.path())
                    .env("AK_C02_CRASH_STEP", step.to_string())
                    .status()
                    .unwrap();
                assert_eq!(status.code(), Some(88));
                assert_eq!(store.inventory().unwrap(), vec![id]);
                store.evict(id).unwrap();
                assert!(store.inventory().unwrap().is_empty());
            }
        }

        #[test]
        fn mcp_artifacts_revalidates_private_ancestors() {
            let (temp, store) = store();
            let ancestor = temp.path().join("ak-configurator");
            std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(store.inventory().is_err());
            std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o700)).unwrap();
            assert!(store.inventory().is_ok());
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn mcp_artifacts_macos_missing_base_is_not_created() {
            let temp = tempfile::tempdir().unwrap();
            let base = temp.path().join("missing");
            assert!(Store::open_in(&base).is_err());
            assert!(!base.exists());
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn mcp_artifacts_macos_inherited_acl_preflight_creates_nothing() {
            let temp = tempfile::tempdir().unwrap();
            let status = std::process::Command::new("/bin/chmod")
                .args([
                    "+a",
                    "everyone allow read,write,file_inherit,directory_inherit",
                ])
                .arg(temp.path())
                .status()
                .unwrap();
            assert!(status.success());
            assert!(Store::open_in(temp.path()).is_err());
            assert!(!temp.path().join("ak-configurator").exists());
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn mcp_artifacts_macos_initial_acl_blocks_inheritance() {
            let temp = tempfile::tempdir().unwrap();
            let status = std::process::Command::new("/bin/chmod")
                .args([
                    "+a",
                    "everyone allow read,write,file_inherit,directory_inherit",
                ])
                .arg(temp.path())
                .status()
                .unwrap();
            assert!(status.success());
            let parent = File::open(temp.path()).unwrap();
            let dir_name = name(OsStr::new("child")).unwrap();
            macos_mkdir(parent.as_raw_fd(), &dir_name).unwrap();
            let child = open_at(
                parent.as_raw_fd(),
                OsStr::new("child"),
                libc::O_RDONLY | libc::O_DIRECTORY,
                0,
            )
            .unwrap();
            private_dir(&child).unwrap();
            let file = macos_create_file(
                parent.as_raw_fd(),
                OsStr::new("test.bin"),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
                0o600,
            )
            .unwrap();
            private_file(&file).unwrap();
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn mcp_artifacts_macos_extended_acl_is_rejected() {
            let (temp, store) = store();
            let (id, _) = store.put(Kind::Backup, b"synthetic", source()).unwrap();
            let payload = root(&temp).join(&id).join("payload.bin");
            let status = std::process::Command::new("/bin/chmod")
                .args(["+a", "everyone allow read"])
                .arg(&payload)
                .status()
                .unwrap();
            assert!(status.success());
            assert_eq!(std::fs::metadata(&payload).unwrap().mode() & 0o777, 0o600);
            assert!(store.read(&id).is_err());
            let ancestor = temp.path().join("ak-configurator");
            let status = std::process::Command::new("/bin/chmod")
                .args(["+a", "everyone allow read"])
                .arg(&ancestor)
                .status()
                .unwrap();
            assert!(status.success());
            assert_eq!(std::fs::metadata(&ancestor).unwrap().mode() & 0o777, 0o700);
            assert!(store.inventory().is_err());
        }

        #[test]
        fn mcp_artifacts_permissions_and_manifest_bound() {
            let (temp, store) = store();
            let (id, _) = store.put(Kind::Backup, b"fixture", source()).unwrap();
            let dir = root(&temp).join(&id);
            assert_eq!(
                std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            let manifest = dir.join("manifest.json");
            assert_eq!(
                std::fs::metadata(&manifest).unwrap().permissions().mode() & 0o777,
                0o600
            );
            std::fs::write(&manifest, vec![b'x'; MAX_MANIFEST + 1]).unwrap();
            assert!(store.read(&id).is_err());
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(store.read(&id).is_err());
        }
    }
}

// These primitives are adapted from the project's Windows backup I/O; unlike
// backup staging they must also support bounded inventory and a stable lock.
// Native Windows behavior remains unverified.
#[cfg(target_os = "windows")]
pub(crate) mod platform {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::ffi::{c_void, OsStr, OsString};
    use std::fs::File;
    use std::io::{self, Read, Write};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::path::{Component, Path, Prefix};
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
        fn RtlNtStatusToDosError(status: i32) -> u32;
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
        fn GetCurrentProcess() -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn LocalFree(memory: Handle) -> Handle;
        fn GetFileInformationByHandle(file: Handle, info: *mut FileInfo) -> i32;
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

    #[link(name = "bcrypt")]
    extern "system" {
        fn BCryptGenRandom(algorithm: Handle, buffer: *mut u8, length: u32, flags: u32) -> i32;
    }

    fn invalid() -> io::Error {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid private artifact storage object",
        )
    }

    fn random_id() -> io::Result<String> {
        let mut bytes = [0u8; 16];
        // SAFETY: BCRYPT_USE_SYSTEM_PREFERRED_RNG requires a null algorithm handle;
        // bytes points to exactly length writable bytes.
        let status = unsafe { BCryptGenRandom(null_mut(), bytes.as_mut_ptr(), 16, 2) };
        if status < 0 {
            return Err(io::Error::other(
                "Windows cryptographic random generator failed",
            ));
        }
        use std::fmt::Write;
        let mut id = String::with_capacity(32);
        for byte in bytes {
            write!(&mut id, "{byte:02x}").expect("writing to a string cannot fail");
        }
        Ok(id)
    }

    struct Local(Handle);
    impl Drop for Local {
        fn drop(&mut self) {
            // SAFETY: Local owns memory allocated by a Windows LocalAlloc API.
            unsafe { LocalFree(self.0) };
        }
    }

    fn private_descriptor() -> io::Result<Local> {
        let mut token = null_mut();
        // SAFETY: output handle is valid writable storage.
        if unsafe { OpenProcessToken(GetCurrentProcess(), 8, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        struct Token(Handle);
        impl Drop for Token {
            fn drop(&mut self) {
                // SAFETY: Token owns the process-token handle.
                unsafe { CloseHandle(self.0) };
            }
        }
        let token = Token(token);
        let mut needed = 0;
        // SAFETY: the zero-length query obtains the necessary buffer size.
        unsafe { GetTokenInformation(token.0, 1, null_mut(), 0, &mut needed) };
        if needed == 0 || needed > 65536 {
            return Err(invalid());
        }
        let mut data = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        // SAFETY: data is aligned and at least needed bytes long.
        if unsafe { GetTokenInformation(token.0, 1, data.as_mut_ptr().cast(), needed, &mut needed) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut text = null_mut();
        // SAFETY: TOKEN_USER begins with a valid SID pointer from GetTokenInformation.
        if unsafe { ConvertSidToStringSidW(data[0] as *const c_void, &mut text) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let sid_text = Local(text.cast());
        let mut length = 0;
        // SAFETY: ConvertSidToStringSidW returned a NUL-terminated SID string.
        while length < 256 && unsafe { *text.add(length) } != 0 {
            length += 1;
        }
        if length == 256 {
            return Err(invalid());
        }
        // SAFETY: length was bounded by the terminator above.
        let sid = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
            .map_err(|_| invalid())?;
        drop(sid_text);
        let sddl = format!("O:{sid}D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;{sid})");
        let sddl: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut sd = null_mut();
        // SAFETY: sddl is NUL-terminated and sd is writable.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut sd,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Local(sd))
    }

    // Compare actual owner and protected ACL with the explicit descriptor; do
    // not assume the filesystem honored the requested security attributes.
    fn verify_private(file: &File, expected: &Local, protected: bool) -> io::Result<()> {
        let (mut owner, mut actual_acl, mut actual_sd) = (null_mut(), null_mut(), null_mut());
        // SAFETY: output pointers are valid and the file handle is live.
        let result = unsafe {
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
        };
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result as i32));
        }
        let actual_sd = Local(actual_sd);
        let (mut expected_owner, mut expected_acl) = (null_mut(), null_mut());
        let (mut defaulted, mut present) = (0, 0);
        let (mut control, mut revision) = (0, 0);
        // SAFETY: descriptors are live and each out-parameter is writable.
        if unsafe { GetSecurityDescriptorOwner(expected.0, &mut expected_owner, &mut defaulted) }
            == 0
            || unsafe { EqualSid(owner, expected_owner) } == 0
            || unsafe {
                GetSecurityDescriptorDacl(
                    expected.0,
                    &mut present,
                    &mut expected_acl,
                    &mut defaulted,
                )
            } == 0
            || present == 0
            || actual_acl.is_null()
            || expected_acl.is_null()
            || unsafe { GetSecurityDescriptorControl(actual_sd.0, &mut control, &mut revision) }
                == 0
            || (protected && control & 0x1000 == 0)
        {
            return Err(invalid());
        }
        // SAFETY: Windows returned valid ACLs within their security descriptors;
        // compare their bounded declared sizes and exactly two ACEs.
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

    // Only single components may be resolved below an already-open directory.
    fn component(value: &OsStr) -> io::Result<Vec<u16>> {
        let text: Vec<_> = value.encode_wide().collect();
        if text.is_empty()
            || text.len() > 32766
            || text.iter().any(|c| matches!(*c, 0 | 47 | 58 | 92))
            || matches!(text.last(), Some(32 | 46))
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
        Ok(text)
    }

    fn names(dir: &File, limit: usize) -> io::Result<(Vec<OsString>, bool)> {
        let mut out = Vec::new();
        let mut buffer = vec![0u64; 8192];
        let mut restart = 1;
        loop {
            let mut io = IoStatus {
                status: 0,
                information: 0,
            };
            // SAFETY: buffer and status are aligned and live for this synchronous query.
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
            };
            restart = 0;
            if status as u32 == 0x8000_0006 {
                return Ok((out, false));
            }
            if status < 0 || io.information < 12 || io.information > buffer.len() * 8 {
                return Err(invalid());
            }
            // SAFETY: information is bounded to the initialized output buffer.
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
            component(&name)?;
            if out.len() == limit {
                return Ok((out, true));
            }
            if out.contains(&name) {
                return Err(invalid());
            }
            out.push(name);
        }
    }

    #[repr(C)]
    struct Rename {
        replace: u32,
        root: Handle,
        length: u32,
        name: [u16; 1],
    }

    fn rename(file: &File, parent: &File, name: &OsStr) -> io::Result<()> {
        let name = component(name)?;
        let offset = std::mem::offset_of!(Rename, name);
        let size = (offset + name.len() * 2).max(std::mem::size_of::<Rename>());
        let mut buffer = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
        let info = buffer.as_mut_ptr().cast::<Rename>();
        // SAFETY: buffer is aligned, has size bytes, and holds a valid variable-sized record.
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
        // SAFETY: both handles and the record are valid until this synchronous call returns.
        let status = unsafe {
            NtSetInformationFile(file.as_raw_handle(), &mut io, info.cast(), size as u32, 10)
        };
        if status < 0 {
            return Err(io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(status) } as i32,
            ));
        }
        Ok(())
    }

    fn remove(file: &File) -> io::Result<()> {
        let delete: u8 = 1;
        let mut io = IoStatus {
            status: 0,
            information: 0,
        };
        // SAFETY: this marks the opened DELETE-capable handle, not a resolved pathname.
        let status = unsafe {
            NtSetInformationFile(
                file.as_raw_handle(),
                &mut io,
                (&delete as *const u8).cast(),
                1,
                13,
            )
        };
        if status < 0 {
            return Err(io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(status) } as i32,
            ));
        }
        Ok(())
    }

    fn validate_id(id: &str) -> io::Result<()> {
        if id.len() != 32
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        Ok(())
    }

    fn digest(bytes: &[u8]) -> String {
        use std::fmt::Write;
        let mut out = String::with_capacity(64);
        for byte in Sha256::digest(bytes) {
            write!(&mut out, "{byte:02x}").expect("writing to a string cannot fail");
        }
        out
    }

    pub(crate) struct Store {
        root: File,
        private_ancestors: Vec<File>,
        security: Local,
    }

    impl Store {
        pub(crate) fn open() -> io::Result<Self> {
            let base = dirs::data_local_dir().ok_or_else(invalid)?;
            Self::open_in(&base)
        }

        fn open_in(base: &Path) -> io::Result<Self> {
            let mut dir =
                directory(base).map_err(|e| io::Error::other(format!("open data root: {e}")))?;
            let security = private_descriptor()
                .map_err(|e| io::Error::other(format!("build private descriptor: {e}")))?;
            let mut private_ancestors = Vec::new();
            for (index, part) in ["ak-configurator", "mcp-artifacts", "v1"]
                .iter()
                .enumerate()
            {
                let (next, created) = match open_relative(
                    &dir,
                    OsStr::new(part),
                    true,
                    true,
                    security.0,
                    0x1_0006,
                    3,
                ) {
                    Ok(file) => (file, true),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (
                        open_relative(&dir, OsStr::new(part), true, false, null_mut(), 0x6, 3)?,
                        false,
                    ),
                    Err(e) => return Err(e),
                };
                if let Err(error) = verify_private(&next, &security, true) {
                    if created {
                        if let Err(cleanup) = remove(&next) {
                            return Err(io::Error::other(format!(
                                "{error}; new directory cleanup failed: {cleanup}"
                            )));
                        }
                    }
                    return Err(io::Error::other(format!(
                        "verify private component {part}: {error}"
                    )));
                }
                if index > 0 {
                    private_ancestors.push(dir);
                }
                dir = next;
            }
            Ok(Self {
                root: dir,
                private_ancestors,
                security,
            })
        }

        // Holding the exclusively shared handle is the nonblocking interprocess lock.
        fn lock(&self) -> io::Result<File> {
            for dir in &self.private_ancestors {
                verify_private(dir, &self.security, true)?;
            }
            verify_private(&self.root, &self.security, true)?;
            let (lock, created) = match open_relative(
                &self.root,
                OsStr::new(".lock"),
                false,
                true,
                self.security.0,
                0x1_0002,
                0,
            ) {
                Ok(file) => (file, true),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (
                    open_relative(
                        &self.root,
                        OsStr::new(".lock"),
                        false,
                        false,
                        null_mut(),
                        0x2,
                        0,
                    )?,
                    false,
                ),
                Err(e) => return Err(e),
            };
            if let Err(error) = verify_private(&lock, &self.security, true)
                .and_then(|()| file_size_single_link(&lock).map(|_| ()))
            {
                if created {
                    if let Err(cleanup) = remove(&lock) {
                        return Err(io::Error::other(format!(
                            "{error}; new lock cleanup failed: {cleanup}"
                        )));
                    }
                }
                return Err(error);
            }
            Ok(lock)
        }

        fn inventory_locked(&self) -> io::Result<Vec<String>> {
            let mut ids = Vec::new();
            for name in names(&self.root, MAX_ARTIFACTS + 1)?.0 {
                if name == ".lock" {
                    continue;
                }
                let id = name.into_string().map_err(|_| invalid())?;
                validate_id(&id)?;
                let dir =
                    open_relative(&self.root, OsStr::new(&id), true, false, null_mut(), 0, 3)?;
                verify_private(&dir, &self.security, true)?;
                ids.push(id);
                if ids.len() == MAX_ARTIFACTS {
                    break;
                }
            }
            ids.sort_unstable();
            Ok(ids)
        }

        pub(crate) fn durability(&self) -> Durability {
            Durability::DirectoryFlushUnsupported
        }

        pub(crate) fn inventory(&self) -> io::Result<Vec<String>> {
            let _lock = self.lock()?;
            self.inventory_locked()
        }

        pub(crate) fn put(
            &self,
            kind: Kind,
            payload: &[u8],
            source: Source<'_>,
        ) -> io::Result<(String, Manifest)> {
            let (id, manifest, _) = self.put_with_id(kind, payload, source, &random_id()?, None)?;
            Ok((id, manifest))
        }

        fn put_with_id(
            &self,
            kind: Kind,
            payload: &[u8],
            source: Source<'_>,
            id: &str,
            fault: Option<usize>,
        ) -> io::Result<(String, Manifest, Durability)> {
            validate_id(id)?;
            let limit = match kind {
                Kind::Backup => MAX_BACKUP,
                Kind::Plan => MAX_PLAN,
            };
            if payload.len() > limit {
                return Err(invalid());
            }
            let manifest = Manifest {
                schema: SCHEMA.to_owned(),
                kind,
                created_utc: chrono::Utc::now().to_rfc3339(),
                payload_length: payload.len() as u64,
                payload_sha256: digest(payload),
                source_code_sha256: source.code_sha256.to_owned(),
                source_release: source.release.to_owned(),
                qs_version: source.qs_version,
                complete: source.complete,
                plan_expiry_utc: source.plan_expiry_utc.map(str::to_owned),
            };
            if !valid_manifest(&manifest) {
                return Err(invalid());
            }
            let encoded = serde_json::to_vec(&manifest).map_err(io::Error::other)?;
            if encoded.len() > MAX_MANIFEST {
                return Err(invalid());
            }
            let _lock = self.lock()?;
            if self.inventory_locked()?.len() >= MAX_ARTIFACTS {
                return Err(invalid());
            }
            let dir = open_relative(
                &self.root,
                OsStr::new(id),
                true,
                true,
                self.security.0,
                0x1_0006,
                3,
            )?;
            let result = (|| {
                verify_private(&dir, &self.security, true)?;
                Self::fault(fault, 0)?;
                self.write_file(
                    &dir,
                    ".payload.tmp",
                    "payload.bin",
                    payload,
                    fault == Some(1),
                )?;
                Self::fault(fault, 2)?;
                self.write_file(
                    &dir,
                    ".manifest.tmp",
                    "manifest.json",
                    &encoded,
                    fault == Some(3),
                )?;
                Self::fault(fault, 4)?;
                Ok(())
            })();
            if let Err(error) = result {
                // Cleanup is handle-relative and never reopens the ID by name.
                if let Err(cleanup) = self.remove_contents(&dir).and_then(|()| remove(&dir)) {
                    return Err(io::Error::other(format!(
                        "storage failure: {error}; incomplete artifact cleanup failed: {cleanup}"
                    )));
                }
                return Err(error);
            }
            Ok((
                id.to_owned(),
                manifest,
                Durability::DirectoryFlushUnsupported,
            ))
        }

        fn fault(fault: Option<usize>, step: usize) -> io::Result<()> {
            if fault == Some(step) {
                #[cfg(test)]
                if std::env::var_os("AK_C02_CRASH_CHILD").is_some() {
                    std::process::exit(88);
                }
                return Err(io::Error::other("injected storage fault"));
            }
            Ok(())
        }

        fn write_file(
            &self,
            dir: &File,
            temp: &str,
            final_name: &str,
            bytes: &[u8],
            fault: bool,
        ) -> io::Result<()> {
            let mut file = open_relative(
                dir,
                OsStr::new(temp),
                false,
                true,
                self.security.0,
                0x1_0116,
                1,
            )?;
            let result = (|| {
                verify_private(&file, &self.security, true)?;
                file_size_single_link(&file)?;
                if fault {
                    file.write_all(&bytes[..bytes.len() / 2])?;
                    #[cfg(test)]
                    if std::env::var_os("AK_C02_CRASH_CHILD").is_some() {
                        std::process::exit(88);
                    }
                    return Err(io::Error::from_raw_os_error(112));
                }
                file.write_all(bytes)?;
                file.flush()?;
                file.sync_all()?;
                // NT rename operates on the still-open DELETE-capable source handle.
                rename(&file, dir, OsStr::new(final_name))
            })();
            if let Err(error) = result {
                if let Err(cleanup) = remove(&file) {
                    return Err(io::Error::other(format!(
                        "{error}; temporary file cleanup failed: {cleanup}"
                    )));
                }
                return Err(error);
            }
            Ok(())
        }

        fn remove_contents(&self, dir: &File) -> io::Result<()> {
            self.remove_contents_with_fault(dir, None)
        }

        fn remove_contents_with_fault(
            &self,
            dir: &File,
            fault_after: Option<usize>,
        ) -> io::Result<()> {
            let (mut names, more) = names(dir, 4)?;
            if more
                || names.iter().any(|name| {
                    ![
                        ".payload.tmp",
                        "payload.bin",
                        ".manifest.tmp",
                        "manifest.json",
                    ]
                    .iter()
                    .any(|allowed| name == allowed)
                })
            {
                return Err(invalid());
            }
            // Stable ordering makes the prevalidation regression test deterministic.
            names.sort_unstable();
            let mut files = Vec::with_capacity(names.len());
            for name in names {
                let file = open_relative(dir, &name, false, false, null_mut(), 0x1_0000, 1)?;
                verify_private(&file, &self.security, true)?;
                file_size_single_link(&file)?;
                files.push(file);
            }
            // Deletion itself is not atomic: an error can leave a partial artifact.
            for (index, file) in files.iter().enumerate() {
                if fault_after == Some(index) {
                    return Err(io::Error::other("injected eviction fault"));
                }
                remove(file)?;
            }
            Ok(())
        }

        pub(crate) fn evict(&self, id: &str) -> io::Result<()> {
            self.evict_with_durability(id).map(|_| ())
        }

        fn evict_with_durability(&self, id: &str) -> io::Result<Durability> {
            validate_id(id)?;
            let _lock = self.lock()?;
            let dir = open_relative(
                &self.root,
                OsStr::new(id),
                true,
                false,
                null_mut(),
                0x1_0000,
                3,
            )?;
            verify_private(&dir, &self.security, true)?;
            self.remove_contents(&dir)?;
            remove(&dir)?;
            Ok(Durability::DirectoryFlushUnsupported)
        }

        fn read_file(&self, dir: &File, name: &str, limit: usize) -> io::Result<Vec<u8>> {
            let file = open_relative(dir, OsStr::new(name), false, false, null_mut(), 0, 1)?;
            verify_private(&file, &self.security, true)?;
            let size = file_size_single_link(&file)?;
            if size > limit as u64 {
                return Err(invalid());
            }
            let mut bytes = Vec::with_capacity(size as usize);
            file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 != size {
                return Err(invalid());
            }
            Ok(bytes)
        }

        pub(crate) fn read(&self, id: &str) -> io::Result<(Manifest, Vec<u8>)> {
            validate_id(id)?;
            let _lock = self.lock()?;
            let dir = open_relative(&self.root, OsStr::new(id), true, false, null_mut(), 0, 3)?;
            verify_private(&dir, &self.security, true)?;
            let manifest: Manifest =
                serde_json::from_slice(&self.read_file(&dir, "manifest.json", MAX_MANIFEST)?)
                    .map_err(io::Error::other)?;
            if !valid_manifest(&manifest) {
                return Err(invalid());
            }
            let limit = match manifest.kind {
                Kind::Backup => MAX_BACKUP,
                Kind::Plan => MAX_PLAN,
            };
            let payload = self.read_file(&dir, "payload.bin", limit)?;
            if payload.len() as u64 != manifest.payload_length
                || digest(&payload) != manifest.payload_sha256
            {
                return Err(invalid());
            }
            Ok((manifest, payload))
        }
    }

    fn file_size_single_link(file: &File) -> io::Result<u64> {
        // SAFETY: zero is a valid initial state for this output-only C structure.
        let mut info: FileInfo = unsafe { std::mem::zeroed() };
        // SAFETY: file is live and info is writable.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if info.links != 1 {
            return Err(invalid());
        }
        Ok((u64::from(info.size_high) << 32) | u64::from(info.size_low))
    }

    fn directory(path: &Path) -> io::Result<File> {
        if !path.is_absolute() {
            return Err(invalid());
        }
        let mut parts = path.components();
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
        let mut dir = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(format!("{}:\\", char::from(drive)))?;
        if dir.metadata()?.file_attributes() & 0x400 != 0 {
            return Err(invalid());
        }
        let parts: Vec<_> = parts.collect();
        for (index, part) in parts.iter().enumerate() {
            match part {
                Component::Normal(name) => {
                    // The base must allow creation below it; ancestors are read-only.
                    let rights = if index + 1 == parts.len() { 0x6 } else { 0 };
                    dir = open_relative(&dir, name, true, false, null_mut(), rights, 3)?;
                }
                _ => return Err(invalid()),
            }
        }
        Ok(dir)
    }

    fn open_relative(
        parent: &File,
        entry: &OsStr,
        directory: bool,
        create: bool,
        security: *mut c_void,
        access_extra: u32,
        sharing: u32,
    ) -> io::Result<File> {
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
        };
        let mut status = IoStatus {
            status: 0,
            information: 0,
        };
        let mut handle = null_mut();
        // SYNCHRONIZE | READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_READ_DATA.
        // Open the reparse point itself, never follow it, and require the type.
        let access = 0x0010_0000 | 0x0002_0000 | 0x80 | 1 | access_extra;
        let options = 0x20 | 0x0020_0000 | if directory { 1 } else { 0x40 };
        // SAFETY: pointers refer to live local buffers for the synchronous call.
        let result = unsafe {
            NtCreateFile(
                &mut handle,
                access,
                &mut attrs,
                &mut status,
                null(),
                0,
                sharing,
                if create { 2 } else { 1 },
                options,
                null(),
                0,
            )
        };
        if result < 0 {
            // SAFETY: converts the NTSTATUS returned by NtCreateFile.
            return Err(io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(result) } as i32,
            ));
        }
        // SAFETY: successful NtCreateFile returns a new owned handle.
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
            if create && access_extra & 0x1_0000 != 0 {
                if let Err(cleanup) = remove(&file) {
                    return Err(io::Error::other(format!(
                        "invalid new object; cleanup failed: {cleanup}"
                    )));
                }
            }
            return Err(invalid());
        }
        Ok(file)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn source() -> Source<'static> {
            Source {
                code_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                release: "synthetic",
                qs_version: 3,
                complete: true,
                plan_expiry_utc: None,
            }
        }
        fn store() -> (tempfile::TempDir, Store) {
            let temp = tempfile::tempdir().unwrap();
            let store = Store::open_in(temp.path()).unwrap();
            (temp, store)
        }
        fn root(temp: &tempfile::TempDir) -> std::path::PathBuf {
            temp.path().join("ak-configurator/mcp-artifacts/v1")
        }

        #[test]
        fn mcp_artifacts_windows_roundtrip_collision_and_faults() {
            let (temp, store) = store();
            let id = "0123456789abcdef0123456789abcdef";
            store
                .put_with_id(Kind::Backup, b"first", source(), id, None)
                .unwrap();
            assert!(store
                .put_with_id(Kind::Backup, b"replacement", source(), id, None)
                .is_err());
            assert_eq!(store.read(id).unwrap().1, b"first");
            assert!(store.read("../payload.bin").is_err());
            for step in 0..=4 {
                let other = format!("{:032x}", step + 1);
                assert!(store
                    .put_with_id(Kind::Backup, b"fault", source(), &other, Some(step))
                    .is_err());
                assert!(!root(&temp).join(other).exists());
                assert_eq!(store.read(id).unwrap().1, b"first");
            }
            store.evict(id).unwrap();
            assert!(store.read(id).is_err());
        }

        #[test]
        fn mcp_artifacts_windows_lock_and_bounds() {
            let (_temp, store) = store();
            let lock = store.lock().unwrap();
            assert!(store.lock().is_err());
            drop(lock);
            assert!(store.lock().is_ok());
            assert!(store
                .put(Kind::Backup, &vec![0; MAX_BACKUP + 1], source())
                .is_err());
            let mut plan = source();
            plan.plan_expiry_utc = Some("2030-01-01T00:00:00Z");
            assert!(store.put(Kind::Plan, &vec![0; MAX_PLAN + 1], plan).is_err());
        }

        #[test]
        fn mcp_artifacts_windows_crash_child() {
            let Some(base) = std::env::var_os("AK_C02_CRASH_BASE") else {
                return;
            };
            let step: usize = std::env::var("AK_C02_CRASH_STEP").unwrap().parse().unwrap();
            let store = Store::open_in(Path::new(&base)).unwrap();
            let _ = store.put_with_id(
                Kind::Backup,
                b"synthetic",
                source(),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                Some(step),
            );
            panic!("injected child unexpectedly returned");
        }

        #[test]
        fn mcp_artifacts_windows_crash_inventory_and_recovery() {
            for step in 0..=4 {
                let temp = tempfile::tempdir().unwrap();
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("mcp_artifacts_windows_crash_child")
                    .env("AK_C02_CRASH_BASE", temp.path())
                    .env("AK_C02_CRASH_STEP", step.to_string())
                    .env("AK_C02_CRASH_CHILD", "1")
                    .status()
                    .unwrap();
                assert_eq!(status.code(), Some(88));
                let store = Store::open_in(temp.path()).unwrap();
                assert_eq!(
                    store.inventory().unwrap(),
                    ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
                );
                assert_eq!(
                    store
                        .evict_with_durability("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                        .unwrap(),
                    Durability::DirectoryFlushUnsupported
                );
                assert!(store.inventory().unwrap().is_empty());
            }
        }

        #[test]
        fn mcp_artifacts_windows_eviction_prevalidates_and_reports_partial_failure() {
            let (temp, store) = store();
            let (id, _) = store.put(Kind::Backup, b"fixture", source()).unwrap();
            let path = root(&temp).join(&id);
            let payload = path.join("payload.bin");
            let manifest = path.join("manifest.json");
            let link = temp.path().join("external-link");
            std::fs::hard_link(&payload, &link).unwrap();
            assert!(store.evict(&id).is_err());
            assert!(payload.exists() && manifest.exists());
            std::fs::remove_file(link).unwrap();

            let dir = open_relative(
                &store.root,
                OsStr::new(&id),
                true,
                false,
                null_mut(),
                0x1_0000,
                3,
            )
            .unwrap();
            assert!(store.remove_contents_with_fault(&dir, Some(1)).is_err());
            assert_ne!(payload.exists(), manifest.exists());
            drop(dir);
            store.evict(&id).unwrap();
        }

        #[test]
        fn mcp_artifacts_windows_hardlink_and_truncation() {
            let (temp, store) = store();
            let (id, _) = store.put(Kind::Backup, b"synthetic", source()).unwrap();
            assert_eq!(store.durability(), Durability::DirectoryFlushUnsupported);
            let payload = root(&temp).join(&id).join("payload.bin");
            let hardlink = temp.path().join("hardlink");
            std::fs::hard_link(&payload, &hardlink).unwrap();
            assert!(store.read(&id).is_err());
            std::fs::remove_file(hardlink).unwrap();
            std::fs::write(&payload, b"short").unwrap();
            assert!(store.read(&id).is_err());
        }
    }
}

// Do not silently substitute path-based operations where private, no-follow,
// directory-relative storage and its durability semantics are not implemented.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) mod platform {
    use std::io;

    pub(crate) struct Store;

    impl Store {
        pub(crate) fn open() -> io::Result<Self> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "private artifact storage and directory durability are not implemented on this platform",
            ))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn mcp_artifacts_unsupported_platform_fails_closed() {
            assert!(matches!(Store::open(), Err(e) if e.kind() == io::ErrorKind::Unsupported));
        }
    }
}
