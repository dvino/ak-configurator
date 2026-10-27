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
        envelope(&self.inner,"cancelled",json!({"response_detached":true,"worker_cancel_requested":false,"owner_released":false}),Some(("CANCELLED","Request response cancelled; worker retains ownership until completion. Use ak820_operation to cancel an asynchronous open")))
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
                dto::Session::Open { device_id, load: _ } => self.start_open(device_id),
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
                        mode: _,
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
                    if cancel && op["state"] == "running" {
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
impl Engine {
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
            let devices = match DeviceManager::scan_usb_devices() {
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
            let hid=match HidDevice::open_fresh_for(device) {Ok(h)=>h,Err(_)=>return envelope(s,"failed",Value::Null,Some(("DEVICE_BUSY","Cannot acquire the selected HID owner; inspect permissions or another configurator")))};
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
        self.session.take();
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
    let profile = image
        .matrix_profile()
        .expect("diagnostic bootstrap admits beta.3")
        .profile;
    let row = crate::app::ak820_beta1_profiles::catalog()
        .images
        .iter()
        .find(|p| p.profile == profile)
        .expect("catalog profile");
    json!({"status":"verified","release":crate::app::ak820_beta1_profiles::BETA_RELEASE,"profile":profile.as_str(),"ihx_sha256":row.build_identity.ihx_sha256,"packed_code_sha256":row.code_identity.sha256,"provenance":"FD00 manifest plus complete packed CODE matched compiled catalog","catalog_sha256":crate::app::ak820_beta1_profiles::SOURCE_CATALOG_SHA256})
}

#[cfg(test)]
mod tests {
    use super::*;
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
