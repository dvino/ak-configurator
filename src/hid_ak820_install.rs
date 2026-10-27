// SPDX-License-Identifier: GPL-3.0-or-later
//! Installation IO remains in the existing helper and its cooperative lease.
use super::*;
use crate::ak820_install_executor::{
    self as engine, InstallRequest, InstallationResult, PreparedInstallation,
};
use crate::ak820_loader::{self as loader, Command};
use crate::ak820_settings_backup::BackupResult;
use std::sync::atomic::AtomicU32;

impl HidDevice {
    /// Read-only discovery; no HID handle or report is opened here. The caller
    /// explicitly selects one endpoint before opening an installation owner.
    pub(crate) fn ak820_install_candidates() -> Result<Vec<crate::device::Device>> {
        if crate::isolated_runtime() {
            return Ok(Vec::new());
        }
        ensure!(
            !std::env::args().any(|a| a == "--demo-ak820"),
            "Physical installation is disabled in demo mode"
        );
        #[cfg(test)]
        {
            return Ok(Vec::new());
        }
        #[cfg(not(test))]
        {
            let api = hidapi::HidApi::new()?;
            let mut out = Vec::new();
            for info in api.device_list() {
                if info.vendor_id() == 0x320f
                    && info.product_id() == 0x505b
                    && matches!(info.bus_type(), hidapi::BusType::Usb)
                    && ((info.usage_page() == 0xff1c && info.usage() == 0x92)
                        || (info.usage_page() == 0xff60 && info.usage() == 0x61))
                {
                    usb_port(info.path())?;
                    out.push(super::super::device_from_info(info));
                }
            }
            ensure!(
                out.len() <= 1,
                "Multiple AK820 applications; select a single keyboard before installation"
            );
            Ok(out)
        }
    }
    pub(crate) fn open_for_install(device: &crate::device::Device) -> Result<Self> {
        ensure!(!crate::isolated_runtime(), "Installation is disabled in isolated application modes");
        ensure!(
            !std::env::args().any(|a| a == "--demo-ak820"),
            "Physical installation is disabled in demo mode"
        );
        ensure!(
            super::super::read_only_hid_session().is_none(),
            "Installation owner is forbidden in read-only mode"
        );
        #[cfg(test)]
        {
            let _ = device;
            bail!("Physical installation is disabled in test builds");
        }
        #[cfg(not(test))]
        {
            ensure!(device.is_ak820(), "Unsupported installation device");
            Ok(Self {
                backend: super::super::HidBackend::Proxy(std::sync::Arc::new(
                    super::super::HidProxy::open_install(device)?,
                )),
                unlock_confirmation_pending: AtomicBool::new(false),
                read_only: None,
                ak820: Some(Default::default()),
            })
        }
    }
    pub(crate) fn ak820_prepare_install(
        &self,
        request: InstallRequest,
        admission: Option<&crate::ak820_release::ReleaseAdmission>,
        cancel: &AtomicBool,
    ) -> Result<PreparedInstallation> {
        ensure!(
            self.read_only.is_none() && !std::env::args().any(|a| a == "--demo-ak820"),
            "Installation is unavailable in read-only or demo mode"
        );
        self.ak820_install_session_ready()?;
        capture_check_cancel(cancel)?;
        let mut io = ParentTransport(self, cancel, false);
        let source = engine::InstallTransport::read_rom(&mut io).map_err(anyhow::Error::msg)?;
        capture_check_cancel(cancel)?;
        let binding = self
            .install_io(&WireCommand::Binding)
            .map_err(anyhow::Error::msg)?;
        let owner_binding = if binding == [0; 32] {
            None
        } else {
            Some(crate::ak820_settings_backup::sha256(&binding))
        };
        PreparedInstallation::prepare(request, source, admission, owner_binding)
            .map_err(anyhow::Error::msg)
    }
    fn ak820_install_session_ready(&self) -> Result<()> {
        let state = self
            .ak820
            .as_ref()
            .context("Not an AK820 session")?
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        ensure!(
            state.failure.is_none(),
            "Read and verify the device again after a failed operation before preparing installation"
        );
        Ok(())
    }
    pub(crate) fn ak820_execute_install(
        &self,
        plan: &PreparedInstallation,
        admission: Option<&crate::ak820_release::ReleaseAdmission>,
        confirmed_pin: &str,
        cancel: &AtomicBool,
        progress: &AtomicU32,
        save_safety: impl FnOnce(&[u8]) -> BackupResult<()>,
    ) -> Result<InstallationResult> {
        ensure!(
            self.read_only.is_none() && !std::env::args().any(|a| a == "--demo-ak820"),
            "Installation is unavailable in read-only or demo mode"
        );
        let mut state = self
            .ak820
            .as_ref()
            .context("Not AK820")?
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        ensure!(
            state.failure.is_none(),
            "Installation is blocked after a failed operation; read and verify the device again"
        );
        // Keep the settings owner locked for the complete transaction; helper
        // requests are serialized and its lease survives USB mode changes.
        state.loaded = false;
        state.failure =
            Some("Installation owns the device; reconnect and verify after it ends".into());
        Ok(engine::execute(
            &mut ParentTransport(self, cancel, false),
            plan,
            admission,
            confirmed_pin,
            cancel,
            progress,
            save_safety,
        ))
    }
    fn install_io(&self, command: &WireCommand) -> BackupResult<[u8; 32]> {
        #[cfg(test)]
        {
            let _ = command;
            return Err("Physical installation is disabled in test builds".into());
        }
        #[cfg(not(test))]
        match &self.backend {
            super::super::HidBackend::Proxy(proxy) => {
                proxy.install_command(command).map_err(|e| format!("{e:#}"))
            }
            _ => Err("Installation requires the exclusive native HID helper".into()),
        }
    }
}

