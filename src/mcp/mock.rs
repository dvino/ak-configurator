//! Public synthetic FD01 vectors. Strict scripts never fall back to a canned reply.
use std::collections::VecDeque;
pub(crate) struct Script {
    exchanges: VecDeque<(Vec<u8>, Result<Vec<u8>, &'static str>)>,
}
impl Script {
    pub(crate) fn new(
        exchanges: impl IntoIterator<Item = (Vec<u8>, Result<Vec<u8>, &'static str>)>,
    ) -> Self {
        Self {
            exchanges: exchanges.into_iter().collect(),
        }
    }
    pub(crate) fn send(&mut self, request: &[u8]) -> anyhow::Result<Vec<u8>> {
        let (expected, response) = self
            .exchanges
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("Unscripted mock request"))?;
        anyhow::ensure!(request == expected, "Mock request sequence mismatch");
        response.map_err(anyhow::Error::msg)
    }
    pub(crate) fn finish(self) -> anyhow::Result<()> {
        anyhow::ensure!(self.exchanges.is_empty(), "Unconsumed mock exchanges");
        Ok(())
    }
}
pub(crate) fn clean() -> Vec<u8> {
    let mut bytes = vec![0; 32];
    bytes[..4].copy_from_slice(&[0xfd, 1, 1, 0xa8]);
    bytes[19] = 1;
    bytes[21] = 3;
    bytes
}
pub(crate) fn status() -> anyhow::Result<crate::ak820_diagnostics::Status> {
    let mut script = Script::new([(vec![0xfd, 1], Ok(clean()))]);
    let reply = script.send(&[0xfd, 1])?;
    script.finish()?;
    crate::ak820_diagnostics::Status::decode(&reply)
}
pub(crate) fn error_checks() -> serde_json::Value {
    let mut checks = vec![];
    for (name, offset, value) in [
        ("dirty_storage", 6, 1),
        ("faulted_storage", 4, 1),
        ("batch_busy", 7, 1),
        ("unlock_busy", 18, 1),
    ] {
        let mut bytes = clean();
        bytes[offset] = value;
        let rejected = crate::ak820_diagnostics::Status::decode(&bytes)
            .unwrap()
            .ensure_ready()
            .is_err();
        checks.push(serde_json::json!({"id":name,"status":if rejected{"PASS"}else{"FAIL"}}));
    }
    for (name, bytes) in [
        ("short_reply", clean()[..31].to_vec()),
        ("long_reply", {
            let mut v = clean();
            v.push(0);
            v
        }),
        ("wrong_echo", {
            let mut v = clean();
            v[1] = 2;
            v
        }),
        ("reserved_byte", {
            let mut v = clean();
            v[31] = 1;
            v
        }),
    ] {
        checks.push(serde_json::json!({"id":name,"status":if crate::ak820_diagnostics::Status::decode(&bytes).is_err(){"PASS"}else{"FAIL"}}));
    }
    for fault in ["timeout", "disconnect", "helper_crash"] {
        let mut script = Script::new([(vec![0xfd, 1], Err(fault))]);
        let rejected = script.send(&[0xfd, 1]).is_err() && script.send(&[0xfd, 1]).is_err();
        checks.push(serde_json::json!({"id":fault,"status":if rejected{"PASS"}else{"FAIL"}}));
    }
    let mut script = Script::new([(vec![0xfd, 1], Ok(clean()))]);
    let mismatch = script.send(&[0xfd, 0]).is_err();
    checks.push(
        serde_json::json!({"id":"unscripted_request","status":if mismatch{"PASS"}else{"FAIL"}}),
    );
    serde_json::json!({"suite_id":"protocol.errors","fixture":"synthetic-fd01-v1","synthetic":true,"checks":checks,"full_acceptance":false,"exclusions":["full image/bootstrap, real transport and physical behavior"]})
}
#[cfg(test)]
mod tests {
    #[test]
    fn strict_script_rejects_extra_missing_and_mismatched_requests() {
        let mut s = super::Script::new([(vec![1], Ok(vec![2]))]);
        assert!(s.send(&[2]).is_err());
        assert!(s.send(&[1]).is_err());
        assert!(super::Script::new([(vec![1], Ok(vec![2]))])
            .finish()
            .is_err());
        assert!(super::error_checks()["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["status"] == "PASS"));
    }
}
