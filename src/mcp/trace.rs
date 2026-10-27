//! Bounded metadata only: no report bytes, addresses, paths, serials or settings.
use serde_json::{json, Value};
#[derive(Default)]
pub(crate) struct Recorder {
    capture: Option<Capture>,
}
struct Capture {
    id: String,
    started: std::time::Instant,
    max_bytes: usize,
    max_seconds: u16,
    bytes: usize,
    dropped: u64,
    state: &'static str,
    records: Vec<Value>,
}
impl Recorder {
    pub(crate) fn start(
        &mut self,
        id: String,
        max_bytes: usize,
        max_seconds: u16,
    ) -> Result<(), ()> {
        if let Some(c) = self.capture.as_mut() {
            if c.state == "recording" && c.started.elapsed().as_secs() >= u64::from(c.max_seconds) {
                c.state = "time_limit";
            }
        }
        if self
            .capture
            .as_ref()
            .is_some_and(|c| c.state == "recording")
        {
            return Err(());
        }
        self.capture = Some(Capture {
            id,
            started: std::time::Instant::now(),
            max_bytes,
            max_seconds,
            bytes: 0,
            dropped: 0,
            state: "recording",
            records: vec![],
        });
        Ok(())
    }
    pub(crate) fn record(&mut self, data: &[u8], elapsed: std::time::Duration, success: bool) {
        let Some(c) = self.capture.as_mut() else {
            return;
        };
        if c.state != "recording" {
            return;
        }
        if c.started.elapsed().as_secs() >= u64::from(c.max_seconds) {
            c.state = "time_limit";
            c.dropped += 1;
            return;
        }
        let family = match data.first() {
            Some(0xfd) => match data.get(1) {
                Some(0) => "code_or_nvm_read",
                Some(1) => "diagnostic_status",
                _ => "extension",
            },
            Some(0xfe) => "vial_identity_or_getter",
            _ => "via_getter",
        };
        let v = json!({"family":family,"request_bytes":data.len(),"response_bytes":if success{32}else{0},"elapsed_us":elapsed.as_micros(),"result":if success{"ok"}else{"transport_error"}});
        let bytes = serde_json::to_vec(&v).unwrap().len() + 1;
        if bytes > c.max_bytes.saturating_sub(c.bytes) {
            c.state = "byte_limit";
            c.dropped += 1;
            return;
        }
        c.bytes += bytes;
        c.records.push(v);
    }
    pub(crate) fn status(&mut self, id: &str, stop: bool) -> Option<Value> {
        let c = self.capture.as_mut().filter(|c| c.id == id)?;
        if c.state == "recording" && c.started.elapsed().as_secs() >= u64::from(c.max_seconds) {
            c.state = "time_limit";
        }
        if stop && c.state == "recording" {
            c.state = "stopped";
        }
        Some(
            json!({"trace_id":c.id,"state":c.state,"records":c.records.len(),"bytes":c.bytes,"dropped":c.dropped,"replayable":false,"page_records":128,"pages":c.records.len().div_ceil(128)}),
        )
    }
    pub(crate) fn page(&self, id: &str, page: usize) -> Option<Value> {
        let c = self.capture.as_ref().filter(|c| c.id == id)?;
        let start = page.checked_mul(128)?;
        if start > c.records.len() {
            return None;
        }
        Some(
            json!({"trace_id":id,"page":page,"replayable":false,"records":c.records.iter().skip(start).take(128).collect::<Vec<_>>()}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_capture_expires_a_quiet_timed_out_capture() {
        let mut recorder = Recorder::default();
        recorder.start("old".into(), 512, 1).unwrap();
        recorder.capture.as_mut().unwrap().started =
            std::time::Instant::now() - std::time::Duration::from_secs(2);
        recorder.start("new".into(), 512, 1).unwrap();
        assert_eq!(recorder.status("new", false).unwrap()["state"], "recording");
    }
    #[test]
    fn capture_is_bounded_metadata_and_never_replayable() {
        let mut r = Recorder::default();
        r.start("trace-test".into(), 512, 60).unwrap();
        for _ in 0..100 {
            r.record(
                &[0xfd, 0, 0xef, 0x80, 27],
                std::time::Duration::from_micros(5),
                true,
            );
        }
        let summary = r.status("trace-test", false).unwrap();
        assert_eq!(summary["state"], "byte_limit");
        assert_eq!(summary["replayable"], false);
        assert!(summary["bytes"].as_u64().unwrap() <= 512);
        let page = r.page("trace-test", 0).unwrap();
        let text = page.to_string();
        assert!(!text.contains("239"));
        assert!(!text.contains("payload"));
        assert!(text.contains("code_or_nvm_read"));
        assert!(r.page("unknown", 0).is_none());
        assert!(r.page("trace-test", usize::MAX).is_none());
    }
}