#[cfg(test)]
mod install_session_tests {
    use super::*;

    #[test]
    fn failed_capture_owner_cannot_prepare_even_with_defaults_or_rollback() {
        let (hid, log) = HidDevice::test_ak820_device(true);
        hid.ak820.as_ref().unwrap().lock().unwrap().failure = Some("storage fault".into());
        let error = hid
            .ak820_prepare_install(
                InstallRequest::BundledFirmware {
                    profile: crate::app::ak820_beta1_profiles::ProfileId::Main,
                    recovery: None,
                    settings: engine::InstallSettingsChoice::Defaults,
                },
                None,
                &AtomicBool::new(false),
            )
            .err()
            .expect("fault must stop before any installation transport");
        assert!(error.to_string().contains("failed operation"));
        assert!(log.requests().is_empty());
        assert!(hid.ak820_install_session_ready().is_err());
    }

    #[test]
    fn fresh_install_owner_does_not_require_loaded_project_settings() {
        let (hid, log) = HidDevice::test_ak820_device(false);
        assert!(hid.ak820_install_session_ready().is_ok());
        assert!(log.requests().is_empty());
    }
}
struct ParentTransport<'a>(&'a HidDevice, &'a AtomicBool, bool);
impl engine::InstallTransport for ParentTransport<'_> {
    fn read_rom(&mut self) -> BackupResult<Vec<u8>> {
        let mut out = Vec::with_capacity(65536);
        while out.len() < 65536 {
            if !self.2 && self.1.load(Ordering::Acquire) {
                return Err("Installation cancelled before erase".into());
            }
            let count = (65536 - out.len()).min(24);
            let reply = self.0.install_io(&WireCommand::Read {
                address: out.len() as u16,
                count: count as u8,
            })?;
            out.extend_from_slice(&reply[..count]);
        }
        Ok(out)
    }
    fn fresh_hold(&mut self, cancel: &AtomicBool) -> BackupResult<()> {
        self.0.install_io(&WireCommand::Lock)?;
        self.0.install_io(&WireCommand::StartHold)?;
        let start = std::time::Instant::now();
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err("Installation cancelled during confirmation".into());
            }
            let status = self.0.install_io(&WireCommand::PollHold)?;
            if status[0] == 1 && status[1] == 0 && status[2] == 0 {
                return Ok(());
            }
            if start.elapsed() > std::time::Duration::from_secs(60) {
                return Err("Fresh physical hold timed out".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    fn transition(&mut self) -> BackupResult<()> {
        self.0.install_io(&WireCommand::Transition).map(|_| ())
    }
    fn loader_command(&mut self, command: Command, checksum: u32) -> BackupResult<()> {
        if command == Command::Erase {
            self.2 = true;
        }
        self.0
            .install_io(&WireCommand::Loader {
                command: command as u8,
                checksum,
            })
            .map(|_| ())
    }
    fn loader_block(&mut self, report: &[u8; 65]) -> BackupResult<()> {
        self.0
            .install_io(&WireCommand::Block {
                bytes: report[1..].to_vec(),
            })
            .map(|_| ())
    }
    fn reconnect(&mut self) -> BackupResult<()> {
        self.0.install_io(&WireCommand::Reconnect).map(|_| ())
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum WireCommand {
    Binding,
    Read { address: u16, count: u8 },
    Lock,
    StartHold,
    PollHold,
    Transition,
    Loader { command: u8, checksum: u32 },
    Block { bytes: Vec<u8> },
    Reconnect,
}
/// Helper-only state. An error ends the helper; no failed command is replayed.
pub(crate) struct InstallUsb {
    port: std::path::PathBuf,
    binding: [u8; 32],
    application: Option<hidapi::HidDevice>,
    loader: Option<hidapi::HidDevice>,
    native: bool,
    phase: u8,
    blocks: usize,
}
impl InstallUsb {
    pub(crate) fn open(device: &crate::device::Device) -> Result<Self> {
        ensure!(!crate::isolated_runtime(), "Installation is disabled in isolated application modes");
        ensure!(device.is_ak820(), "Not an AK820 USB device");
        let api = hidapi::HidApi::new()?;
        let mut matches = api
            .device_list()
            .filter(|i| i.path().to_string_lossy() == device.path);
        let info = matches.next().context("Selected application disappeared")?;
        ensure!(matches.next().is_none(), "Ambiguous selected application");
        ensure!(
            matches!(info.bus_type(), hidapi::BusType::Usb),
            "Installation requires USB"
        );
        let native = info.usage_page() == 0xff1c && info.usage() == 0x92;
        ensure!(
            native || (info.usage_page() == 0xff60 && info.usage() == 0x61),
            "Unsupported application endpoint"
        );
        let port = usb_port(info.path())?;
        ensure!(
            !api.device_list()
                .any(|i| i.vendor_id() == 0x0c45 && i.product_id() == 0x7687),
            "An OEM loader is already present; no transition allowed"
        );
        let mut binding = [0; 32];
        if let Some(serial) = info
            .serial_number()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            use sha2::Digest;
            binding.copy_from_slice(&sha2::Sha256::digest(
                format!("ak820-owner-1|{}|{}", serial, port.display()).as_bytes(),
            ));
        }
        Ok(Self {
            binding,
            port,
            application: Some(info.open_device(&api)?),
            loader: None,
            native,
            phase: 0,
            blocks: 0,
        })
    }
    fn raw(&self, request: &[u8]) -> Result<[u8; 32]> {
        let dev = self
            .application
            .as_ref()
            .context("Application is unavailable")?;
        let mut packet = [0u8; 33];
        packet[1..1 + request.len()].copy_from_slice(request);
        ensure!(dev.write(&packet)? == 33, "Short application write");
        let mut reply = [0; 32];
        ensure!(
            dev.read_timeout(&mut reply, 2500)? == 32,
            "Incomplete application reply"
        );
        Ok(reply)
    }
    fn find(&self, loader: bool) -> Result<(hidapi::HidDevice, bool)> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
        loop {
            let api = hidapi::HidApi::new()?;
            let mut found = None;
            for info in api.device_list() {
                let native = info.usage_page() == 0xff1c && info.usage() == 0x92;
                let eligible = if loader {
                    info.vendor_id() == 0x0c45
                        && info.product_id() == 0x7687
                        && info.interface_number() == 1
                        && info.usage_page() == 12
                        && info.usage() == 1
                } else {
                    info.vendor_id() == 0x320f
                        && info.product_id() == 0x505b
                        && (native || (info.usage_page() == 0xff60 && info.usage() == 0x61))
                };
                if eligible {
                    ensure!(
                        matches!(info.bus_type(), hidapi::BusType::Usb),
                        "Non-USB installation endpoint"
                    );
                    ensure!(
                        usb_port(info.path())? == self.port,
                        "An installation endpoint appeared on a different USB port"
                    );
                    if !loader && self.binding != [0; 32] {
                        use sha2::Digest;
                        let serial = info
                            .serial_number()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .context("Application serial disappeared after installation")?;
                        let rebound = sha2::Sha256::digest(
                            format!("ak820-owner-1|{}|{}", serial, self.port.display()).as_bytes(),
                        );
                        ensure!(
                            rebound.as_slice() == self.binding,
                            "Application serial differs after installation"
                        );
                    }
                    ensure!(found.is_none(), "Ambiguous installation endpoint");
                    found = Some((info.open_device(&api)?, native));
                }
            }
            if let Some(found) = found {
                return Ok(found);
            }
            ensure!(
                std::time::Instant::now() < deadline,
                "USB transition timed out; no retry or recovery"
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    pub(crate) fn command(&mut self, command: WireCommand) -> Result<[u8; 32]> {
        let mut out = [0; 32];
        match command {
            WireCommand::Binding => {
                ensure!(self.phase == 0, "Owner binding during flash");
                out = self.binding;
            }
            WireCommand::Read { address, count } => {
                ensure!(
                    (1..=24).contains(&count) && address as usize + count as usize <= 65536,
                    "Invalid ROM read"
                );
                ensure!(self.phase == 0 || self.phase == 6, "ROM read during flash");
                if self.native {
                    let mut p = [0u8; 64];
                    p[0] = 4;
                    p[3] = 7;
                    p[4] = count;
                    p[5..7].copy_from_slice(&address.wrapping_sub(0x7077).to_le_bytes());
                    let sum = p[3..].iter().map(|b| *b as u16).sum::<u16>();
                    p[1..3].copy_from_slice(&sum.to_le_bytes());
                    let dev = self.application.as_ref().context("No application")?;
                    ensure!(dev.write(&p)? == 64, "Short OEM read request");
                    let mut reply = [0; 64];
                    ensure!(
                        dev.read_timeout(&mut reply, 2500)? == 64
                            && reply[0] == 4
                            && reply[3..7] == p[3..7]
                            && reply[7] == 0,
                        "Invalid OEM ROM reply"
                    );
                    out[..count as usize].copy_from_slice(&reply[8..8 + count as usize]);
                } else {
                    let p = [0xfd, 0, (address >> 8) as u8, address as u8, count];
                    let reply = self.raw(&p)?;
                    ensure!(reply[..5] == p, "ROM address differs");
                    out[..count as usize].copy_from_slice(&reply[5..5 + count as usize]);
                }
            }
            WireCommand::Lock | WireCommand::StartHold | WireCommand::PollHold => {
                ensure!(
                    self.phase == 0 && !self.native,
                    "Physical hold requires project application"
                );
                let op = match command {
                    WireCommand::Lock => 8,
                    WireCommand::StartHold => 6,
                    _ => 7,
                };
                let reply = self.raw(&[0xfe, op])?;
                if op == 7 {
                    ensure!(
                        reply[0] <= 1
                            && reply[1] <= 1
                            && reply[2] <= 30
                            && (reply[0] == 0 || reply[1..3] == [0, 0]),
                        "Malformed physical hold status"
                    );
                    out = reply;
                } else {
                    let mut expected = [0; 32];
                    expected[..2].copy_from_slice(&[0xfe, op]);
                    ensure!(reply == expected, "Physical hold command rejected");
                    if op == 8 {
                        let locked = self.raw(&[0xfe, 5])?;
                        ensure!(
                            locked[..6] == [0, 0, 0, 0, 0, 5]
                                && locked[6..].iter().all(|b| *b == 0xff),
                            "Physical unlock combination or fresh lock differs"
                        );
                    }
                }
            }
            WireCommand::Transition => {
                ensure!(self.phase == 0, "Transition already attempted");
                self.phase = 1;
                if self.native {
                    let mut p = [0; 64];
                    p[..4].copy_from_slice(&[4, 0xee, 0xee, 0]);
                    ensure!(
                        self.application
                            .as_ref()
                            .context("No OEM application")?
                            .write(&p)?
                            == 64,
                        "Short OEM transition"
                    );
                } else {
                    let reply = self.raw(&[0x0b])?;
                    let mut expected = [0; 32];
                    expected[0] = 0x0b;
                    ensure!(reply == expected, "Project loader transition rejected");
                }
                self.application.take();
                let (dev, _) = self.find(true)?;
                self.loader = Some(dev);
            }
            WireCommand::Loader { command, checksum } => {
                let cmd = match command {
                    1 => Command::Identity,
                    2 => Command::Erase,
                    3 => Command::Begin,
                    4 => Command::Checksum,
                    5 => Command::Restart,
                    _ => bail!("Unknown loader command"),
                };
                let required = match cmd {
                    Command::Identity => 1,
                    Command::Erase => 2,
                    Command::Begin => 3,
                    Command::Checksum => 4,
                    Command::Restart => 5,
                };
                ensure!(self.phase == required, "Invalid loader command order");
                if cmd == Command::Checksum {
                    ensure!(self.blocks == 799, "Incomplete application payload");
                }
                self.phase += 1;
                let dev = self.loader.as_ref().context("No loader")?;
                // hidapi 2.6.6 checks the returned byte count internally and
                // returns IncompleteSendError on a short feature transfer.
                dev.send_feature_report(&loader::command_report(cmd, checksum))?;
                if cmd != Command::Restart {
                    std::thread::sleep(loader::response_delay(cmd));
                    let mut reply = [0; 65];
                    ensure!(
                        dev.get_feature_report(&mut reply)? == 65,
                        "Incomplete loader reply"
                    );
                    loader::validate_reply(&reply, cmd).map_err(anyhow::Error::msg)?;
                }
            }
            WireCommand::Block { bytes } => {
                ensure!(
                    self.phase == 4 && self.blocks < 799 && bytes.len() == 64,
                    "Invalid loader data block"
                );
                self.blocks += 1;
                let mut report = [0; 65];
                report[1..].copy_from_slice(&bytes);
                self.loader
                    .as_ref()
                    .context("No loader")?
                    .send_feature_report(&report)?;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            WireCommand::Reconnect => {
                ensure!(
                    self.phase == 6,
                    "Reconnect before successful flash sequence"
                );
                self.loader.take();
                let (dev, native) = self.find(false)?;
                self.native = native;
                self.application = Some(dev);
            }
        }
        Ok(out)
    }
}
#[cfg(target_os = "linux")]
pub(crate) fn usb_port(path: &std::ffi::CStr) -> Result<std::path::PathBuf> {
    let name = std::path::Path::new(path.to_str()?)
        .file_name()
        .context("Invalid hidraw path")?;
    let hid = std::fs::canonicalize(
        std::path::Path::new("/sys/class/hidraw")
            .join(name)
            .join("device"),
    )?;
    for ancestor in hid.ancestors() {
        if ancestor.join("idVendor").exists() && ancestor.join("idProduct").exists() {
            // Physical port path survives the application/loader VID/PID change.
            return Ok(ancestor.to_path_buf());
        }
    }
    bail!("Physical USB port unavailable")
}
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub(crate) fn usb_port(_path: &std::ffi::CStr) -> Result<std::path::PathBuf> {
    bail!(
        "This platform has no reviewed stable application/loader USB-location binding; installation is blocked"
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn usb_port(path: &std::ffi::CStr) -> Result<std::path::PathBuf> {
    use windows_sys::Win32::Devices::{DeviceAndDriverInstallation::*, Properties::*};
    let interface: Vec<u16> = path.to_str()?.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let mut kind = 0;
        let mut id = [0u16; 1024];
        let mut size = std::mem::size_of_val(&id) as u32;
        ensure!(
            CM_Get_Device_Interface_PropertyW(
                interface.as_ptr(),
                &DEVPKEY_Device_InstanceId,
                &mut kind,
                id.as_mut_ptr().cast(),
                &mut size,
                0
            ) == CR_SUCCESS
                && kind == DEVPROP_TYPE_STRING,
            "USB instance identity unavailable"
        );
        let mut node = 0;
        ensure!(
            CM_Locate_DevNodeW(&mut node, id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) == CR_SUCCESS,
            "USB devnode unavailable"
        );
        for _ in 0..64 {
            let mut id = [0u16; 1024];
            ensure!(
                CM_Get_Device_IDW(node, id.as_mut_ptr(), id.len() as u32, 0) == CR_SUCCESS,
                "USB ancestry ID unavailable"
            );
            let text = String::from_utf16(
                &id[..id
                    .iter()
                    .position(|x| *x == 0)
                    .context("USB ancestry ID incomplete")?],
            )?
            .to_ascii_uppercase();
            ensure!(
                !text.starts_with("BTH") && !text.starts_with("BLUETOOTH"),
                "Bluetooth ancestry is forbidden"
            );
            if text.starts_with("USB\\VID_") && !text.contains("&MI_") {
                let mut paths = [0u16; 4096];
                let mut size = std::mem::size_of_val(&paths) as u32;
                let mut kind = 0;
                ensure!(
                    CM_Get_DevNode_PropertyW(
                        node,
                        &DEVPKEY_Device_LocationPaths,
                        &mut kind,
                        paths.as_mut_ptr().cast(),
                        &mut size,
                        0
                    ) == CR_SUCCESS
                        && kind == DEVPROP_TYPE_STRING_LIST,
                    "Physical USB location paths unavailable"
                );
                let end = paths
                    .iter()
                    .position(|x| *x == 0)
                    .context("USB location path incomplete")?;
                let path = String::from_utf16(&paths[..end])?.to_ascii_uppercase();
                ensure!(
                    !path.is_empty() && path.contains("USB("),
                    "Invalid physical USB location path"
                );
                return Ok(std::path::PathBuf::from(path));
            }
            let mut parent = 0;
            ensure!(
                CM_Get_Parent(&mut parent, node, 0) == CR_SUCCESS,
                "Physical USB ancestor unavailable"
            );
            node = parent;
        }
    }
    bail!("Physical USB ancestry exceeds bound")
}
#[cfg(target_os = "macos")]
pub(crate) fn usb_port(path: &std::ffi::CStr) -> Result<std::path::PathBuf> {
    use std::ffi::{c_char, c_void};
    type CF = *const c_void;
    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IORegistryEntryIDMatching(id: u64) -> CF;
        fn IOServiceGetMatchingService(port: u32, matching: CF) -> u32;
        fn IORegistryEntryCreateCFProperty(entry: u32, key: CF, allocator: CF, options: u32) -> CF;
        fn IOObjectRelease(object: u32) -> i32;
        fn IOObjectConformsTo(object: u32, name: *const c_char) -> u32;
        fn IORegistryEntryGetParentEntry(entry: u32, plane: *const c_char, parent: *mut u32)
        -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(a: CF, s: *const c_char, encoding: u32) -> CF;
        fn CFNumberGetValue(v: CF, kind: i32, out: *mut i64) -> u8;
        fn CFNumberGetTypeID() -> usize;
        fn CFGetTypeID(v: CF) -> usize;
        fn CFRelease(v: CF);
    }
    struct Entry(u32);
    impl Drop for Entry {
        fn drop(&mut self) {
            unsafe {
                IOObjectRelease(self.0);
            }
        }
    }
    let id = path
        .to_str()?
        .strip_prefix("DevSrvsID:")
        .context("Unsupported macOS HID path")?
        .parse::<u64>()?;
    unsafe {
        let matching = IORegistryEntryIDMatching(id);
        ensure!(!matching.is_null(), "USB registry matching unavailable");
        let mut entry = Entry(IOServiceGetMatchingService(0, matching));
        ensure!(entry.0 != 0, "USB registry entry disappeared");
        for _ in 0..64 {
            ensure!(
                IOObjectConformsTo(entry.0, b"IOBluetoothDevice\0".as_ptr().cast()) == 0,
                "Bluetooth ancestry is forbidden"
            );
            if IOObjectConformsTo(entry.0, b"IOUSBHostDevice\0".as_ptr().cast()) != 0
                || IOObjectConformsTo(entry.0, b"IOUSBDevice\0".as_ptr().cast()) != 0
            {
                let key = CFStringCreateWithCString(
                    std::ptr::null(),
                    b"locationID\0".as_ptr().cast(),
                    0x08000100,
                );
                ensure!(!key.is_null(), "USB location key unavailable");
                let value = IORegistryEntryCreateCFProperty(entry.0, key, std::ptr::null(), 0);
                CFRelease(key);
                ensure!(!value.is_null(), "Physical USB location unavailable");
                let mut location = 0i64;
                let valid = CFGetTypeID(value) == CFNumberGetTypeID()
                    && CFNumberGetValue(value, 4, &mut location) != 0;
                CFRelease(value);
                ensure!(
                    valid && location > 0 && location <= u32::MAX as i64,
                    "Malformed physical USB location"
                );
                return Ok(std::path::PathBuf::from(format!(
                    "macos-usb-location:{location:08x}"
                )));
            }
            let mut parent = 0;
            ensure!(
                IORegistryEntryGetParentEntry(entry.0, b"IOService\0".as_ptr().cast(), &mut parent)
                    == 0
                    && parent != 0,
                "Physical USB ancestor unavailable"
            );
            entry = Entry(parent);
        }
    }
    bail!("Physical USB ancestry exceeds bound")
}
