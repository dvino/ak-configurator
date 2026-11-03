//! Canonical immutable installation preview. This module has no transport.
use crate::ak820_settings_backup::{canonical, parse_json, sha256, BackupResult};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Source {
    pub session: String,
    pub generation: u64,
    pub backup_id: String,
    pub backup_sha256: String,
    pub native_manifest_sha256: String,
    pub full_rom_sha256: String,
    pub release: String,
    pub profile: String,
    pub code_sha256: String,
    pub image_qs: u32,
    pub persisted_qs: Option<u32>,
    pub captured_utc: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Target {
    pub release: String,
    pub profile: String,
    pub code_sha256: String,
    pub ihx_sha256: String,
    pub map_sha256: String,
    pub image_qs: u32,
    pub catalog_sha256: String,
    pub matrix_sha256: String,
    pub capability_sha256: String,
    pub profiles_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    pub format: String,
    pub operation: String,
    pub source: Source,
    pub target: Target,
    pub settings_mode: String,
    pub settings_conversion: String,
    pub expected_nvm_sha256: String,
    pub payload_sha256: String,
    pub expected_full_rom_sha256: String,
    pub prerelease_opt_in: bool,
    pub observation_status: String,
    pub observation_sha256: Option<String>,
    pub observation_checked_at: Option<String>,
    pub issued_utc: String,
    pub expires_utc: String,
    pub settings_writes_authorized: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedPlan {
    plan: Plan,
    digest: String,
}

pub(super) fn encode(plan: &Plan) -> BackupResult<(Vec<u8>, String)> {
    let bytes = canonical(&serde_json::to_value(plan).map_err(|e| e.to_string())?)?;
    let digest = sha256(&bytes);
    let payload = canonical(
        &serde_json::to_value(SignedPlan {
            plan: plan.clone(),
            digest: digest.clone(),
        })
        .map_err(|e| e.to_string())?,
    )?;
    Ok((payload, digest))
}

/// Require canonical bytes, exact independently recomputed bindings and a
/// bounded validity window. A modified plan is rejected, never repaired.
pub(super) fn validate(
    payload: &[u8],
    expected: &Plan,
    now: chrono::DateTime<chrono::Utc>,
) -> BackupResult<String> {
    if payload.len() > 1024 * 1024 {
        return Err("Plan exceeds limit".into());
    }
    let value = parse_json(payload)?;
    if canonical(&value)? != payload {
        return Err("Plan is not canonical".into());
    }
    let signed: SignedPlan = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if &signed.plan != expected
        || signed.plan.format != "ak820.mcp.plan/v1"
        || signed.plan.operation != "install-preview"
        || signed.plan.settings_writes_authorized
    {
        return Err("Plan bindings changed".into());
    }
    let issued = chrono::DateTime::parse_from_rfc3339(&signed.plan.issued_utc)
        .map_err(|_| "Invalid issue time")?;
    let expires = chrono::DateTime::parse_from_rfc3339(&signed.plan.expires_utc)
        .map_err(|_| "Invalid expiry")?;
    if issued.offset().local_minus_utc() != 0
        || expires.offset().local_minus_utc() != 0
        || issued > now
        || expires <= now
        || expires <= issued
        || expires - issued > chrono::Duration::minutes(15)
    {
        return Err("Plan time window invalid".into());
    }
    let (_, digest) = encode(&signed.plan)?;
    if signed.digest != digest {
        return Err("Plan digest differs".into());
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(now: chrono::DateTime<chrono::Utc>) -> Plan {
        Plan {
            format: "ak820.mcp.plan/v1".into(),
            operation: "install-preview".into(),
            source: Source {
                session: "session".into(),
                generation: 1,
                backup_id: "backup".into(),
                backup_sha256: "a".repeat(64),
                native_manifest_sha256: "b".repeat(64),
                full_rom_sha256: "c".repeat(64),
                release: "v0.6.0-beta.5".into(),
                profile: "main".into(),
                code_sha256: "d".repeat(64),
                image_qs: 3,
                persisted_qs: None,
                captured_utc: now.to_rfc3339(),
            },
            target: Target {
                release: "v0.6.0-beta.5".into(),
                profile: "game".into(),
                code_sha256: "e".repeat(64),
                ihx_sha256: "f".repeat(64),
                map_sha256: "0".repeat(64),
                image_qs: 3,
                catalog_sha256: "1".repeat(64),
                matrix_sha256: "2".repeat(64),
                capability_sha256: "3".repeat(64),
                profiles_sha256: "7".repeat(64),
            },
            settings_mode: "preserve".into(),
            settings_conversion: "preserve".into(),
            expected_nvm_sha256: "4".repeat(64),
            payload_sha256: "5".repeat(64),
            expected_full_rom_sha256: "6".repeat(64),
            prerelease_opt_in: true,
            observation_status: "unknown".into(),
            observation_sha256: None,
            observation_checked_at: None,
            issued_utc: now.to_rfc3339(),
            expires_utc: (now + chrono::Duration::minutes(15)).to_rfc3339(),
            settings_writes_authorized: false,
        }
    }

    #[test]
    fn mcp_prepare_canonical_binding_and_expiry() {
        let now = chrono::Utc::now();
        let plan = fixture(now);
        let (encoded, digest) = encode(&plan).unwrap();
        assert_eq!(validate(&encoded, &plan, now).unwrap(), digest);
        assert!(validate(&encoded, &plan, now + chrono::Duration::minutes(15)).is_err());
        let mut changed = plan.clone();
        changed.source.generation += 1;
        assert!(validate(&encoded, &changed, now).is_err());
        changed = plan.clone();
        changed.target.catalog_sha256.push('0');
        assert!(validate(&encoded, &changed, now).is_err());
        changed = plan.clone();
        changed.source.backup_sha256.push('0');
        assert!(validate(&encoded, &changed, now).is_err());
        let mut value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        value["plan"]["target"]["profile"] = "main".into();
        assert!(validate(&canonical(&value).unwrap(), &plan, now).is_err());
        value["plan"]["unexpected"] = true.into();
        assert!(validate(&canonical(&value).unwrap(), &plan, now).is_err());
        assert!(validate(br#"{"plan":{},"plan":{},"digest":"x"}"#, &plan, now).is_err());
    }

    #[test]
    fn mcp_prepare_every_plan_field_is_bound() {
        fn mutate_leaves(
            value: &serde_json::Value,
            path: &mut Vec<String>,
            out: &mut Vec<Vec<String>>,
        ) {
            if let Some(object) = value.as_object() {
                for (key, child) in object {
                    path.push(key.clone());
                    mutate_leaves(child, path, out);
                    path.pop();
                }
            } else {
                out.push(path.clone());
            }
        }
        let now = chrono::Utc::now();
        let plan = fixture(now);
        let original = serde_json::to_value(&plan).unwrap();
        let mut paths = Vec::new();
        mutate_leaves(&original, &mut Vec::new(), &mut paths);
        for path in paths {
            let mut changed = original.clone();
            let mut leaf = &mut changed;
            for key in &path {
                leaf = &mut leaf[key];
            }
            *leaf = match &*leaf {
                serde_json::Value::String(s) => format!("{s}x").into(),
                serde_json::Value::Number(_) => 99.into(),
                serde_json::Value::Bool(b) => (!*b).into(),
                serde_json::Value::Null => "changed".into(),
                _ => unreachable!(),
            };
            let mut wrapper = serde_json::json!({"plan":changed,"digest":"0".repeat(64)});
            // Recompute the unkeyed digest: expected bindings must still reject the mutation.
            wrapper["digest"] = sha256(&canonical(&wrapper["plan"]).unwrap()).into();
            assert!(
                validate(&canonical(&wrapper).unwrap(), &plan, now).is_err(),
                "field {path:?}"
            );
        }
    }
}
