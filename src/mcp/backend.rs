//! One bounded worker owns all real device handles. Cached status never sends HID.
use super::dto;
use crate::{
    device::{Device, DeviceManager},
    hid::HidDevice,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc, Mutex,
};

#[derive(Clone)]
pub struct Backend {
    inner: Arc<Shared>,
}
struct Shared {
    mock: bool,
    trace: crate::hid::ReadOnlyHidSession,
    nonce: String,
    sequence: AtomicU64,
    busy: AtomicBool,
    cancel: AtomicBool,
    cached: Mutex<Cache>,
    sender: Mutex<Option<mpsc::SyncSender<Job>>>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    #[cfg(test)]
    test_store_root: Option<std::path::PathBuf>,
    #[cfg(test)]
    test_storage_fault: bool,
    #[cfg(test)]
    test_observation: Option<Result<Option<crate::ak820_release::ReleaseObservation>, String>>,
    #[cfg(test)]
    test_prepare_publication: Option<(mpsc::SyncSender<()>, Mutex<mpsc::Receiver<()>>)>,
}
#[derive(Default)]
struct Cache {
    session: Option<Value>,
    operations: Vec<(String, Value)>,
    reports: Vec<(String, Value)>,
}
type Job = Box<dyn FnOnce(&mut Engine, &Shared) + Send>;
struct Engine {
    devices: Vec<(String, Device)>,
    generation: u64,
    hid: Option<HidDevice>,
    session: Option<String>,
    image: Value,
    stopped: bool,
    backups: Vec<BackupBinding>,
    #[cfg(test)]
    test_io: Option<TestIo>,
    #[cfg(test)]
    test_owner: Option<Box<dyn Send>>,
}
struct BackupBinding {
    artifact_id: String,
    session: String,
    generation: u64,
    image: crate::ak820::ImageContract,
    payload_sha256: String,
    manifest_sha256: String,
    captured_utc: String,
}
#[cfg(test)]
type TestOpen = Box<dyn FnMut(&Device) -> anyhow::Result<(HidDevice, Box<dyn Send>)> + Send>;
#[cfg(test)]
struct TestIo {
    candidates: Vec<Device>,
    open: TestOpen,
}
impl Backend {
    pub fn new(mock: bool) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Job>(1);
        let inner = Arc::new(Shared {
            mock,
            trace: if mock {
                crate::hid::ReadOnlyHidSession::default()
            } else {
                crate::hid::enforce_read_only_hid()
            },
            nonce: format!(
                "{:x}-{:x}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            sequence: AtomicU64::new(1),
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            cached: Mutex::new(Cache::default()),
            sender: Mutex::new(Some(sender)),
            worker: Mutex::new(None),
            #[cfg(test)]
            test_store_root: None,
            #[cfg(test)]
            test_storage_fault: false,
            #[cfg(test)]
            test_observation: None,
            #[cfg(test)]
            test_prepare_publication: None,
        });
        let shared = inner.clone();
        let worker = std::thread::spawn(move || {
            let mut engine = Engine {
                devices: vec![],
                generation: 0,
                hid: None,
                session: None,
                image: Value::Null,
                stopped: false,
                backups: Vec::new(),
                #[cfg(test)]
                test_io: None,
                #[cfg(test)]
                test_owner: None,
            };
            while let Ok(job) = receiver.recv() {
                job(&mut engine, &shared);
            }
        });
        *inner.worker.lock().unwrap() = Some(worker);
        Self { inner }
    }
    pub fn shutdown(&self) {
        self.inner.cancel.store(true, Ordering::Release);
        self.inner.sender.lock().unwrap().take();
        if let Some(worker) = self.inner.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
    fn result(&self, data: Value) -> Value {
        envelope(&self.inner, "ok", data, None)
    }
    fn error(&self, code: &str, message: &str) -> Value {
        envelope(&self.inner, "failed", Value::Null, Some((code, message)))
    }
    // Cancellation detaches only this response. A submitted bounded worker
    // retains its owner; explicit operation.cancel and EOF control the worker.
    pub fn cancelled_request(&self) -> Value {
        envelope(&self.inner,"cancelled",json!({"response_detached":true,"worker_cancel_requested":false,"owner_released":false}),Some(("CANCELLED","Request response cancelled; worker retains ownership until completion. Use ak820_operation to cancel a tracked operation")))
    }
    pub async fn call(&self, name: &str, args: Value) -> Value {
        macro_rules! parse {
            ($t:ty) => {
                match serde_json::from_value::<$t>(args) {
                    Ok(v) => v,
                    Err(_) => {
                        return self.error(
                            "INVALID_ARGUMENT",
                            "Arguments do not match the closed P0 contract",
                        )
                    }
                }
            };
        }
        match name {
            "ak820_devices" => {
                let p = parse!(dto::Devices);
                self.work(move |e, s| e.devices(s, p.refresh)).await
            }
            "ak820_session" => match parse!(dto::Session) {
                dto::Session::Open {
                    device_id,
                    load: dto::DiagnosticLoad::Diagnostics,
                } => self.start_open(device_id),
                dto::Session::Status { device_session_id } => {
                    self.cached_session(&device_session_id)
                }
                dto::Session::Close { device_session_id } => {
                    self.work(move |e, s| e.close(s, &device_session_id)).await
                }
            },
            "ak820_read" => {
                let p = parse!(dto::Read);
                self.work(move |e, s| e.read(s, &p.device_session_id, p.section))
                    .await
            }
            "ak820_prepare" => {
                let p = parse!(dto::Prepare);
                self.start_prepare(p)
            }
            "ak820_backup" => {
                let p = parse!(dto::Backup);
                self.start_backup(p.device_session_id, p.expected_generation)
            }
            "ak820_tests" => match parse!(dto::Tests) {
                dto::Tests::List => {
                    let mut suites = vec![
                        json!({"suite_id":"protocol.smoke","version":1,"write_budget":0,"physical_requirements":[],"evidence":if self.inner.mock {"host_model"} else {"device_protocol"}}),
                    ];
                    if self.inner.mock {
                        suites.push(json!({"suite_id":"protocol.errors","version":1,"write_budget":0,"evidence":"host_model","synthetic":true,"physical_requirements":[]}));
                    }
                    self.result(json!({"suites":suites}))
                }
                dto::Tests::Run {
                    device_session_id,
                    suite_id,
                    seed,
                } => {
                    if matches!(suite_id, dto::Suite::Errors) {
                        if !self.inner.mock {
                            return self.error(
                                "UNSUPPORTED_FEATURE",
                                "Synthetic fault suite requires the development mock backend",
                            );
                        }
                        let data = super::mock::error_checks();
                        let failed = data["checks"].as_array().is_none_or(|checks| {
                            checks.iter().any(|check| check["status"] == "FAIL")
                        });
                        return envelope(
                            &self.inner,
                            if failed { "failed" } else { "ok" },
                            data,
                            if failed {
                                Some(("READBACK_MISMATCH", "Synthetic protocol assertion failed"))
                            } else {
                                None
                            },
                        );
                    }
                    let Some(device_session_id) = device_session_id else {
                        return self.error("INVALID_ARGUMENT", "protocol.smoke requires a session");
                    };
                    self.work(move |e,s| {
                    let status=e.read(s,&device_session_id,dto::Section::Storage);
                    if status["error"].is_object() { return status; }
                    let report_id=format!("report-{}-{}",s.nonce,s.sequence.fetch_add(1,Ordering::Relaxed));
                    let data=json!({"suite_id":"protocol.smoke","seed":seed,"checks":[{"id":"diagnostic_status","status":"PASS"},{"id":"physical_behavior","status":"SKIP","reason":"No physical fixture"}],"setters_sent":0,"full_acceptance":false,"report_id":report_id});
                    let report=envelope(s,"ok",data,None);
                    let mut cache=s.cached.lock().unwrap();
                    if cache.reports.len()==32 {cache.reports.remove(0);}
                    cache.reports.push((report_id,report.clone())); report
                }).await
                }
            },
            "ak820_trace" => {
                let p = parse!(dto::Trace);
                match p {
                    dto::Trace::Start {
                        device_session_id,
                        mode: dto::TraceMode::Metadata,
                        max_bytes,
                        max_seconds,
                    } => {
                        if max_bytes == 0
                            || max_bytes > 8 * 1024 * 1024
                            || max_seconds == 0
                            || max_seconds > 60
                        {
                            return self.error("LIMIT_EXCEEDED", "Trace exceeds P0 limits");
                        }
                        let session = self.cached_session(&device_session_id);
                        if session["error"].is_object() {
                            return session;
                        }
                        let id = format!(
                            "trace-{}-{}",
                            self.inner.nonce,
                            self.inner.sequence.fetch_add(1, Ordering::Relaxed)
                        );
                        if self
                            .inner
                            .trace
                            .trace_start(id.clone(), max_bytes as usize, max_seconds)
                            .is_err()
                        {
                            return self.error("OPERATION_BUSY", "A trace is already recording");
                        }
                        {
                            let mut status = self.inner.trace.trace_status(&id, false).unwrap();
                            status["resource_uri"] = json!(format!("ak820://trace/{id}/page/0"));
                            status["synthetic"] = json!(self.inner.mock);
                            self.result(status)
                        }
                    }
                    dto::Trace::Stop { trace_id } => {
                        match self.inner.trace.trace_status(&trace_id, true) {
                            Some(v) => self.result(v),
                            None => self.error("INVALID_ARGUMENT", "Unknown trace ID"),
                        }
                    }
                    dto::Trace::Status { trace_id } => {
                        match self.inner.trace.trace_status(&trace_id, false) {
                            Some(v) => self.result(v),
                            None => self.error("INVALID_ARGUMENT", "Unknown trace ID"),
                        }
                    }
                }
            }
            "ak820_operation" => {
                let p = parse!(dto::Operation);
                let (id, cancel) = match p {
                    dto::Operation::Status { operation_id } => (operation_id, false),
                    dto::Operation::Cancel { operation_id } => (operation_id, true),
                };
                let cache = self.inner.cached.lock().unwrap();
                if let Some((_, op)) = cache.operations.iter().find(|(key, _)| key == &id) {
                    if cancel && op["state"] == "running" && op["cancellable_now"] == true {
                        self.inner.cancel.store(true, Ordering::Release);
                    }
                    self.result(op.clone())
                } else {
                    self.error("INVALID_ARGUMENT", "Unknown operation ID")
                }
            }
            _ => self.error("INVALID_ARGUMENT", "Unknown tool"),
        }
    }
    fn cached_session(&self, id: &str) -> Value {
        let cache = self.inner.cached.lock().unwrap();
        match &cache.session {
            Some(v) if v["device_session_id"] == id => self.result(v.clone()),
            _ => self.error("SESSION_STALE", "Unknown or closed session"),
        }
    }
    async fn work(&self, f: impl FnOnce(&mut Engine, &Shared) -> Value + Send + 'static) -> Value {
        if self.inner.busy.swap(true, Ordering::AcqRel) {
            return self.error("OPERATION_BUSY", "One device operation is already running");
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let job: Job = Box::new(move |e, s| {
            let result = f(e, s);
            s.busy.store(false, Ordering::Release);
            let _ = tx.send(result);
        });
        if self
            .inner
            .sender
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(|s| s.try_send(job).is_err())
        {
            self.inner.busy.store(false, Ordering::Release);
            return self.error("SESSION_STOPPED", "Worker stopped");
        }
        rx.await
            .unwrap_or_else(|_| self.error("SESSION_STOPPED", "Worker stopped"))
    }
    fn start_open(&self, id: String) -> Value {
        if self.inner.busy.swap(true, Ordering::AcqRel) {
            return self.error("OPERATION_BUSY", "One device operation is already running");
        }
        self.inner.cancel.store(false, Ordering::Release);
        let op = format!(
            "op-{}-{}",
            self.inner.nonce,
            self.inner.sequence.fetch_add(1, Ordering::Relaxed)
        );
        {
            let mut cache = self.inner.cached.lock().unwrap();
            if cache.operations.len() == 32 {
                cache.operations.remove(0);
            }
            cache.operations.push((op.clone(),json!({"operation_id":op,"state":"running","stage":"identifying","cancellable_now":true,"lease_held":false})));
        }
        let op_worker = op.clone();
        let job: Job = Box::new(move |e, s| {
            let result = e.open(s, &id);
            let cancel_requested = s.cancel.load(Ordering::Acquire);
            let mut cache = s.cached.lock().unwrap();
            if let Some((_, v)) = cache.operations.iter_mut().find(|(id, _)| id == &op_worker) {
                *v = json!({"operation_id":op_worker,"state":if result["error"]["code"]=="CANCELLED"{"cancelled"}else if result["error"].is_object(){"failed"}else{"completed"},"stage":"terminal","cancellable_now":false,"cancel_requested":cancel_requested,"lease_held":e.hid.is_some(),"result":result});
            }
            s.busy.store(false, Ordering::Release);
        });
        if self
            .inner
            .sender
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(|s| s.try_send(job).is_err())
        {
            self.inner.busy.store(false, Ordering::Release);
            return self.error("SESSION_STOPPED", "Worker stopped");
        }
        envelope(&self.inner, "accepted", json!({"operation_id":op}), None)
    }
    fn start_prepare(&self, p: dto::Prepare) -> Value {
        if self.inner.busy.swap(true, Ordering::AcqRel) {
            return self.error("OPERATION_BUSY", "One device operation is already running");
        }
        self.inner.cancel.store(false, Ordering::Release);
        let op = format!(
            "op-{}-{}",
            self.inner.nonce,
            self.inner.sequence.fetch_add(1, Ordering::Relaxed)
        );
        {
            let mut cache = self.inner.cached.lock().unwrap();
            if cache.operations.len() == 32 {
                cache.operations.remove(0);
            }
            cache.operations.push((op.clone(), json!({"operation_id":op,"state":"running","stage":"queued","cancellable_now":true,"lease_held":null})));
        }
        let op_worker = op.clone();
        let job: Job = Box::new(move |e, s| {
            {
                let mut cache = s.cached.lock().unwrap();
                if let Some((_, v)) = cache.operations.iter_mut().find(|(id, _)| id == &op_worker) {
                    v["stage"] = json!("preparing");
                    v["lease_held"] = json!(e.hid.is_some());
                }
            }
            let result = e.prepare(s, p);
            let mut cache = s.cached.lock().unwrap();
            if let Some((_, v)) = cache.operations.iter_mut().find(|(id, _)| id == &op_worker) {
                *v = json!({"operation_id":op_worker,"state":if result["error"]["code"]=="CANCELLED" {"cancelled"} else if result["error"].is_object() {"failed"} else {"completed"},"stage":"terminal","cancellable_now":false,"lease_held":e.hid.is_some(),"result":result});
            }
            s.busy.store(false, Ordering::Release);
        });
        if self
            .inner
            .sender
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(|s| s.try_send(job).is_err())
        {
            self.inner
                .cached
                .lock()
                .unwrap()
                .operations
                .retain(|(id, _)| id != &op);
            self.inner.busy.store(false, Ordering::Release);
            return self.error("SESSION_STOPPED", "Worker stopped");
        }
        envelope(&self.inner, "accepted", json!({"operation_id":op}), None)
    }
    fn start_backup(&self, id: String, generation: u64) -> Value {
        if self.inner.busy.swap(true, Ordering::AcqRel) {
            return self.error("OPERATION_BUSY", "One device operation is already running");
        }
        self.inner.cancel.store(false, Ordering::Release);
        let op = format!(
            "op-{}-{}",
            self.inner.nonce,
            self.inner.sequence.fetch_add(1, Ordering::Relaxed)
        );
        {
            let mut cache = self.inner.cached.lock().unwrap();
            if cache.operations.len() == 32 {
                cache.operations.remove(0);
            }
            cache.operations.push((op.clone(), json!({"operation_id":op,"state":"running","stage":"capture","cancellable_now":true,"lease_held":true})));
        }
        let op_worker = op.clone();
        let job: Job = Box::new(move |e, s| {
            let result = e.backup(s, &id, generation);
            let mut cache = s.cached.lock().unwrap();
            if let Some((_, v)) = cache.operations.iter_mut().find(|(id, _)| id == &op_worker) {
                *v = json!({"operation_id":op_worker,"state":if result["error"]["code"]=="CANCELLED" {"cancelled"} else if result["error"].is_object() {"failed"} else {"completed"},"stage":"terminal","cancellable_now":false,"lease_held":e.hid.is_some(),"result":result});
            }
            s.busy.store(false, Ordering::Release);
        });
        if self
            .inner
            .sender
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(|s| s.try_send(job).is_err())
        {
            self.inner.busy.store(false, Ordering::Release);
            return self.error("SESSION_STOPPED", "Worker stopped");
        }
        envelope(&self.inner, "accepted", json!({"operation_id":op}), None)
    }
    pub fn resources(&self) -> Vec<(String, String)> {
        let mut out = vec![
            ("ak820://server".into(), "Server policy".into()),
            (
                format!(
                    "ak820://catalog/{}",
                    crate::app::ak820_beta1_profiles::SOURCE_CATALOG_SHA256
                ),
                "Public beta.3 catalog".into(),
            ),
            ("ak820://inputs".into(), "Registered inputs".into()),
        ];
        out.extend(
            self.inner
                .cached
                .lock()
                .unwrap()
                .reports
                .iter()
                .map(|(id, _)| (format!("ak820://report/{id}"), "Diagnostic report".into())),
        );
        out
    }
    pub fn resource(&self, uri: &str) -> Option<Value> {
        match uri {
            "ak820://server" => Some(
                json!({"api":"ak820-mcp-result-1","sdk":"rmcp-3.8.0","application":env!("CARGO_PKG_VERSION"),"backend":if self.inner.mock {"mock"}else{"hid"},"access":"read-only","limits":{"page_bytes":65536,"sessions":1,"operations":1},"trace_available":true,"physical_behavior_validated":false}),
            ),
            "ak820://inputs" => Some(json!({"inputs":[]})),
            _ => {
                if uri
                    == format!(
                        "ak820://catalog/{}",
                        crate::app::ak820_beta1_profiles::SOURCE_CATALOG_SHA256
                    )
                {
                    return Some(catalog());
                }
                if let Some(rest) = uri.strip_prefix("ak820://trace/") {
                    let (id, page) = rest.split_once("/page/")?;
                    return self.inner.trace.trace_page(id, page.parse().ok()?);
                }
                let cache = self.inner.cached.lock().unwrap();
                cache
                    .reports
                    .iter()
                    .find(|(id, _)| uri == format!("ak820://report/{id}"))
                    .map(|(_, v)| v.clone())
            }
        }
    }
}
fn envelope(s: &Shared, status: &str, data: Value, error: Option<(&str, &str)>) -> Value {
    json!({"format":"ak820-mcp-result-1","request_id":format!("req-{}",s.sequence.fetch_add(1,Ordering::Relaxed)),"status":status,"backend":if s.mock {"mock"}else{"hid"},"evidence":if s.mock {"host_model"}else{"device_protocol"},"image":data.get("image").cloned().unwrap_or(Value::Null),"device_session_id":data.get("device_session_id").cloned().unwrap_or(Value::Null),"session_generation":data.get("session_generation").cloned().unwrap_or(Value::Null),"data":data,"verification":{"setters_sent":0,"nvm_readback":"not_requested","physical_behavior_validated":false,"exclusions":["physical switch, encoder, LED and retention behavior"]},"artifacts":[],"error":error.map(|(code,message)|json!({"code":code,"message":message,"stage":"diagnostics","retryable":false,"recovery_required":false,"details":{}}))})
}
fn catalog() -> Value {
    let c = crate::app::ak820_beta1_profiles::catalog();
    json!({"catalog_hash":crate::app::ak820_beta1_profiles::SOURCE_CATALOG_SHA256,"release":crate::app::ak820_beta1_profiles::BETA_RELEASE,"provenance":"compiled public catalog; not an attached-device observation","profiles":c.images.iter().map(|p|json!({"profile":p.profile.as_str(),"features":p.features,"schema":p.settings_schema,"ihx_sha256":p.build_identity.ihx_sha256,"hardware_validated":p.hardware_validated})).collect::<Vec<_>>()})
}
/// Shared pure preparation path. Inputs must already have passed the caller's
/// source and release checks; it performs no IO and grants no execution authority.
fn prepare_image(
    rom: &[u8],
    profile: crate::app::ak820_beta1_profiles::ProfileId,
    mode: dto::SettingsMode,
    admission: &crate::ak820_release::ReleaseAdmission,
) -> crate::ak820_settings_backup::BackupResult<(
    crate::ak820_install_executor::PreparedInstallation,
    &'static str,
    String,
)> {
    use crate::ak820_install_executor::{
        InstallRequest, InstallSettingsChoice, PreparedInstallation,
    };
    use crate::ak820_settings_backup::sha256;
    let source_kind = crate::ak820_installer::classify_snapshot(rom)?;
    let image = admission.image();
    let (settings, conversion, hash) = match mode {
        dto::SettingsMode::Defaults => {
            let nvm = crate::ak820_release::bundled_defaults_for_image(image)?;
            (InstallSettingsChoice::Defaults, "defaults", sha256(&nvm))
        }
        dto::SettingsMode::Preserve => {
            let nvm =
                crate::ak820_install_executor::preserved_nvm(source_kind, rom, profile, image)?;
            let conversion = if crate::ak820_install_executor::preservation_materializes_beta4_qs(
                source_kind,
                rom,
                image,
            ) {
                "materialize_beta4_qs_default"
            } else {
                "preserve"
            };
            let hash = sha256(&nvm);
            (
                InstallSettingsChoice::PreservedInPayload {
                    nvm_sha256: hash.clone(),
                },
                conversion,
                hash,
            )
        }
    };
    let prepared = PreparedInstallation::prepare(
        InstallRequest::BundledFirmware {
            profile,
            image,
            recovery: None,
            settings,
        },
        rom.to_vec(),
        Some(admission),
        None,
    )?;
    Ok((prepared, conversion, hash))
}
impl Engine {
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    fn prepare(&mut self, s: &Shared, _p: dto::Prepare) -> Value {
        envelope(
            s,
            "failed",
            Value::Null,
            Some(("STORAGE_ERROR", "Private artifact storage is unavailable")),
        )
    }

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn prepare(&mut self, s: &Shared, p: dto::Prepare) -> Value {
        use super::immutable_plan::{Plan, Source, Target};
        use crate::ak820_release::{InstallerRelease, ReleaseAdmission};
        use crate::ak820_settings_backup::{sha256, versioned_metadata};
        let fail = |code, message| envelope(s, "failed", Value::Null, Some((code, message)));
        if self.session.as_deref() != Some(&p.device_session_id)
            || self.generation != p.expected_generation
        {
            return fail("SESSION_STALE", "Session or generation changed");
        }
        if self.stopped {
            return fail("SESSION_STOPPED", "Session is stopped");
        }
        if s.cancel.load(Ordering::Acquire) {
            return fail("CANCELLED", "Preparation cancelled");
        }
        let Some(binding) = self.backups.iter().find(|b| {
            b.artifact_id == p.backup_artifact_id
                && b.session == p.device_session_id
                && b.generation == p.expected_generation
        }) else {
            return fail(
                "BACKUP_INVALID",
                "Backup is not bound to this session and generation",
            );
        };
        #[cfg(test)]
        let store_result = if s.test_storage_fault {
            Err(std::io::Error::other("injected storage fault"))
        } else if let Some(root) = &s.test_store_root {
            super::artifacts::platform::Store::test_open_in(root)
        } else {
            Err(std::io::Error::other("test storage root required"))
        };
        #[cfg(not(test))]
        let store_result = super::artifacts::platform::Store::open();
        let Ok(store) = store_result else {
            return fail("STORAGE_ERROR", "Private artifact storage is unavailable");
        };
        let Ok((manifest, payload)) = store.read(&binding.artifact_id) else {
            return fail("STORAGE_ERROR", "Private backup could not be read");
        };
        if manifest.kind != super::artifacts::Kind::Backup
            || manifest.payload_sha256 != binding.payload_sha256
            || sha256(&payload) != binding.payload_sha256
            || !manifest.complete
        {
            return fail("BACKUP_INVALID", "Backup binding changed");
        }
        let Ok(loaded) =
            crate::ak820_backup_files::decode_private_loaded_capture(&payload, binding.image)
        else {
            return fail("BACKUP_INVALID", "Backup validation failed");
        };
        let archive = loaded.archive();
        if archive.manifest_sha256 != binding.manifest_sha256
            || archive.source.code_sha256 != manifest.source_code_sha256
        {
            return fail("BACKUP_INVALID", "Backup identity changed");
        }
        let (release, image_qs) = match binding.image {
            crate::ak820::ImageContract::Beta5(_) => ("v0.6.0-beta.5", 3),
            crate::ak820::ImageContract::Beta3(_) => ("v0.6.0-beta.3", 1),
            _ => return fail("UNKNOWN_IMAGE", "Source image is not admitted"),
        };
        if manifest.source_release != release || manifest.qs_version != image_qs {
            return fail("BACKUP_INVALID", "Backup source metadata changed");
        }
        let profile = p.target_profile.native();
        let selected = match p.target_release {
            dto::TargetRelease::Beta5 => InstallerRelease::Beta5,
        };
        let Ok(mut admission) = ReleaseAdmission::selected(selected, profile, p.prerelease_opt_in)
        else {
            return fail("ADMISSION_DENIED", "Target release is not admitted");
        };
        #[cfg(not(test))]
        let observation_result = crate::app::read_release_observation();
        // Tests do not inspect the user's settings directory.
        #[cfg(test)]
        let observation_result = s.test_observation.clone().unwrap_or(Ok(None));
        let observation = match observation_result {
            Ok(value) => value,
            Err(_) => {
                return fail(
                    "OBSERVATION_ERROR",
                    "Release observation cannot be verified",
                )
            }
        };
        let (observation_status, observation_sha256, observation_checked_at) =
            if let Some(observation) = observation {
                if observation.blocks_selected(selected) {
                    return fail(
                        "ADMISSION_DENIED",
                        "Selected release is blocked by a saved observation",
                    );
                }
                let Ok(value) = serde_json::to_value(&observation) else {
                    return fail(
                        "OBSERVATION_ERROR",
                        "Release observation cannot be verified",
                    );
                };
                let Ok(bytes) = crate::ak820_settings_backup::canonical(&value) else {
                    return fail(
                        "OBSERVATION_ERROR",
                        "Release observation cannot be verified",
                    );
                };
                let checked = observation.checked_at.clone();
                admission = match admission.with_observation(&observation) {
                    Ok(a) => a,
                    Err(_) => return fail("ADMISSION_DENIED", "Selected release is blocked"),
                };
                ("persisted_block_only", Some(sha256(&bytes)), checked)
            } else {
                ("unknown", None, None)
            };
        let rom = &loaded.files()["before-full-rom.bin"];
        let target_image = admission.image();
        let Ok((prepared, conversion, nvm_hash)) =
            prepare_image(rom, profile, p.settings_mode, &admission)
        else {
            return fail(
                "ADMISSION_DENIED",
                "Source and target installation pair is not admitted",
            );
        };
        let row = target_image
            .matrix_profile()
            .expect("admitted image has matrix");
        let Some(identity) = row.settings_identity.as_ref() else {
            return fail("ADMISSION_DENIED", "Target identity unavailable");
        };
        let source_qs = if image_qs == 3 {
            match versioned_metadata(binding.image, loaded.nvm()) {
                Ok(m) => m["qs"]["record_version"].as_u64().map(|n| n as u32),
                Err(_) => return fail("BACKUP_INVALID", "Source QS metadata invalid"),
            }
        } else {
            None
        };
        let now = chrono::Utc::now();
        let expires = now + chrono::Duration::minutes(15);
        let plan = Plan {
            format: "ak820.mcp.plan/v1".into(),
            operation: "install-preview".into(),
            source: Source {
                session: binding.session.clone(),
                generation: binding.generation,
                backup_id: binding.artifact_id.clone(),
                backup_sha256: binding.payload_sha256.clone(),
                native_manifest_sha256: binding.manifest_sha256.clone(),
                full_rom_sha256: sha256(rom),
                release: release.into(),
                profile: binding
                    .image
                    .profile()
                    .expect("validated profile")
                    .as_str()
                    .into(),
                code_sha256: archive.source.code_sha256.clone(),
                image_qs,
                persisted_qs: source_qs,
                captured_utc: binding.captured_utc.clone(),
            },
            target: Target {
                release: selected.version().into(),
                profile: profile.as_str().into(),
                code_sha256: identity.code_sha256.clone(),
                ihx_sha256: identity.ihx_sha256.clone(),
                map_sha256: row.identity.map_sha256.clone(),
                image_qs: 3,
                catalog_sha256: admission.metadata_sha256().into(),
                matrix_sha256: sha256(include_bytes!(
                    "../../assets/ak820/beta5-firmware-matrix.json"
                )),
                capability_sha256: sha256(include_bytes!(
                    "../../assets/ak820/beta5-capability-catalog.json"
                )),
                profiles_sha256: sha256(include_bytes!("../../assets/ak820/beta5-profiles.json")),
            },
            settings_mode: match p.settings_mode {
                dto::SettingsMode::Defaults => "defaults",
                dto::SettingsMode::Preserve => "preserve",
            }
            .into(),
            settings_conversion: conversion.into(),
            expected_nvm_sha256: nvm_hash,
            payload_sha256: prepared.summary().payload_sha256.clone(),
            expected_full_rom_sha256: prepared.summary().expected_sha256.clone(),
            prerelease_opt_in: p.prerelease_opt_in,
            observation_status: observation_status.into(),
            observation_sha256,
            observation_checked_at,
            issued_utc: now.to_rfc3339(),
            expires_utc: expires.to_rfc3339(),
            settings_writes_authorized: false,
        };
        let Ok((encoded, digest)) = super::immutable_plan::encode(&plan) else {
            return fail("PLAN_INVALID", "Plan encoding failed");
        };
        if super::immutable_plan::validate(&encoded, &plan, now).is_err() {
            return fail("PLAN_INVALID", "Plan validation failed");
        }
        {
            let mut cache = s.cached.lock().unwrap();
            if s.cancel.load(Ordering::Acquire) {
                return fail("CANCELLED", "Preparation cancelled before publication");
            }
            for (_, op) in &mut cache.operations {
                if op["state"] == "running" {
                    op["stage"] = json!("publishing");
                    op["cancellable_now"] = json!(false);
                }
            }
        }
        #[cfg(test)]
        if let Some((started, release)) = &s.test_prepare_publication {
            let _ = started.send(());
            let _ = release
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(10));
        }
        let source = super::artifacts::Source {
            code_sha256: &plan.source.code_sha256,
            release,
            qs_version: image_qs,
            complete: true,
            plan_expiry_utc: Some(&plan.expires_utc),
        };
        let Ok((artifact_id, _)) = store.put(super::artifacts::Kind::Plan, &encoded, source) else {
            return fail(
                "STORAGE_ERROR",
                "Private plan publication failed or capacity was reached",
            );
        };
        envelope(
            s,
            "ok",
            json!({"device_session_id":p.device_session_id,"session_generation":p.expected_generation,"artifact_id":artifact_id,"plan_sha256":digest,"expires_utc":plan.expires_utc,"observation_status":plan.observation_status,"settings_writes_authorized":false}),
            None,
        )
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    fn backup(&mut self, s: &Shared, _id: &str, _generation: u64) -> Value {
        envelope(
            s,
            "failed",
            Value::Null,
            Some(("STORAGE_ERROR", "Private artifact storage is unavailable")),
        )
    }
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn backup(&mut self, s: &Shared, id: &str, generation: u64) -> Value {
        let fail = |code, message| envelope(s, "failed", Value::Null, Some((code, message)));
        if self.session.as_deref() != Some(id) || self.generation != generation {
            return fail("SESSION_STALE", "Session or generation changed");
        }
        if self.stopped {
            return fail("SESSION_STOPPED", "Session is stopped");
        }
        if s.cancel.load(Ordering::Acquire) {
            return fail("CANCELLED", "Backup cancelled before capture");
        }
        if s.mock {
            return fail(
                "UNSUPPORTED_FEATURE",
                "The synthetic diagnostic fixture does not establish a trusted complete ROM",
            );
        }
        let Some(hid) = self.hid.as_ref() else {
            return fail("DISCONNECTED", "Device owner is unavailable");
        };
        let progress = std::sync::atomic::AtomicU32::new(0);
        let capture = match hid.ak820_capture_diagnostic_backup(&s.cancel, &progress) {
            Ok(capture) => capture,
            Err(error) => {
                let failed = hid.ak820_failure().is_some();
                let code = if !failed && s.cancel.load(Ordering::Acquire) {
                    "CANCELLED"
                } else {
                    classify(&error)
                };
                if failed {
                    self.stopped = true;
                }
                return fail(code, "Verified backup capture stopped");
            }
        };
        let captured_utc = chrono::Utc::now().to_rfc3339();
        let (payload, archive) = match crate::ak820_backup_files::encode_private_capture(&capture) {
            Ok(v) => v,
            Err(_) => {
                return fail(
                    "BACKUP_INVALID",
                    "Captured backup failed archive validation",
                )
            }
        };
        if s.cancel.load(Ordering::Acquire) {
            return fail("CANCELLED", "Backup cancelled before publication");
        }
        let image = capture.image();
        let (release, qs_version) = match image {
            crate::ak820::ImageContract::Beta3(_) => ("v0.6.0-beta.3", 1),
            crate::ak820::ImageContract::Beta5(_) => ("v0.6.0-beta.5", 3),
            _ => {
                return fail(
                    "UNKNOWN_IMAGE",
                    "Image is not admitted for diagnostic backup",
                )
            }
        };
        #[cfg(test)]
        let store_result = if s.test_storage_fault {
            Err(std::io::Error::other("injected private storage fault"))
        } else if let Some(root) = &s.test_store_root {
            super::artifacts::platform::Store::test_open_in(root)
        } else {
            Err(std::io::Error::other("test storage root required"))
        };
        #[cfg(not(test))]
        let store_result = super::artifacts::platform::Store::open();
        let store = match store_result {
            Ok(store) => store,
            Err(_) => return fail("STORAGE_ERROR", "Private artifact storage is unavailable"),
        };
        let durability = match store.durability() {
            super::artifacts::Durability::DirectorySync => "directory_sync",
            super::artifacts::Durability::DirectoryFlushUnsupported => {
                "directory_flush_unsupported"
            }
        };
        {
            let mut cache = s.cached.lock().unwrap();
            if s.cancel.load(Ordering::Acquire) {
                return fail("CANCELLED", "Backup cancelled before publication");
            }
            for (_, op) in &mut cache.operations {
                if op["state"] == "running" {
                    op["stage"] = json!("publishing");
                    op["cancellable_now"] = json!(false);
                }
            }
        }
        let source = super::artifacts::Source {
            code_sha256: &capture.identity().code_sha256,
            release,
            qs_version,
            complete: true,
            plan_expiry_utc: None,
        };
        let (artifact_id, manifest) =
            match store.put(super::artifacts::Kind::Backup, &payload, source) {
                Ok(v) => v,
                Err(_) => {
                    return fail(
                        "STORAGE_ERROR",
                        "Private backup publication failed or capacity was reached",
                    )
                }
            };
        self.backups.push(BackupBinding {
            artifact_id: artifact_id.clone(),
            session: id.to_owned(),
            generation,
            image,
            payload_sha256: manifest.payload_sha256.clone(),
            manifest_sha256: archive.manifest_sha256.clone(),
            captured_utc,
        });
        envelope(
            s,
            "ok",
            json!({"device_session_id":id,"session_generation":generation,"image":self.image,"artifact_id":artifact_id,"payload_sha256":manifest.payload_sha256,"payload_length":manifest.payload_length,"native_manifest_sha256":archive.manifest_sha256,"qs_version":qs_version,"complete":true,"durability":durability}),
            None,
        )
    }
    fn devices(&mut self, s: &Shared, refresh: bool) -> Value {
        if self.session.is_some() && refresh {
            return envelope(
                s,
                "failed",
                Value::Null,
                Some((
                    "OPERATION_BUSY",
                    "Close the session before refreshing candidates",
                )),
            );
        }
        if s.mock {
            self.generation = self.generation.max(1);
            return envelope(
                s,
                "ok",
                json!({"devices":[{"device_id":"mock-1","label":"Synthetic AK820 diagnostic fixture","vid":crate::ak820::VID,"pid":crate::ak820::PID,"usage_page":crate::ak820::RAW_HID_USAGE_PAGE,"usage":crate::ak820::RAW_HID_USAGE,"identity_status":"synthetic_unverified","connection_state":"fixture"}],"enumeration_generation":self.generation,"overflow":false}),
                None,
            );
        }
        if self.generation == 0 || refresh {
            #[cfg(test)]
            let scanned = if let Some(io) = &self.test_io {
                Ok(io.candidates.clone())
            } else {
                DeviceManager::scan_usb_devices()
            };
            #[cfg(not(test))]
            let scanned = DeviceManager::scan_usb_devices();
            let devices = match scanned {
                Ok(v) => v,
                Err(_) => {
                    return envelope(
                        s,
                        "failed",
                        Value::Null,
                        Some((
                            "ACCESS_DENIED",
                            "USB discovery failed; inspect local HID permissions",
                        )),
                    )
                }
            };
            self.generation += 1;
            self.devices = devices
                .into_iter()
                .filter(|d| d.is_ak820())
                .take(33)
                .enumerate()
                .map(|(n, d)| (format!("dev-{}-{}-{}", s.nonce, self.generation, n + 1), d))
                .collect();
        }
        envelope(
            s,
            "ok",
            json!({"devices":self.devices.iter().take(32).map(|(id,_)|json!({"device_id":id,"label":"AK820 Raw HID candidate","vid":crate::ak820::VID,"pid":crate::ak820::PID,"usage_page":crate::ak820::RAW_HID_USAGE_PAGE,"usage":crate::ak820::RAW_HID_USAGE,"connection_state":"candidate","identity_status":"unverified"})).collect::<Vec<_>>(),"enumeration_generation":self.generation,"overflow":self.devices.len()>32}),
            None,
        )
    }
    fn open(&mut self, s: &Shared, id: &str) -> Value {
        if self.session.is_some() {
            return envelope(
                s,
                "failed",
                Value::Null,
                Some((
                    "DEVICE_BUSY",
                    "Close the existing session before opening another",
                )),
            );
        }
        if s.cancel.load(Ordering::Acquire) {
            return envelope(
                s,
                "cancelled",
                Value::Null,
                Some(("CANCELLED", "Open cancelled before device access")),
            );
        }
        if s.mock {
            if id != "mock-1" || self.generation == 0 {
                return envelope(
                    s,
                    "failed",
                    Value::Null,
                    Some((
                        "DEVICE_NOT_FOUND",
                        "Enumerate and select a current candidate",
                    )),
                );
            }
            self.image = json!({"status":"synthetic","profile":"main","release":crate::app::ak820_beta1_profiles::BETA_RELEASE,"provenance":"public metadata only; no packed image bytes verified"});
        } else {
            let Some((_, device)) = self.devices.iter().take(32).find(|(key, _)| key == id) else {
                return envelope(
                    s,
                    "failed",
                    Value::Null,
                    Some((
                        "DEVICE_NOT_FOUND",
                        "Enumerate and select a current candidate",
                    )),
                );
            };
            #[cfg(test)]
            let opened = if let Some(io) = &mut self.test_io {
                (io.open)(device).map(|(hid, owner)| (hid, Some(owner)))
            } else {
                HidDevice::open_fresh_for(device).map(|hid| (hid, None))
            };
            #[cfg(not(test))]
            let opened = HidDevice::open_fresh_for(device);
            #[cfg(test)]
            let (hid, owner) = match opened { Ok(v) => v, Err(_) => return envelope(s,"failed",Value::Null,Some(("DEVICE_BUSY","Cannot acquire the selected HID owner; inspect permissions or another configurator"))) };
            #[cfg(not(test))]
            let hid = match opened { Ok(v) => v, Err(_) => return envelope(s,"failed",Value::Null,Some(("DEVICE_BUSY","Cannot acquire the selected HID owner; inspect permissions or another configurator"))) };
            {
                let mut cache = s.cached.lock().unwrap();
                for (_, operation) in &mut cache.operations {
                    if operation["state"] == "running" {
                        operation["lease_held"] = json!(true);
                        operation["stage"] = json!("reading_code");
                    }
                }
            }
            let (image, status) = match hid.ak820_diagnostic_bootstrap(&s.cancel) {
                Ok(v) => v,
                Err(error) => {
                    return envelope(
                        s,
                        "failed",
                        Value::Null,
                        Some((
                            if s.cancel.load(Ordering::Acquire) {
                                "CANCELLED"
                            } else {
                                classify(&error)
                            },
                            "Safe diagnostic bootstrap stopped; no speculative follow-up was sent",
                        )),
                    );
                }
            };
            self.image = image_json(image);
            self.stopped = status.ensure_ready().is_err();
            self.hid = Some(hid);
            #[cfg(test)]
            {
                self.test_owner = owner;
            }
            self.generation += 1;
            let session = format!(
                "session-{}-{}",
                s.nonce,
                s.sequence.fetch_add(1, Ordering::Relaxed)
            );
            self.session = Some(session.clone());
            let data = json!({"device_session_id":session,"session_generation":self.generation,"state":if self.stopped{"stopped"}else{"diagnostic"},"image":self.image,"storage":status,"settings_ready":false,"allowed_operations":["identity","capabilities","storage","connection"]});
            s.cached.lock().unwrap().session = Some(data.clone());
            return envelope(
                s,
                if self.stopped { "failed" } else { "ok" },
                data,
                if self.stopped {
                    Some((
                        "STORAGE_FAULT",
                        "Storage is busy or faulted; session stopped",
                    ))
                } else {
                    None
                },
            );
        }
        self.generation += 1;
        let session = format!(
            "session-{}-{}",
            s.nonce,
            s.sequence.fetch_add(1, Ordering::Relaxed)
        );
        self.session = Some(session.clone());
        let data = json!({"device_session_id":session,"session_generation":self.generation,"state":"diagnostic","image":self.image,"synthetic":true,"settings_ready":false,"allowed_operations":["identity","capabilities","storage","connection"]});
        s.cached.lock().unwrap().session = Some(data.clone());
        envelope(s, "ok", data, None)
    }
    fn close(&mut self, s: &Shared, id: &str) -> Value {
        if self.session.as_deref() != Some(id) {
            return envelope(
                s,
                "failed",
                Value::Null,
                Some(("SESSION_STALE", "Unknown or closed session")),
            );
        }
        self.hid.take();
        #[cfg(test)]
        self.test_owner.take();
        self.session.take();
        self.backups.clear();
        self.stopped = false;
        self.image = Value::Null;
        let mut cache = s.cached.lock().unwrap();
        cache.session = None;
        for (_, op) in &mut cache.operations {
            op["lease_held"] = json!(false);
        }
        drop(cache);
        envelope(s, "ok", json!({"state":"closed"}), None)
    }
    fn read(&mut self, s: &Shared, id: &str, section: dto::Section) -> Value {
        if self.session.as_deref() != Some(id) {
            return envelope(
                s,
                "failed",
                Value::Null,
                Some(("SESSION_STALE", "Unknown or closed session")),
            );
        }
        if self.stopped {
            return envelope(
                s,
                "failed",
                Value::Null,
                Some((
                    "SESSION_STOPPED",
                    "Session is stopped; inspect cached status or explicitly close",
                )),
            );
        }
        let data = match section {
            dto::Section::Identity => self.image.clone(),
            dto::Section::Capabilities => {
                json!({"settings_ready":false,"reads":["identity","capabilities","storage","connection"],"writes":false,"unlock":false,"physical_behavior_validated":false})
            }
            dto::Section::Storage | dto::Section::Connection => {
                if s.mock {
                    s.trace.trace_synthetic_status();
                    let status = super::mock::status().expect("fixed synthetic status script");
                    json!({"synthetic":true,"protocol":"FD01-v1 host fixture","status":status})
                } else {
                    let status = match self.hid.as_ref().unwrap().ak820_diagnostic_status() {
                        Ok(v) => v,
                        Err(error) => {
                            self.stopped = true;
                            if let Some(v) = s.cached.lock().unwrap().session.as_mut() {
                                v["state"] = json!("stopped");
                            }
                            return envelope(
                                s,
                                "failed",
                                Value::Null,
                                Some((
                                    classify(&error),
                                    "Diagnostic status read failed; session stopped",
                                )),
                            );
                        }
                    };
                    if status.ensure_ready().is_err() {
                        self.stopped = true;
                        let mut cache = s.cached.lock().unwrap();
                        if let Some(v) = cache.session.as_mut() {
                            v["state"] = json!("stopped");
                            v["storage"] = serde_json::to_value(&status).unwrap();
                        }
                        return envelope(
                            s,
                            "failed",
                            json!({"storage":status}),
                            Some((
                                "STORAGE_FAULT",
                                "Storage is busy or faulted; session stopped",
                            )),
                        );
                    }
                    serde_json::to_value(status).expect("typed status")
                }
            }
        };
        let mut result = envelope(
            s,
            "ok",
            json!({"device_session_id":id,"session_generation":self.generation,"observation_unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(),"value":data}),
            None,
        );
        result["image"] = self.image.clone();
        result["device_session_id"] = json!(id);
        result["session_generation"] = json!(self.generation);
        result
    }
}
fn image_json(image: crate::ak820::ImageContract) -> Value {
    use crate::ak820::ImageContract;
    use sha2::{Digest, Sha256};
    let (release, catalog): (&str, &[u8]) = match image {
        ImageContract::Beta3(_) => (
            "v0.6.0-beta.3",
            include_bytes!("../../assets/ak820/beta3-capability-catalog.json"),
        ),
        ImageContract::Beta5(_) => (
            "v0.6.0-beta.5",
            include_bytes!("../../assets/ak820/beta5-capability-catalog.json"),
        ),
        _ => unreachable!("diagnostic bootstrap admits only reviewed FD01 contracts"),
    };
    let row = image.matrix_profile().expect("diagnostic image profile");
    let identity = row
        .settings_identity
        .as_ref()
        .expect("packed settings identity");
    let catalog_sha256 = format!("{:x}", Sha256::digest(catalog));
    json!({"status":"verified","release":release,"source_alias":null,"profile":row.profile.as_str(),"ihx_sha256":identity.ihx_sha256,"packed_code_sha256":identity.code_sha256,"provenance":"FD00 manifest plus complete packed CODE matched reviewed identity","catalog_sha256":catalog_sha256})
}

// Only classify local errors. Their potentially private text never leaves the process.
fn classify(error: &anyhow::Error) -> &'static str {
    let message = format!("{error:#}").to_ascii_lowercase();
    if crate::hid::is_transport_disconnect_error(error) {
        "DISCONNECTED"
    } else if message.contains("timeout")
        || message.contains("timed out")
        || message.contains("deadline")
    {
        "TIMEOUT"
    } else if message.contains("malformed")
        || message.contains("reply")
        || message.contains("format")
    {
        "MALFORMED_REPLY"
    } else if message.contains("differs") {
        "IMAGE_MISMATCH"
    } else {
        "UNKNOWN_IMAGE"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mcp_backup_mock_and_stale_generation_fail_without_capture() {
        let mut shared = fixture_shared();
        Arc::get_mut(&mut shared).unwrap().mock = true;
        let io = TestIo {
            candidates: vec![],
            open: Box::new(|_| unreachable!()),
        };
        let mut engine = fixture_engine(io);
        engine.session = Some("session".into());
        engine.generation = 7;
        assert_eq!(
            engine.backup(&shared, "session", 6)["error"]["code"],
            "SESSION_STALE"
        );
        assert_eq!(
            engine.backup(&shared, "session", 7)["error"]["code"],
            "UNSUPPORTED_FEATURE"
        );
        assert!(engine.hid.is_none());
    }
    #[test]
    fn mcp_backup_busy_owner_rejects_before_queueing() {
        let backend = Backend::new(true);
        backend.inner.busy.store(true, Ordering::Release);
        assert_eq!(
            backend.start_backup("session".into(), 1)["error"]["code"],
            "OPERATION_BUSY"
        );
        assert!(backend.inner.cached.lock().unwrap().operations.is_empty());
        backend.shutdown();
    }
    #[test]
    fn mcp_prepare_queued_invalid_session_has_unknown_lease() {
        let backend = Backend::new(true);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        backend
            .inner
            .sender
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .send(Box::new(move |_, _| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            }))
            .unwrap();
        started_rx.recv().unwrap();
        let request: dto::Prepare = serde_json::from_value(json!({"device_session_id":"invalid","expected_generation":1,"backup_artifact_id":"unknown","target_release":"v0.6.0-beta.5","target_profile":"main","settings_mode":"defaults","prerelease_opt_in":true})).unwrap();
        let accepted = backend.start_prepare(request);
        let id = accepted["data"]["operation_id"].as_str().unwrap();
        let cache = backend.inner.cached.lock().unwrap();
        assert!(cache
            .operations
            .iter()
            .find(|(key, _)| key == id)
            .unwrap()
            .1["lease_held"]
            .is_null());
        drop(cache);
        release_tx.send(()).unwrap();
        backend.shutdown();
    }
    #[test]
    fn mcp_prepare_tracked_cancel_and_previous_cancel_reset() {
        use std::time::Duration;
        let backend = Backend::new(true);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        backend
            .inner
            .sender
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .send(Box::new(move |engine, _| {
                engine.session = Some("model-session".into());
                engine.generation = 1;
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }))
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let args = || json!({"device_session_id":"model-session","expected_generation":1,"backup_artifact_id":"unknown","target_release":"v0.6.0-beta.5","target_profile":"main","settings_mode":"defaults","prerelease_opt_in":true});
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let accepted = backend.call("ak820_prepare", args()).await;
            let id = accepted["data"]["operation_id"].as_str().unwrap();
            let queued = backend
                .call(
                    "ak820_operation",
                    json!({"action":"status","operation_id":id}),
                )
                .await;
            assert!(queued["data"]["lease_held"].is_null());
            assert_eq!(backend.cancelled_request()["status"], "cancelled");
            assert_eq!(
                backend
                    .call(
                        "ak820_operation",
                        json!({"action":"cancel","operation_id":id})
                    )
                    .await["data"]["state"],
                "running"
            );
            release_tx.send(()).unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let status = backend
                        .call(
                            "ak820_operation",
                            json!({"action":"status","operation_id":id}),
                        )
                        .await;
                    if status["data"]["state"] != "running" {
                        assert_eq!(status["data"]["state"], "cancelled");
                        assert_eq!(status["data"]["result"]["error"]["code"], "CANCELLED");
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let accepted = backend.call("ak820_prepare", args()).await;
            let id = accepted["data"]["operation_id"].as_str().unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let status = backend
                        .call(
                            "ak820_operation",
                            json!({"action":"status","operation_id":id}),
                        )
                        .await;
                    if status["data"]["state"] != "running" {
                        assert_eq!(status["data"]["state"], "failed");
                        assert_eq!(status["data"]["result"]["error"]["code"], "BACKUP_INVALID");
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        });
        backend.shutdown();
    }
    #[test]
    #[ignore = "requires AK820_PRIVATE_ROM_MODEL, a qualified offline full-ROM model"]
    fn mcp_prepare_two_hundred_offline_builder_pairs() {
        use crate::ak820_release::{InstallerRelease, ReleaseAdmission};
        let model = std::fs::read(
            std::env::var_os("AK820_PRIVATE_ROM_MODEL").expect("private model required"),
        )
        .unwrap();
        assert_eq!(
            crate::ak820_settings_backup::sha256(&model),
            "abdb30dedac76e3221024cb901e92e36c2d2ebfffd8985b6685486fc0c15a260"
        );
        let profiles = &crate::app::ak820_beta1_profiles::beta5_firmware_matrix().profiles;
        let mut count = 0;
        for release in [InstallerRelease::Beta4_1, InstallerRelease::Beta5] {
            for source in profiles {
                let source_admission =
                    ReleaseAdmission::selected(release, source.profile, true).unwrap();
                let defaults =
                    crate::ak820_release::bundled_defaults_for_image(source_admission.image())
                        .unwrap();
                let payload = crate::ak820_release::bundled_payload_from_project(
                    &source_admission,
                    &model,
                    Some(&defaults),
                )
                .unwrap();
                let mut rom = model.clone();
                rom[crate::ak820_image_pack::APP_START..crate::ak820_image_pack::APP_END]
                    .copy_from_slice(&payload);
                assert_eq!(
                    crate::ak820_installer::classify_snapshot(&rom).unwrap(),
                    crate::ak820_installer::SnapshotKind::Project(source_admission.image())
                );
                for target in profiles {
                    let admission =
                        ReleaseAdmission::selected(InstallerRelease::Beta5, target.profile, true)
                            .unwrap();
                    let (prepared, conversion, _) = prepare_image(
                        &rom,
                        target.profile,
                        dto::SettingsMode::Defaults,
                        &admission,
                    )
                    .unwrap();
                    assert_eq!(conversion, "defaults");
                    assert_eq!(
                        prepared.summary().source_sha256,
                        crate::ak820_settings_backup::sha256(&rom)
                    );
                    count += 1;
                }
            }
        }
        assert_eq!(count, 200);
        let target = profiles[0].profile;
        assert!(ReleaseAdmission::selected(InstallerRelease::Beta5, target, false).is_err());
        let admission = ReleaseAdmission::selected(InstallerRelease::Beta5, target, true).unwrap();
        let unknown = &model[..1];
        assert!(prepare_image(unknown, target, dto::SettingsMode::Defaults, &admission).is_err());
        let downgrade =
            ReleaseAdmission::selected(InstallerRelease::Beta4_1, target, true).unwrap();
        assert!(prepare_image(&model, target, dto::SettingsMode::Defaults, &downgrade).is_err());
    }
    #[test]
    #[ignore = "requires private trusted ROM in AK820_C03_FULL_ROM_PATH"]
    fn mcp_prepare_tracked_private_publication_is_retrievable() {
        use crate::app::ak820_beta1_profiles::ProfileId;
        use std::time::Duration;
        let rom = std::fs::read(
            std::env::var_os("AK820_C03_FULL_ROM_PATH").expect("private fixture required"),
        )
        .unwrap();
        let capture =
            crate::ak820_backup_capture::CapturedBackup::from_rom(ProfileId::Main, rom, true)
                .unwrap();
        let (payload, archive) =
            crate::ak820_backup_files::encode_private_capture(&capture).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let store = super::super::artifacts::platform::Store::test_open_in(temp.path()).unwrap();
        let source = super::super::artifacts::Source {
            code_sha256: &capture.identity().code_sha256,
            release: "v0.6.0-beta.5",
            qs_version: 3,
            complete: true,
            plan_expiry_utc: None,
        };
        let (backup_id, manifest) = store
            .put(super::super::artifacts::Kind::Backup, &payload, source)
            .unwrap();
        drop(store);
        let (sender, receiver) = mpsc::sync_channel::<Job>(1);
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let mut shared = fixture_shared();
        let setup = Arc::get_mut(&mut shared).unwrap();
        setup.test_store_root = Some(temp.path().to_owned());
        setup.test_prepare_publication = Some((started_tx, Mutex::new(release_rx)));
        *setup.sender.lock().unwrap() = Some(sender);
        let mut engine = fixture_engine(TestIo {
            candidates: vec![],
            open: Box::new(|_| unreachable!()),
        });
        engine.session = Some("session".into());
        engine.generation = 7;
        engine.backups.push(BackupBinding {
            artifact_id: backup_id.clone(),
            session: "session".into(),
            generation: 7,
            image: capture.image(),
            payload_sha256: manifest.payload_sha256,
            manifest_sha256: archive.manifest_sha256,
            captured_utc: chrono::Utc::now().to_rfc3339(),
        });
        let worker_shared = shared.clone();
        *shared.worker.lock().unwrap() = Some(std::thread::spawn(move || {
            while let Ok(job) = receiver.recv() {
                job(&mut engine, &worker_shared);
            }
        }));
        let backend = Backend { inner: shared };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let accepted = backend.call("ak820_prepare", json!({"device_session_id":"session","expected_generation":7,"backup_artifact_id":backup_id,"target_release":"v0.6.0-beta.5","target_profile":"main","settings_mode":"defaults","prerelease_opt_in":true})).await;
            let id = accepted["data"]["operation_id"].as_str().unwrap();
            started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            let publishing = backend.call("ak820_operation", json!({"action":"cancel","operation_id":id})).await;
            assert_eq!(publishing["data"]["stage"], "publishing");
            assert_eq!(publishing["data"]["cancellable_now"], false);
            assert!(!backend.inner.cancel.load(Ordering::Acquire));
            release_tx.send(()).unwrap();
            let terminal = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let status = backend.call("ak820_operation", json!({"action":"status","operation_id":id})).await;
                    if status["data"]["state"] != "running" { break status; }
                    tokio::task::yield_now().await;
                }
            }).await.unwrap();
            assert_eq!(terminal["data"]["state"], "completed");
            let data = &terminal["data"]["result"]["data"];
            assert_eq!(data["settings_writes_authorized"], false);
            let id = data["artifact_id"].as_str().unwrap();
            let store = super::super::artifacts::platform::Store::test_open_in(temp.path()).unwrap();
            let (manifest, bytes) = store.read(id).unwrap();
            assert_eq!(manifest.kind, super::super::artifacts::Kind::Plan);
            let plan = crate::ak820_settings_backup::parse_json(&bytes).unwrap();
            assert_eq!(plan["digest"], data["plan_sha256"]);
            assert_eq!(plan["plan"]["source"]["full_rom_sha256"], crate::ak820_settings_backup::sha256(&capture.files()["before-full-rom.bin"]));
        });
        backend.shutdown();
    }
    #[test]
    #[ignore = "requires private trusted ROM in AK820_C03_FULL_ROM_PATH"]
    fn mcp_prepare_native_private_fixture_parity_and_stale() {
        use crate::ak820_install_executor::{
            InstallRequest, InstallSettingsChoice, PreparedInstallation,
        };
        use crate::app::ak820_beta1_profiles::ProfileId;
        let rom = std::fs::read(
            std::env::var_os("AK820_C03_FULL_ROM_PATH").expect("private fixture required"),
        )
        .unwrap();
        let capture = crate::ak820_backup_capture::CapturedBackup::from_rom(
            ProfileId::Main,
            rom.clone(),
            true,
        )
        .unwrap();
        let (payload, archive) =
            crate::ak820_backup_files::encode_private_capture(&capture).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let mut shared = fixture_shared();
        Arc::get_mut(&mut shared).unwrap().test_store_root = Some(temp.path().to_owned());
        let store = super::super::artifacts::platform::Store::test_open_in(temp.path()).unwrap();
        let source = super::super::artifacts::Source {
            code_sha256: &capture.identity().code_sha256,
            release: "v0.6.0-beta.5",
            qs_version: 3,
            complete: true,
            plan_expiry_utc: None,
        };
        let (id, manifest) = store
            .put(super::super::artifacts::Kind::Backup, &payload, source)
            .unwrap();
        drop(store);
        let mut engine = fixture_engine(TestIo {
            candidates: vec![],
            open: Box::new(|_| unreachable!()),
        });
        engine.session = Some("session".into());
        engine.generation = 7;
        engine.backups.push(BackupBinding {
            artifact_id: id.clone(),
            session: "session".into(),
            generation: 7,
            image: capture.image(),
            payload_sha256: manifest.payload_sha256,
            manifest_sha256: archive.manifest_sha256,
            captured_utc: chrono::Utc::now().to_rfc3339(),
        });
        let args = |generation| dto::Prepare {
            device_session_id: "session".into(),
            expected_generation: generation,
            backup_artifact_id: id.clone(),
            target_release: dto::TargetRelease::Beta5,
            target_profile: dto::TargetProfile::Main,
            settings_mode: dto::SettingsMode::Defaults,
            prerelease_opt_in: true,
        };
        assert_eq!(
            engine.prepare(&shared, args(8))["error"]["code"],
            "SESSION_STALE"
        );
        shared.cancel.store(true, Ordering::Release);
        assert_eq!(
            engine.prepare(&shared, args(7))["error"]["code"],
            "CANCELLED"
        );
        shared.cancel.store(false, Ordering::Release);
        let mut no_opt_in = args(7);
        no_opt_in.prerelease_opt_in = false;
        assert_eq!(
            engine.prepare(&shared, no_opt_in)["error"]["code"],
            "ADMISSION_DENIED"
        );
        Arc::get_mut(&mut shared).unwrap().test_storage_fault = true;
        assert_eq!(
            engine.prepare(&shared, args(7))["error"]["code"],
            "STORAGE_ERROR"
        );
        Arc::get_mut(&mut shared).unwrap().test_storage_fault = false;
        let original_hash = engine.backups[0].payload_sha256.clone();
        engine.backups[0].payload_sha256 = "0".repeat(64);
        assert_eq!(
            engine.prepare(&shared, args(7))["error"]["code"],
            "BACKUP_INVALID"
        );
        engine.backups[0].payload_sha256 = original_hash;
        Arc::get_mut(&mut shared).unwrap().test_observation = Some(Err("fixture fault".into()));
        assert_eq!(
            engine.prepare(&shared, args(7))["error"]["code"],
            "OBSERVATION_ERROR"
        );
        let observation: crate::ak820_release::ReleaseObservation = serde_json::from_value(json!({"release":"other", "catalog_sha256":"other", "published":false, "checked_at":null, "blocked":false, "reason":null, "blocked_pins":{"v0.6.0-beta.5":"9a033d944db3282120df5b68c0c0c2a3c3a370d103e9482b95c66d673a43882d"}})).unwrap();
        Arc::get_mut(&mut shared).unwrap().test_observation = Some(Ok(Some(observation)));
        assert_eq!(
            engine.prepare(&shared, args(7))["error"]["code"],
            "ADMISSION_DENIED"
        );
        let direct: crate::ak820_release::ReleaseObservation = serde_json::from_value(json!({"release":"v0.6.0-beta.5", "catalog_sha256":"9a033d944db3282120df5b68c0c0c2a3c3a370d103e9482b95c66d673a43882d", "published":true, "checked_at":null, "blocked":true, "reason":null})).unwrap();
        Arc::get_mut(&mut shared).unwrap().test_observation = Some(Ok(Some(direct)));
        assert_eq!(
            engine.prepare(&shared, args(7))["error"]["code"],
            "ADMISSION_DENIED"
        );
        Arc::get_mut(&mut shared).unwrap().test_observation = None;
        let result = engine.prepare(&shared, args(7));
        assert_eq!(
            result["status"], "ok",
            "prepare error: {} {}",
            result["error"]["code"], result["error"]["message"]
        );
        let store = super::super::artifacts::platform::Store::test_open_in(temp.path()).unwrap();
        let (_, encoded) = store
            .read(result["data"]["artifact_id"].as_str().unwrap())
            .unwrap();
        let value = crate::ak820_settings_backup::parse_json(&encoded).unwrap();
        let release = crate::ak820_release::InstallerRelease::Beta5;
        let admission =
            crate::ak820_release::ReleaseAdmission::selected(release, ProfileId::Main, true)
                .unwrap();
        let native = PreparedInstallation::prepare(
            InstallRequest::BundledFirmware {
                profile: ProfileId::Main,
                image: admission.image(),
                recovery: None,
                settings: InstallSettingsChoice::Defaults,
            },
            rom.clone(),
            Some(&admission),
            None,
        )
        .unwrap();
        assert_eq!(
            value["plan"]["payload_sha256"],
            native.summary().payload_sha256
        );
        assert_eq!(
            value["plan"]["expected_full_rom_sha256"],
            native.summary().expected_sha256
        );
        assert_eq!(value["plan"]["settings_writes_authorized"], false);
        drop(store);
        for profile in [
            dto::TargetProfile::Main,
            dto::TargetProfile::Game,
            dto::TargetProfile::Mmo,
            dto::TargetProfile::Typing,
            dto::TargetProfile::Code,
            dto::TargetProfile::Pointer,
            dto::TargetProfile::Onehand,
            dto::TargetProfile::Creative,
            dto::TargetProfile::Shortcuts,
            dto::TargetProfile::Multios,
        ] {
            for mode in [dto::SettingsMode::Defaults, dto::SettingsMode::Preserve] {
                let mut request = args(7);
                request.target_profile = profile;
                request.settings_mode = mode;
                let target = profile.native();
                let admission =
                    crate::ak820_release::ReleaseAdmission::selected(release, target, true)
                        .unwrap();
                let settings = match mode {
                    dto::SettingsMode::Defaults => Some(InstallSettingsChoice::Defaults),
                    dto::SettingsMode::Preserve => crate::ak820_install_executor::preserved_nvm(
                        crate::ak820_installer::classify_snapshot(&rom).unwrap(),
                        &rom,
                        target,
                        admission.image(),
                    )
                    .ok()
                    .map(|nvm| InstallSettingsChoice::PreservedInPayload {
                        nvm_sha256: crate::ak820_settings_backup::sha256(&nvm),
                    }),
                };
                let native = settings.and_then(|settings| {
                    PreparedInstallation::prepare(
                        InstallRequest::BundledFirmware {
                            profile: target,
                            image: admission.image(),
                            recovery: None,
                            settings,
                        },
                        rom.clone(),
                        Some(&admission),
                        None,
                    )
                    .ok()
                });
                let result = engine.prepare(&shared, request);
                if let Some(native) = native {
                    assert_eq!(
                        result["status"], "ok",
                        "target {:?}, mode {:?}: {} {}",
                        profile, mode, result["error"]["code"], result["error"]["message"]
                    );
                    let store = super::super::artifacts::platform::Store::test_open_in(temp.path())
                        .unwrap();
                    let (_, encoded) = store
                        .read(result["data"]["artifact_id"].as_str().unwrap())
                        .unwrap();
                    let plan = crate::ak820_settings_backup::parse_json(&encoded).unwrap();
                    assert_eq!(
                        plan["plan"]["payload_sha256"],
                        native.summary().payload_sha256
                    );
                    assert_eq!(
                        plan["plan"]["expected_full_rom_sha256"],
                        native.summary().expected_sha256
                    );
                } else {
                    assert_eq!(result["error"]["code"], "ADMISSION_DENIED");
                }
            }
        }
        let store = super::super::artifacts::platform::Store::test_open_in(temp.path()).unwrap();
        let before = store.inventory().unwrap().len();
        for _ in before..100 {
            let expiry = (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339();
            let source = super::super::artifacts::Source {
                code_sha256: &capture.identity().code_sha256,
                release: "v0.6.0-beta.5",
                qs_version: 3,
                complete: true,
                plan_expiry_utc: Some(&expiry),
            };
            store
                .put(super::super::artifacts::Kind::Plan, b"{}", source)
                .unwrap();
        }
        assert_eq!(store.inventory().unwrap().len(), 100);
        drop(store);
        assert_eq!(
            engine.prepare(&shared, args(7))["error"]["code"],
            "STORAGE_ERROR"
        );
        let store = super::super::artifacts::platform::Store::test_open_in(temp.path()).unwrap();
        assert_eq!(store.inventory().unwrap().len(), 100);
    }
    #[test]
    fn mcp_backup_async_cancel_before_dispatch_and_publication_boundary() {
        use std::time::Duration;
        let backend = Backend::new(true);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        backend
            .inner
            .sender
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .send(Box::new(move |engine, _| {
                engine.session = Some("model-session".into());
                engine.generation = 1;
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }))
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let accepted = backend.call("ak820_backup", json!({"device_session_id":"model-session","expected_generation":1})).await;
            let op = accepted["data"]["operation_id"].as_str().unwrap();
            assert_eq!(backend.call("ak820_operation", json!({"action":"cancel","operation_id":op})).await["data"]["state"], "running");
            release_tx.send(()).unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let status = backend.call("ak820_operation", json!({"action":"status","operation_id":op})).await;
                    if status["data"]["state"] != "running" {
                        assert_eq!(status["data"]["state"], "cancelled");
                        assert_eq!(status["data"]["result"]["error"]["code"], "CANCELLED");
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            }).await.unwrap();
            let prepare = backend.call("ak820_prepare", json!({"device_session_id":"model-session","expected_generation":1,"backup_artifact_id":"unknown","target_release":"v0.6.0-beta.5","target_profile":"main","settings_mode":"defaults","prerelease_opt_in":true})).await;
            let prepare_id = prepare["data"]["operation_id"].as_str().unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let status = backend.call("ak820_operation", json!({"action":"status","operation_id":prepare_id})).await;
                    if status["data"]["state"] != "running" {
                        assert_eq!(status["data"]["result"]["error"]["code"], "BACKUP_INVALID");
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            }).await.unwrap();
            backend.inner.cached.lock().unwrap().operations.push(("publishing".into(), json!({"operation_id":"publishing","state":"running","stage":"publishing","cancellable_now":false})));
            let status = backend.call("ak820_operation", json!({"action":"cancel","operation_id":"publishing"})).await;
            assert_eq!(status["data"]["cancellable_now"], false);
            assert!(!backend.inner.cancel.load(Ordering::Acquire));
        });
        backend.shutdown();
    }
    #[test]
    #[ignore = "requires private trusted ROM in AK820_C03_FULL_ROM_PATH"]
    fn mcp_backup_engine_exact_fixture_publication_and_fail_stop() {
        use crate::app::ak820_beta1_profiles::ProfileId;
        use base64::Engine as _;
        let rom = std::fs::read(
            std::env::var_os("AK820_C03_FULL_ROM_PATH").expect("private ROM path required"),
        )
        .unwrap();
        assert_eq!(rom.len(), 65536);
        let image = crate::ak820::ImageContract::Beta5(ProfileId::Main);
        // Validate the model before any scripted device or storage interaction.
        let expected = crate::ak820_backup_capture::CapturedBackup::from_rom(
            ProfileId::Main,
            rom.clone(),
            true,
        )
        .expect("trusted exact ROM required");
        assert_eq!(expected.image(), image);
        for mode in ["success", "mismatch", "disconnect", "cancel", "storage"] {
            let temp = tempfile::tempdir().unwrap();
            let mut shared = fixture_shared();
            let setup = Arc::get_mut(&mut shared).unwrap();
            setup.test_store_root = Some(temp.path().to_owned());
            setup.test_storage_fault = mode == "storage";
            let (hid, recorder) = HidDevice::test_read_only_ak820_device();
            let active = Arc::new(AtomicBool::new(false));
            let capture_active = active.clone();
            let response_shared = shared.clone();
            let fixture = rom.clone();
            recorder.respond_by(move |p| {
                if capture_active.load(Ordering::Acquire) {
                    if mode == "disconnect" {
                        return Some(Err("disconnected".into()));
                    }
                    if mode == "cancel" {
                        response_shared.cancel.store(true, Ordering::Release);
                    }
                }
                let mut r = [0; 32];
                match (p[0], p[1]) {
                    (0xfd, 0) => {
                        let at = u16::from_be_bytes([p[2], p[3]]) as usize;
                        let n = p[4] as usize;
                        r[..5].copy_from_slice(&p[..5]);
                        r[5..5 + n].copy_from_slice(&fixture[at..at + n]);
                    }
                    (0xfd, 1) => r[..4].copy_from_slice(&[0xfd, 1, 1, 0xa8]),
                    (1, 0) => r[..3].copy_from_slice(&[1, 0, 9]),
                    (0xfe, 0) => {
                        r[..4].copy_from_slice(&6u32.to_le_bytes());
                        r[4..12].copy_from_slice(&crate::ak820::VIAL_UID.to_le_bytes());
                    }
                    _ => {
                        let n = match (p[0], p[1]) {
                            (4, _) => 4,
                            (2, _) | (8, _) => 2,
                            (0x0c | 0x0d, _) => 1,
                            _ => 4,
                        };
                        r = crate::hid::hid_ak820::backup_tests::modeled_getter_for_image(
                            &p[..n],
                            &fixture[0xc700..0xef00],
                            image,
                        );
                        if mode == "mismatch" && p[..4] == [0xfe, 10, 23, 0] {
                            r[2] ^= 1;
                        }
                    }
                }
                Some(Ok(r))
            });
            let mut hid = Some(hid);
            let mut engine = fixture_engine(TestIo {
                candidates: vec![fixture_device(crate::ak820::VID, crate::ak820::PID)],
                open: Box::new(move |_| Ok((hid.take().unwrap(), Box::new(())))),
            });
            let list = engine.devices(&shared, true);
            let opened = engine.open(
                &shared,
                list["data"]["devices"][0]["device_id"].as_str().unwrap(),
            );
            assert_eq!(opened["status"], "ok");
            let session = opened["data"]["device_session_id"].as_str().unwrap();
            let generation = opened["data"]["session_generation"].as_u64().unwrap();
            active.store(true, Ordering::Release);
            let result = engine.backup(&shared, session, generation);
            if mode == "success" {
                assert_eq!(result["status"], "ok");
                let id = result["data"]["artifact_id"].as_str().unwrap();
                let store =
                    super::super::artifacts::platform::Store::test_open_in(temp.path()).unwrap();
                let (manifest, payload) = store.read(id).unwrap();
                assert_eq!(manifest.payload_sha256, result["data"]["payload_sha256"]);
                let decoded = crate::ak820_settings_backup::parse_json(&payload).unwrap();
                for (name, bytes) in expected.files() {
                    let actual = base64::engine::general_purpose::STANDARD
                        .decode(decoded["files"][name].as_str().unwrap())
                        .unwrap();
                    assert!(actual == *bytes);
                }
                assert!(crate::ak820_backup_files::decode_private_capture(&payload, image).is_ok());
            } else {
                let expected_code = match mode {
                    "mismatch" => "IMAGE_MISMATCH",
                    "disconnect" => "DISCONNECTED",
                    "cancel" => "CANCELLED",
                    "storage" => "STORAGE_ERROR",
                    _ => unreachable!(),
                };
                assert_eq!(result["error"]["code"], expected_code);
                assert!(result["data"].is_null());
                if let Ok(store) =
                    super::super::artifacts::platform::Store::test_open_in(temp.path())
                {
                    assert!(store.inventory().unwrap().is_empty());
                }
            }
        }
    }
    struct FixtureOwner(Arc<AtomicBool>);
    impl Drop for FixtureOwner {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }
    fn fixture_shared() -> Arc<Shared> {
        Arc::new(Shared {
            mock: false,
            trace: Default::default(),
            nonce: "fixture".into(),
            sequence: AtomicU64::new(1),
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            cached: Mutex::new(Cache::default()),
            sender: Mutex::new(None),
            worker: Mutex::new(None),
            test_store_root: None,
            test_storage_fault: false,
            test_observation: None,
            test_prepare_publication: None,
        })
    }
    fn fixture_device(vid: u16, pid: u16) -> Device {
        Device {
            name: "fixture".into(),
            vendor_id: vid,
            product_id: pid,
            manufacturer: String::new(),
            serial_number: String::new(),
            bus_type: "USB".into(),
            path: "fixture".into(),
            instance_token: "fixture".into(),
            firmware: crate::firmware::FirmwareProtocol::Vial,
        }
    }
    fn fixture_engine(io: TestIo) -> Engine {
        Engine {
            devices: vec![],
            generation: 0,
            hid: None,
            session: None,
            image: Value::Null,
            stopped: false,
            backups: Vec::new(),
            test_io: Some(io),
            test_owner: None,
        }
    }
    #[test]
    fn diagnostic_engine_filters_candidates_and_stops_on_busy_cancel_disconnect() {
        for mode in [
            "busy",
            "cancel_before",
            "cancel_packet",
            "disconnect",
            "open_failure",
        ] {
            let s = fixture_shared();
            let opened = Arc::new(AtomicU64::new(0));
            let held = Arc::new(AtomicBool::new(false));
            let (hid, recorder) = HidDevice::test_ak820_device(false);
            let shared = s.clone();
            recorder.respond_by(move |_| {
                if mode == "cancel_packet" {
                    shared.cancel.store(true, Ordering::Release);
                }
                if mode == "disconnect" {
                    return Some(Err("disconnected".into()));
                }
                Some(Ok([0; 32]))
            });
            let mut device = Some(hid);
            let opens = opened.clone();
            let owner = held.clone();
            let io = TestIo {
                candidates: vec![
                    fixture_device(0x1111, 0x2222),
                    fixture_device(crate::ak820::VID, crate::ak820::PID),
                ],
                open: Box::new(move |_| {
                    opens.fetch_add(1, Ordering::AcqRel);
                    if mode == "busy" || mode == "open_failure" {
                        anyhow::bail!("fixture owner unavailable");
                    }
                    owner.store(true, Ordering::Release);
                    Ok((
                        device.take().unwrap(),
                        Box::new(FixtureOwner(owner.clone())),
                    ))
                }),
            };
            let mut engine = fixture_engine(io);
            let list = engine.devices(&s, true);
            assert_eq!(list["data"]["devices"].as_array().unwrap().len(), 1);
            let id = list["data"]["devices"][0]["device_id"].as_str().unwrap();
            assert_eq!(opened.load(Ordering::Acquire), 0);
            if mode == "cancel_before" {
                s.cancel.store(true, Ordering::Release);
            }
            let result = engine.open(&s, id);
            assert_eq!(
                result["error"]["code"],
                match mode {
                    "cancel_before" | "cancel_packet" => "CANCELLED",
                    "disconnect" => "DISCONNECTED",
                    _ => "DEVICE_BUSY",
                }
            );
            assert_eq!(
                opened.load(Ordering::Acquire),
                if mode == "cancel_before" { 0 } else { 1 }
            );
            assert!(!held.load(Ordering::Acquire));
            let count = recorder.requests().len();
            assert_eq!(
                count,
                if mode == "cancel_packet" || mode == "disconnect" {
                    1
                } else {
                    0
                }
            );
            assert!(engine.session.is_none());
            assert_eq!(recorder.requests().len(), count);
        }
    }
    #[test]
    #[ignore = "requires private B01 permanent payload in AK820_C01_PERMANENT_PATH"]
    fn diagnostic_engine_exact_beta5_owner_close_and_disconnect() {
        let path =
            std::env::var_os("AK820_C01_PERMANENT_PATH").expect("private payload path required");
        let payload = std::fs::read(path).expect("cannot read private payload");
        assert_eq!(payload.len(), 0xc7c0);
        for disconnect in [false, true] {
            let s = fixture_shared();
            let held = Arc::new(AtomicBool::new(false));
            let fail = Arc::new(AtomicBool::new(false));
            let (hid, recorder) = HidDevice::test_ak820_device(false);
            let mut rom = vec![0u8; 65536];
            rom[0x2800..0xefc0].copy_from_slice(&payload);
            let fail_packet = fail.clone();
            recorder.respond_by(move |p| {
                if fail_packet.load(Ordering::Acquire) {
                    return Some(Err("disconnected".into()));
                }
                let mut reply = [0; 32];
                match (p[0], p[1]) {
                    (0xfd, 0) => {
                        let address = usize::from(u16::from_be_bytes([p[2], p[3]]));
                        let n = usize::from(p[4]);
                        reply[..5].copy_from_slice(&p[..5]);
                        reply[5..5 + n].copy_from_slice(&rom[address..address + n]);
                    }
                    (0xfd, 1) => reply[..4].copy_from_slice(&[0xfd, 1, 1, 0xa8]),
                    (1, 0) => {
                        reply[0] = 1;
                        reply[2] = 9;
                    }
                    (0xfe, 0) => {
                        reply[..4].copy_from_slice(&6u32.to_le_bytes());
                        reply[4..12].copy_from_slice(&crate::ak820::VIAL_UID.to_le_bytes());
                    }
                    _ => panic!("unexpected packet"),
                }
                Some(Ok(reply))
            });
            let mut hid = Some(hid);
            let owner = held.clone();
            let mut engine = fixture_engine(TestIo {
                candidates: vec![fixture_device(crate::ak820::VID, crate::ak820::PID)],
                open: Box::new(move |_| {
                    owner.store(true, Ordering::Release);
                    Ok((hid.take().unwrap(), Box::new(FixtureOwner(owner.clone()))))
                }),
            });
            let list = engine.devices(&s, true);
            let id = list["data"]["devices"][0]["device_id"].as_str().unwrap();
            let opened = engine.open(&s, id);
            assert_eq!(opened["status"], "ok");
            assert!(held.load(Ordering::Acquire));
            let session = opened["data"]["device_session_id"].as_str().unwrap();
            if disconnect {
                fail.store(true, Ordering::Release);
                assert_eq!(
                    engine.read(&s, session, dto::Section::Storage)["error"]["code"],
                    "DISCONNECTED"
                );
                let count = recorder.requests().len();
                assert_eq!(
                    engine.read(&s, session, dto::Section::Storage)["error"]["code"],
                    "SESSION_STOPPED"
                );
                assert_eq!(recorder.requests().len(), count);
            }
            let started = std::time::Instant::now();
            assert_eq!(engine.close(&s, session)["status"], "ok");
            assert!(started.elapsed() < std::time::Duration::from_secs(1));
            assert!(!held.load(Ordering::Acquire));
        }
    }
    #[test]
    #[ignore = "requires private B01 permanent payload in AK820_C01_PERMANENT_PATH"]
    fn diagnostic_engine_real_helper_ambiguity_and_release() {
        std::env::var_os("AK820_C01_PERMANENT_PATH").expect("private payload path required");
        let _serial = HidDevice::test_helper_serial();
        let s = fixture_shared();
        let other = fixture_shared();
        let mut first = fixture_device(crate::ak820::VID, crate::ak820::PID);
        first.serial_number = "C01-first".into();
        let mut ambiguous = first.clone();
        ambiguous.serial_number.clear();
        ambiguous.path = "fixture-other-path".into();
        ambiguous.instance_token = "fixture-other-instance".into();
        let make = |device| {
            fixture_engine(TestIo {
                candidates: vec![device],
                open: Box::new(|device| {
                    Ok((HidDevice::test_ak820_real_helper(device)?, Box::new(())))
                }),
            })
        };
        let mut a = make(first);
        let mut b = make(ambiguous);
        let list = a.devices(&s, true);
        let id = list["data"]["devices"][0]["device_id"].as_str().unwrap();
        let opened = a.open(&s, id);
        assert_eq!(opened["status"], "ok", "error: {}", opened["error"]["code"]);
        let list = b.devices(&other, true);
        let id = list["data"]["devices"][0]["device_id"].as_str().unwrap();
        assert_eq!(b.open(&other, id)["error"]["code"], "DEVICE_BUSY");
        let session = opened["data"]["device_session_id"].as_str().unwrap();
        let started = std::time::Instant::now();
        assert_eq!(a.close(&s, session)["status"], "ok");
        HidDevice::test_helper_wait_released();
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        let opened = b.open(&other, id);
        assert_eq!(opened["status"], "ok");
        let session = opened["data"]["device_session_id"].as_str().unwrap();
        assert_eq!(b.close(&other, session)["status"], "ok");
        HidDevice::test_helper_wait_released();
    }
    #[test]
    fn diagnostic_image_metadata_uses_beta5_packed_identity_and_catalog() {
        use crate::ak820::ImageContract;
        use crate::app::ak820_beta1_profiles::ProfileId;
        use sha2::{Digest, Sha256};
        let image = ImageContract::Beta5(ProfileId::Main);
        let row = image.matrix_profile().unwrap();
        let identity = row.settings_identity.as_ref().unwrap();
        let data = image_json(image);
        assert_eq!(data["packed_code_sha256"], identity.code_sha256);
        assert_eq!(
            data["packed_code_sha256"],
            "cd11665942abb5dc01b3bdce0c73b1520c6e106a9cb8d754b891b12171e4f784"
        );
        assert_eq!(data["ihx_sha256"], identity.ihx_sha256);
        assert_eq!(data["release"], "v0.6.0-beta.5");
        assert!(data["source_alias"].is_null());
        assert_eq!(
            data["catalog_sha256"],
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../assets/ak820/beta5-capability-catalog.json"
                ))
            )
        );
    }
    #[test]
    fn cancelled_metadata_request_does_not_cancel_worker_and_mock_traces_are_independent() {
        let first = Backend::new(true);
        let second = Backend::new(true);
        let result = first.cancelled_request();
        assert_eq!(result["data"]["worker_cancel_requested"], false);
        assert!(!first.inner.cancel.load(Ordering::Acquire));
        first
            .inner
            .trace
            .trace_start("private-trace".into(), 512, 60)
            .unwrap();
        assert!(second
            .inner
            .trace
            .trace_status("private-trace", false)
            .is_none());
        first.shutdown();
        second.shutdown();
    }
    #[test]
    fn queued_mock_open_cancels_before_access_and_releases_owner() {
        use std::time::Duration;
        let backend = Backend::new(true);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        backend
            .inner
            .sender
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .send(Box::new(move |_, _| {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }))
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let accepted = backend
                .call(
                    "ak820_session",
                    json!({"action":"open","device_id":"mock-1","load":"diagnostics"}),
                )
                .await;
            let op = accepted["data"]["operation_id"].as_str().unwrap();
            let running = backend
                .call(
                    "ak820_operation",
                    json!({"action":"cancel","operation_id":op}),
                )
                .await;
            assert_eq!(running["data"]["state"], "running");
            release_tx.send(()).unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let result = backend
                        .call(
                            "ak820_operation",
                            json!({"action":"status","operation_id":op}),
                        )
                        .await;
                    if result["data"]["state"] != "running" {
                        assert_eq!(result["data"]["state"], "cancelled");
                        assert_eq!(result["data"]["lease_held"], false);
                        assert_eq!(result["data"]["result"]["error"]["code"], "CANCELLED");
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        });
        backend.shutdown();
    }
    #[test]
    fn mock_selection_session_and_stale_ids_are_process_local() {
        let backend = Backend::new(true);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                let unknown = backend
                    .call(
                        "ak820_session",
                        json!({"action":"open","device_id":"arbitrary-path","load":"diagnostics"}),
                    )
                    .await;
                let op = unknown["data"]["operation_id"].as_str().unwrap().to_owned();
                loop {
                    let result = backend
                        .call(
                            "ak820_operation",
                            json!({"action":"status","operation_id":op}),
                        )
                        .await;
                    if result["data"]["state"] == "failed" {
                        assert_eq!(
                            result["data"]["result"]["error"]["code"],
                            "DEVICE_NOT_FOUND"
                        );
                        break;
                    }
                    tokio::task::yield_now().await;
                }
                let devices = backend.call("ak820_devices", json!({})).await;
                assert_eq!(devices["backend"], "mock");
                assert_eq!(devices["evidence"], "host_model");
                let accepted = backend
                    .call(
                        "ak820_session",
                        json!({"action":"open","device_id":"mock-1","load":"diagnostics"}),
                    )
                    .await;
                let op = accepted["data"]["operation_id"].as_str().unwrap();
                let session = loop {
                    let result = backend
                        .call(
                            "ak820_operation",
                            json!({"action":"status","operation_id":op}),
                        )
                        .await;
                    if result["data"]["state"] == "completed" {
                        assert_eq!(
                            result["data"]["result"]["data"]["image"]["status"],
                            "synthetic"
                        );
                        break result["data"]["result"]["data"]["device_session_id"]
                            .as_str()
                            .unwrap()
                            .to_owned();
                    }
                    tokio::task::yield_now().await;
                };
                let read = backend
                    .call(
                        "ak820_read",
                        json!({"device_session_id":session,"section":"storage"}),
                    )
                    .await;
                assert_eq!(read["verification"]["setters_sent"], 0);
                assert_eq!(read["verification"]["physical_behavior_validated"], false);
                backend
                    .call(
                        "ak820_session",
                        json!({"action":"close","device_session_id":session}),
                    )
                    .await;
                let stale = backend
                    .call(
                        "ak820_read",
                        json!({"device_session_id":session,"section":"identity"}),
                    )
                    .await;
                assert_eq!(stale["error"]["code"], "SESSION_STALE");
                let closed_operation = backend
                    .call(
                        "ak820_operation",
                        json!({"action":"status","operation_id":op}),
                    )
                    .await;
                assert_eq!(closed_operation["data"]["lease_held"], false);
            })
            .await
            .expect("bounded mock workflow");
        });
        backend.shutdown();
    }
}
