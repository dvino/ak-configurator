//! Closed P0 input contracts. Unimplemented mutation actions have no schema branch.
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Devices {
    #[serde(default)]
    pub refresh: bool,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Session {
    Open {
        device_id: String,
        load: DiagnosticLoad,
    },
    Status {
        device_session_id: String,
    },
    Close {
        device_session_id: String,
    },
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLoad {
    Diagnostics,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Read {
    pub device_session_id: String,
    pub section: Section,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Identity,
    Capabilities,
    Storage,
    Connection,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Tests {
    List,
    Run {
        device_session_id: Option<String>,
        suite_id: Suite,
        #[serde(default)]
        seed: u32,
    },
}
#[derive(Debug, Deserialize, JsonSchema)]
pub enum Suite {
    #[serde(rename = "protocol.smoke")]
    Smoke,
    #[serde(rename = "protocol.errors")]
    Errors,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Trace {
    Start {
        device_session_id: String,
        mode: TraceMode,
        #[serde(default = "trace_bytes")]
        max_bytes: u32,
        #[serde(default = "trace_seconds")]
        max_seconds: u16,
    },
    Stop {
        trace_id: String,
    },
    Status {
        trace_id: String,
    },
}
fn trace_bytes() -> u32 {
    8 * 1024 * 1024
}
fn trace_seconds() -> u16 {
    60
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TraceMode {
    Metadata,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Status { operation_id: String },
    Cancel { operation_id: String },
}

#[derive(JsonSchema)]
#[allow(dead_code)]
pub struct ResultEnvelope {
    pub format: String,
    pub request_id: String,
    pub status: String,
    pub backend: String,
    pub evidence: String,
    pub image: Option<serde_json::Value>,
    pub device_session_id: Option<String>,
    pub session_generation: Option<u64>,
    pub data: serde_json::Value,
    pub verification: serde_json::Value,
    pub artifacts: Vec<serde_json::Value>,
    pub error: Option<Failure>,
}
#[derive(JsonSchema)]
#[allow(dead_code)]
pub struct Failure {
    pub code: String,
    pub message: String,
    pub stage: String,
    pub retryable: bool,
    pub recovery_required: bool,
    pub details: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn p0_contract_rejects_unimplemented_and_unknown_actions() {
        for v in [
            json!({"action":"open","device_id":"x","load":"settings"}),
            json!({"action":"open","device_id":"x","load":"diagnostics","unlock":true}),
            json!({"action":"reset","device_session_id":"x"}),
        ] {
            assert!(serde_json::from_value::<Session>(v).is_err());
        }
        assert!(serde_json::from_value::<Read>(
            json!({"device_session_id":"x","section":"macros"})
        )
        .is_err());
        assert!(serde_json::from_value::<Trace>(
            json!({"action":"start","device_session_id":"x","mode":"payload"})
        )
        .is_err());
        assert!(serde_json::from_value::<Tests>(
            json!({"action":"run","device_session_id":"x","suite_id":"protocol.smoke","seed":-1})
        )
        .is_err());
    }
}
