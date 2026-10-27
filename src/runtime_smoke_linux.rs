//! Isolated protocol host for Linux runtime smoke checks. This synthetic host
//! exercises the production SNI transport; it does not render a desktop panel.
use anyhow::{ensure, Context};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::{Arc, Mutex};
use zbus::blocking::{Connection, Proxy};

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
type Registration = Arc<Mutex<Option<(String, String)>>>;

struct SmokeWatcher {
    registration: Registration,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl SmokeWatcher {
    fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        // Only the production object's path form is accepted by this fixture.
        if service != "/StatusNotifierItem" {
            return Err(zbus::fdo::Error::InvalidArgs(
                "unexpected tray object path".into(),
            ));
        }
        let sender = header
            .sender()
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("missing D-Bus sender".into()))?;
        *self.registration.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((sender.to_string(), service.into()));
        Ok(())
    }
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.registration
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|(service, path)| vec![format!("{service}{path}")])
            .unwrap_or_default()
    }
}

pub(crate) struct TrayFixture {
    // Keeping both connections alive preserves the watcher ownership and uses
    // a separate client for activation rather than calling an implementation.
    _watcher: Connection,
    client: Connection,
    registration: Registration,
}

impl TrayFixture {
    pub(crate) const EVIDENCE: &'static str = "synthetic_status_notifier_host";

    pub(crate) fn new() -> anyhow::Result<Self> {
        ensure!(
            crate::isolated_runtime()
                && crate::runtime_safety::smoke_requested()
                && std::env::var("AK820_RUNTIME_SMOKE_TRAY_FIXTURE").as_deref() == Ok("1"),
            "synthetic tray host requires explicit isolated runtime smoke mode"
        );
        let registration = Arc::new(Mutex::new(None));
        let watcher = zbus::blocking::connection::Builder::session()?
            .method_timeout(std::time::Duration::from_secs(1))
            .serve_at(
                WATCHER_PATH,
                SmokeWatcher {
                    registration: registration.clone(),
                },
            )?
            .build()?;
        let bus = Proxy::new(
            &watcher,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )?;
        let owned: bool = bus.call("NameHasOwner", &(WATCHER,))?;
        ensure!(
            !owned,
            "refusing synthetic tray host: a real StatusNotifierWatcher already owns the name"
        );
        // D-Bus DO_NOT_QUEUE (4), without ALLOW_REPLACEMENT or REPLACE_EXISTING.
        // Checking the reply also closes the race after NameHasOwner.
        let reply: u32 = bus.call("RequestName", &(WATCHER, 4u32))?;
        ensure!(
            reply == 1,
            "synthetic tray watcher name was not acquired exclusively"
        );
        drop(bus);
        let client = zbus::blocking::connection::Builder::session()?
            .method_timeout(std::time::Duration::from_secs(1))
            .build()?;
        Ok(Self {
            _watcher: watcher,
            client,
            registration,
        })
    }

