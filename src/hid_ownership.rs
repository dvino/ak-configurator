//! Cross-process cooperative ownership, held by the actual HID helper.
//! Independent of GUI instance settings and released only when the helper exits.
//! External programs that do not use this guard still require HID/OS arbitration.
#[cfg(unix)]
use anyhow::Context;
use anyhow::{bail, Result};

#[derive(Debug)]
pub(crate) struct LeaseBusy;
impl std::fmt::Display for LeaseBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Keyboard is busy in another AK Configurator process")
    }
}
impl std::error::Error for LeaseBusy {}

#[cfg(all(test, target_os = "windows"))]
pub(crate) fn test_contended_lease_error() -> anyhow::Error {
    let key = format!(
        "fixture-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    );
    let _lease = DeviceLease::named(&key).unwrap();
    DeviceLease::named(&key)
        .err()
        .expect("second lease must contend")
}

#[cfg(all(test, unix))]
pub(crate) fn test_contended_lease_error() -> anyhow::Error {
    let path = std::env::temp_dir().join(format!(
        "ak-lease-fixture-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir(&path).unwrap();
    let directory = std::fs::File::open(&path).unwrap();
    let lease = DeviceLease::at(&directory, "fixture").unwrap();
    let error = DeviceLease::at(&directory, "fixture")
        .err()
        .expect("second lease must contend");
    drop(lease);
    std::fs::remove_file(path.join("ak-configurator-hid-fixture.lock")).unwrap();
    std::fs::remove_dir(path).unwrap();
    error
}

#[cfg(any(not(test), unix))]
fn key(device: &crate::device::Device) -> Result<String> {
    use sha2::{Digest, Sha256};
    if !device.is_ak820() || device.path.is_empty() {
        bail!("Cannot establish AK820 device ownership");
    }
    let path = std::ffi::CString::new(device.path.as_bytes())?;
    let port = crate::hid::hid_ak820::usb_port(&path)?;
    Ok(format!(
        "{:x}",
        Sha256::digest(format!("ak820-usb-port:{}", port.display()).as_bytes())
    ))
}

#[cfg(unix)]
pub(crate) struct DeviceLease {
    // Ownership keeps flock held until this guard is dropped; no reads needed.
    _file: std::fs::File,
}

#[cfg(unix)]
impl DeviceLease {
    pub(crate) fn acquire(device: &crate::device::Device) -> Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        // Do not honor TMPDIR/config overrides: all cooperating application
        // instances must rendezvous in the same OS-owned directory.
        let root = if cfg!(target_os = "macos") {
            "/private/tmp"
        } else {
            "/tmp"
        };
        let directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(root)
            .context("Cannot open device ownership directory")?;
        Self::at(&directory, &key(device)?)
    }

    fn at(directory: &std::fs::File, key: &str) -> Result<Self> {
        use std::os::fd::{AsRawFd, FromRawFd};
        use std::os::unix::fs::MetadataExt;
        let name = std::ffi::CString::new(format!("ak-configurator-hid-{key}.lock"))?;
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDWR
                    | libc::O_CREAT
                    | libc::O_NOFOLLOW
                    | libc::O_CLOEXEC
                    | libc::O_NONBLOCK,
                0o600,
            )
        };
        if fd < 0 {
            bail!("Keyboard is busy or exclusive device ownership is unavailable");
        }
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        let metadata = file
            .metadata()
            .context("Cannot inspect device ownership handle")?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.len() != 0
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            bail!("Device ownership file is not trustworthy");
        }
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::WouldBlock {
                return Err(LeaseBusy.into());
            }
            return Err(error.into());
        }
        // Never unlink a lock: another process may already be waiting on this
        // inode. Closing the handle releases the advisory lock after a crash too.
        Ok(Self { _file: file })
    }
}

#[cfg(target_os = "windows")]
pub(crate) struct DeviceLease(*mut core::ffi::c_void);

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateMutexW(
        attributes: *mut core::ffi::c_void,
        initial_owner: i32,
        name: *const u16,
    ) -> *mut core::ffi::c_void;
    fn GetLastError() -> u32;
    fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
}

#[cfg(target_os = "windows")]
impl DeviceLease {
    #[cfg(not(test))]
    pub(crate) fn acquire(device: &crate::device::Device) -> Result<Self> {
        Self::named(&key(device)?)
    }

    fn named(key: &str) -> Result<Self> {
        let name: Vec<u16> = format!("Global\\AKConfiguratorHid-{key}")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            // Object lifetime, not thread-affine mutex ownership, is the lease.
            let handle = CreateMutexW(std::ptr::null_mut(), 0, name.as_ptr());
            if handle.is_null() {
                bail!("Cannot establish exclusive device ownership");
            }
            if GetLastError() == 183 {
                CloseHandle(handle);
                return Err(LeaseBusy.into());
            }
            Ok(Self(handle))
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;

    #[test]
    fn windows_owner_contends_and_releases_with_handle() {
        let key = format!(
            "test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        );
        let first = DeviceLease::named(&key).unwrap();
        assert!(DeviceLease::named(&key).is_err());
        let other = DeviceLease::named(&format!("{key}-other")).unwrap();
        drop(first);
        assert!(DeviceLease::named(&key).is_ok());
        drop(other);
    }
}

#[cfg(target_os = "windows")]
impl Drop for DeviceLease {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn device_owner_rejects_non_ak820_empty_and_nul_paths_before_lock_creation() {
        let mut device = crate::device::Device {
            name: "Offline ownership refusal fixture".into(),
            vendor_id: 0x1209,
            product_id: 0x1234,
            manufacturer: String::new(),
            serial_number: "offline-refusal".into(),
            bus_type: "Usb".into(),
            path: String::new(),
            instance_token: String::new(),
            firmware: crate::firmware::FirmwareProtocol::Vial,
        };
        assert_eq!(
            DeviceLease::acquire(&device).err().unwrap().to_string(),
            "Cannot establish AK820 device ownership"
        );
        device.vendor_id = 0x320f;
        device.product_id = 0x505b;
        assert!(device.is_ak820());
        assert_eq!(
            DeviceLease::acquire(&device).err().unwrap().to_string(),
            "Cannot establish AK820 device ownership"
        );
        device.path = "\0".into();
        assert!(DeviceLease::acquire(&device)
            .err()
            .unwrap()
            .root_cause()
            .is::<std::ffi::NulError>());
    }

    #[test]
    fn device_owner_contends_until_last_handle_closes() {
        let temp = tempfile::tempdir().unwrap();
        let root = File::open(temp.path()).unwrap();
        let first = DeviceLease::at(&root, "same-endpoint").unwrap();
        assert!(DeviceLease::at(&root, "same-endpoint").is_err());
        let other = DeviceLease::at(&root, "different-endpoint").unwrap();
        drop(first);
        assert!(DeviceLease::at(&root, "same-endpoint").is_ok());
        drop(other);
    }

    #[test]
    fn device_owner_rejects_symlinks_hardlinks_and_shared_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = File::open(temp.path()).unwrap();
        let path = temp.path().join("ak-configurator-hid-bad.lock");
        symlink("missing", &path).unwrap();
        assert!(DeviceLease::at(&root, "bad").is_err());
        fs::remove_file(&path).unwrap();
        fs::write(&path, []).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(DeviceLease::at(&root, "bad").is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::hard_link(&path, temp.path().join("alias")).unwrap();
        assert!(DeviceLease::at(&root, "bad").is_err());
    }
}
