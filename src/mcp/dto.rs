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
#[serde(deny_unknown_fields)]
pub struct Backup {
    pub device_session_id: String,
    pub expected_generation: u64,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Prepare {
    pub device_session_id: String,
    pub expected_generation: u64,
    pub backup_artifact_id: String,
    pub target_release: TargetRelease,
    pub target_profile: TargetProfile,
    pub settings_mode: SettingsMode,
    pub prerelease_opt_in: bool,
}
#[derive(Debug, Deserialize, JsonSchema)]
pub enum TargetRelease {
    #[serde(rename = "v0.6.0-beta.5")]
    Beta5,
}
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TargetProfile {
    Main,
    Game,
    Mmo,
    Typing,
    Code,
    Pointer,
    Onehand,
    Creative,
    Shortcuts,
    Multios,
}
impl TargetProfile {
    pub(crate) fn native(self) -> crate::app::ak820_beta1_profiles::ProfileId {
        use crate::app::ak820_beta1_profiles::ProfileId as P;
        match self {
            Self::Main => P::Main,
            Self::Game => P::Game,
            Self::Mmo => P::Mmo,
            Self::Typing => P::Typing,
            Self::Code => P::Code,
            Self::Pointer => P::Pointer,
            Self::Onehand => P::Onehand,
            Self::Creative => P::Creative,
            Self::Shortcuts => P::Shortcuts,
            Self::Multios => P::Multios,
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SettingsMode {
    Preserve,
    Defaults,
}

#[cfg(test)]
#[test]
fn mcp_prepare_closed_contract_rejects_unknowns_and_legacy_targets() {
    let base = serde_json::json!({"device_session_id":"s", "expected_generation":1, "backup_artifact_id":"b", "target_release":"v0.6.0-beta.5", "target_profile":"main", "settings_mode":"preserve", "prerelease_opt_in":true});
    assert!(serde_json::from_value::<Prepare>(base.clone()).is_ok());
    for (key, value) in [
        ("target_release", serde_json::json!("v0.6.0-beta.3")),
        ("target_profile", serde_json::json!("unknown")),
        ("settings_mode", serde_json::json!("restore")),
        ("unexpected", serde_json::json!(true)),
    ] {
        let mut input = base.clone();
        input[key] = value;
        assert!(serde_json::from_value::<Prepare>(input).is_err());
    }
    let mut missing = base;
    missing.as_object_mut().unwrap().remove("prerelease_opt_in");
    assert!(serde_json::from_value::<Prepare>(missing).is_err());
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
    #[test]
    fn mcp_backup_contract_is_closed_and_requires_generation() {
        assert!(serde_json::from_value::<Backup>(
            json!({"device_session_id":"s","expected_generation":1})
        )
        .is_ok());
        assert!(serde_json::from_value::<Backup>(json!({"device_session_id":"s"})).is_err());
        assert!(serde_json::from_value::<Backup>(
            json!({"device_session_id":"s","expected_generation":1,"path":"x"})
        )
        .is_err());
    }
}