    pub(crate) fn registered(&self) -> bool {
        self.registration
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// Inspect exported icon/menu contracts and invoke the real D-Bus Activate.
    pub(crate) fn activate(&self) -> anyhow::Result<()> {
        let (service, path) = self
            .registration
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .context("production tray has not registered with synthetic host")?;
        let item = Proxy::new(
            &self.client,
            service.as_str(),
            path.as_str(),
            "org.kde.StatusNotifierItem",
        )?;
        ensure!(
            item.get_property::<String>("Id")? == "ak-configurator",
            "unexpected tray item identity"
        );
        ensure!(
            item.get_property::<String>("Status")? == "Active",
            "tray item is not active"
        );
        let icons: Vec<(i32, i32, Vec<u8>)> = item.get_property("IconPixmap")?;
        ensure!(
            icons.iter().any(|(w, h, bytes)| *w > 0
                && *h > 0
                && (*w as usize)
                    .checked_mul(*h as usize)
                    .and_then(|n| n.checked_mul(4))
                    == Some(bytes.len())
                && bytes.chunks_exact(4).any(|pixel| pixel[0] != 0)),
            "tray icon has no valid visible ARGB pixmap"
        );
        let menu_path: zbus::zvariant::OwnedObjectPath = item.get_property("Menu")?;
        let menu = Proxy::new(
            &self.client,
            service.as_str(),
            menu_path.as_str(),
            "com.canonical.dbusmenu",
        )?;
        ensure!(
            menu.get_property::<u32>("Version")? == 3,
            "unexpected dbusmenu version"
        );
        type Properties = std::collections::HashMap<String, zbus::zvariant::OwnedValue>;
        type Layout = (i32, Properties, Vec<zbus::zvariant::OwnedValue>);
        let (_revision, layout): (u32, Layout) =
            menu.call("GetLayout", &(0i32, -1i32, Vec::<String>::new()))?;
        ensure!(
            layout.0 == 0 && layout.2.len() == 2,
            "tray menu must export Show and Quit children"
        );
        let (_revision, shallow): (u32, Layout) =
            menu.call("GetLayout", &(0i32, 0i32, Vec::<String>::new()))?;
        ensure!(
            shallow.2.is_empty(),
            "depth-zero menu unexpectedly contains children"
        );
        let (_revision, leaf): (u32, Layout) =
            menu.call("GetLayout", &(1i32, -1i32, vec!["label".to_owned()]))?;
        ensure!(
            leaf.0 == 1 && leaf.2.is_empty() && leaf.1.len() == 1 && leaf.1.contains_key("label"),
            "leaf menu layout or property filtering is incorrect"
        );
        let grouped: Vec<(i32, Properties)> = menu.call(
            "GetGroupProperties",
            &(
                vec![1i32, 2i32],
                vec!["label".to_owned(), "enabled".to_owned()],
            ),
        )?;
        ensure!(
            grouped.len() == 2,
            "grouped menu properties omitted actions"
        );
        for (id, properties) in grouped {
            let expected = match id {
                1 => "Show AK Configurator",
                2 => "Quit AK Configurator",
                _ => anyhow::bail!("unexpected grouped menu item"),
            };
            ensure!(
                properties.len() == 2,
                "grouped property filtering is incorrect"
            );
            let label: &str = properties
                .get("label")
                .context("menu label missing")?
                .try_into()?;
            let enabled = bool::try_from(
                properties
                    .get("enabled")
                    .context("menu enabled missing")?
                    .try_clone()?,
            )?;
            ensure!(
                label == expected && enabled,
                "menu action properties are incorrect"
            );
        }
        let label: zbus::zvariant::OwnedValue = menu.call("GetProperty", &(1i32, "label"))?;
        ensure!(
            <&str>::try_from(&label)? == "Show AK Configurator",
            "singular menu label is incorrect"
        );
        let invalid: zbus::Result<zbus::zvariant::OwnedValue> =
            menu.call("GetProperty", &(999i32, "label"));
        ensure!(
            matches!(invalid, Err(zbus::Error::MethodError(ref name, _, _))
            if name.as_str() == "org.freedesktop.DBus.Error.InvalidArgs"),
            "unknown menu item did not return InvalidArgs"
        );
        let (updates, errors): (Vec<i32>, Vec<i32>) =
            menu.call("AboutToShowGroup", &(vec![0i32, 1i32, 999i32],))?;
        ensure!(
            updates.is_empty() && errors == vec![999],
            "grouped menu preparation returned incorrect errors"
        );
        // Only Show is clicked. Quit is inspected without triggering shutdown.
        let _: () = menu.call(
            "Event",
            &(1i32, "clicked", zbus::zvariant::Value::from(0i32), 0u32),
        )?;
        let failed: Vec<i32> = menu.call(
            "EventGroup",
            &(vec![
                (
                    1i32,
                    "clicked".to_owned(),
                    zbus::zvariant::OwnedValue::from(0i32),
                    0u32,
                ),
                (
                    999i32,
                    "clicked".to_owned(),
                    zbus::zvariant::OwnedValue::from(0i32),
                    0u32,
                ),
            ],),
        )?;
        ensure!(
            failed == vec![999],
            "grouped menu events returned incorrect failed IDs"
        );
        let _: () = item.call("Activate", &(0i32, 0i32))?;
        Ok(())
    }
}

/// Query the native X11 server's map state; an egui viewport flag alone is not
/// evidence that the OS window was hidden or restored. Wayland is unqualified.
pub(crate) fn native_window_visible(frame: &eframe::Frame) -> anyhow::Result<bool> {
    let handle = frame
        .window_handle()
        .context("native window handle unavailable")?;
    let window = match handle.as_raw() {
        RawWindowHandle::Xlib(handle) => handle.window,
        RawWindowHandle::Xcb(handle) => u64::from(handle.window.get()) as _,
        RawWindowHandle::Wayland(_) => {
            anyhow::bail!("Wayland native visibility is not qualified by the X11 smoke fixture")
        }
        _ => anyhow::bail!("runtime smoke requires an X11 native window"),
    };
    let xlib = x11_dl::xlib::Xlib::open().context("load Xlib for native window visibility")?;
    // A dedicated connection avoids borrowing the renderer's X11 connection.
    unsafe {
        let display = (xlib.XOpenDisplay)(std::ptr::null());
        ensure!(
            !display.is_null(),
            "cannot open X11 display for visibility check"
        );
        let mut attributes: x11_dl::xlib::XWindowAttributes = std::mem::zeroed();
        let status = (xlib.XGetWindowAttributes)(display, window, &mut attributes);
        (xlib.XCloseDisplay)(display);
        ensure!(
            status != 0,
            "XGetWindowAttributes failed for native application window"
        );
        Ok(attributes.map_state == x11_dl::xlib::IsViewable)
    }
}
