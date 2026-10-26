// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned public release inputs and admission. No device authorization is
//! implied: fresh per-operation confirmation belongs to the installer.
use crate::ak820_image_pack::{
    default_nvm, pack_candidate, read_symbols, validate_catalog_payload, EntryMode,
};
use crate::ak820_settings_backup::{sha256, BackupResult};
use crate::app::ak820_beta1_profiles::{self as beta, ProfileId};
use serde::{Deserialize, Serialize};

const RELEASE_CATALOG: &str = include_str!("../assets/ak820/beta2-release-catalog.json");
const RELEASE_SHA256: &str = "2ede283a4bf1acfaca237b80d221fc23ba81426ed647aeb4d62474f82e221a73";
const RELEASE_API: &str = "https://api.github.com/repos/dvino/ak820/releases/tags/v0.6.0-beta.2";
const RELEASE_PAGE: &str = "https://github.com/dvino/ak820/releases/tag/v0.6.0-beta.2";
const MANIFEST_SHA256: &str = "999266594b741d92814b454616c2c0266f3515824f294738061066376b1c6ec6";

/// Block-only observations persist separately from firmware and cannot grant
/// support to a new image. A network failure never clears a known block.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReleaseObservation {
    pub(crate) checked_at: Option<String>,
    pub(crate) blocked: bool,
    pub(crate) reason: Option<String>,
}
impl ReleaseObservation {
    pub(crate) fn apply(&mut self, newer: Self) {
        self.checked_at = newer.checked_at;
        if newer.blocked {
            self.blocked = true;
            self.reason = newer.reason;
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ReleaseAdmission {
    profile: ProfileId,
    ihx_sha256: String,
    packed_code_sha256: String,
}
impl ReleaseAdmission {
    pub(crate) fn bundled(profile: ProfileId, prerelease_opt_in: bool) -> BackupResult<Self> {
        if !prerelease_opt_in {
            return Err(
                "The bundled firmware is a prerelease; enable prerelease installation explicitly"
                    .into(),
            );
        }
        if sha256(RELEASE_CATALOG.as_bytes()) != RELEASE_SHA256 {
            return Err("Bundled release catalog differs from its reviewed digest".into());
        }
        let catalog: serde_json::Value =
            serde_json::from_str(RELEASE_CATALOG).map_err(|_| "Invalid bundled release catalog")?;
        let row = beta::matrix_profile(profile);
        let image = beta::profile(profile);
        if catalog["tag"].as_str() != Some(beta::BETA_RELEASE)
            || catalog["format"].as_str() != Some("ak820-beta-release-catalog-1")
            || !catalog["installable_profiles"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|r| r.as_str() == Some(profile.as_str())))
            || !row.installable
            || !row.release_approved
            || !image.release_approved
        {
            return Err("Selected image has no reviewed release admission".into());
        }
        let exact = row
            .settings_identity
            .as_ref()
            .ok_or("Missing packed image policy")?;
        Ok(Self {
            profile,
            ihx_sha256: exact.ihx_sha256.clone(),
            packed_code_sha256: exact.code_sha256.clone(),
        })
    }
    pub(crate) fn with_observation(self, observation: &ReleaseObservation) -> BackupResult<Self> {
        if observation.blocked {
            return Err(
                "The selected bundled release was withdrawn or changed; installation is blocked"
                    .into(),
            );
        }
        Ok(self)
    }
    pub(crate) fn validate_payload(&self, profile: ProfileId, payload: &[u8]) -> BackupResult<()> {
        let exact = beta::matrix_profile(profile)
            .settings_identity
            .as_ref()
            .ok_or("Missing packed image policy")?;
        if profile != self.profile
            || exact.ihx_sha256 != self.ihx_sha256
            || exact.code_sha256 != self.packed_code_sha256
        {
            return Err("Release admission is bound to a different image".into());
        }
        validate_catalog_payload(payload, profile)?;
        Ok(())
    }
    pub(crate) fn metadata_sha256(&self) -> &'static str {
        RELEASE_SHA256
    }
    pub(crate) fn version(&self) -> &'static str {
        beta::BETA_RELEASE
    }
    pub(crate) fn offline_notice(&self) -> &'static str {
        "Bundled release metadata; a later withdrawal may be unknown offline."
    }
}

/// Only these immutable public linked sources are bundled. OEM reference bytes
/// are supplied locally by the user and never included in the application.
pub(crate) fn bundled_sources(profile: ProfileId) -> BackupResult<(&'static str, &'static str)> {
    macro_rules! source {
        ($name:literal) => {
            (
                include_str!(concat!(
                    "../assets/ak820/firmware-beta2/",
                    $name,
                    "/application.ihx"
                )),
                include_str!(concat!(
                    "../assets/ak820/firmware-beta2/",
                    $name,
                    "/application.map"
                )),
            )
        };
    }
    let (ihx, map) = match profile {
        ProfileId::Main => source!("main"),
        ProfileId::Game => source!("game"),
        ProfileId::Mmo => source!("mmo"),
        ProfileId::Typing => source!("typing"),
        ProfileId::Code => source!("code"),
        ProfileId::Pointer => source!("pointer"),
        ProfileId::Onehand => source!("onehand"),
        ProfileId::Creative => source!("creative"),
        ProfileId::Shortcuts => source!("shortcuts"),
        ProfileId::Multios => source!("multios"),
    };
    let identity = &beta::matrix_profile(profile).identity;
    if sha256(ihx.as_bytes()) != identity.ihx_sha256
        || sha256(map.as_bytes()) != identity.map_sha256
    {
        return Err("Bundled linked firmware differs from the release".into());
    }
    Ok((ihx, map))
}
pub(crate) fn bundled_defaults(profile: ProfileId) -> BackupResult<Vec<u8>> {
    let (ihx, map) = bundled_sources(profile)?;
    default_nvm(ihx, &read_symbols(map)?)
}
/// Prepare a firmware payload offline, with either explicit settings or factory
/// defaults. The real installer subsequently binds it to a fresh device read.
pub(crate) fn bundled_payload(
    profile: ProfileId,
    recovery: &[u8],
    nvm: Option<&[u8]>,
) -> BackupResult<Vec<u8>> {
    let (ihx, map) = bundled_sources(profile)?;
    let symbols = read_symbols(map)?;
    let defaults;
    let nvm = match nvm {
        Some(nvm) => nvm,
        None => {
            defaults = default_nvm(ihx, &symbols)?;
            &defaults
        }
    };
    Ok(
        pack_candidate(ihx, &symbols, recovery, nvm, EntryMode::Permanent, profile)?
            .bytes()
            .to_vec(),
    )
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<ApiAsset>,
}
#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    size: u64,
    state: String,
    digest: Option<String>,
    browser_download_url: String,
}
fn observe_api(bytes: &[u8], checked_at: String) -> BackupResult<ReleaseObservation> {
    let release: ApiRelease =
        serde_json::from_slice(bytes).map_err(|_| "Invalid release response")?;
    if release.tag_name != beta::BETA_RELEASE || release.html_url != RELEASE_PAGE {
        return Err("Release response came from an unexpected project or tag".into());
    }
    let expected = [
        (
            "AK820-Beta2-Catalog.zip",
            4034329,
            beta::SOURCE_ARCHIVE_SHA256,
        ),
        ("release-catalog.json", 29522, RELEASE_SHA256),
        ("release-manifest.json", 29281, MANIFEST_SHA256),
    ];
    let mut changed = release.draft || !release.prerelease;
    let mut unknown_digest = false;
    for (name, size, digest) in expected {
        let matches: Vec<_> = release.assets.iter().filter(|a| a.name == name).collect();
        if matches.len() != 1 {
            changed = true;
            continue;
        }
        let asset = matches[0];
        let url = format!(
            "https://github.com/dvino/ak820/releases/download/{}/{name}",
            beta::BETA_RELEASE
        );
        changed |=
            asset.size != size || asset.state != "uploaded" || asset.browser_download_url != url;
        // A missing API digest cannot establish unchanged contents. Keep the
        // old offline status, rather than claiming a current withdrawal check.
        if let Some(reported) = asset.digest.as_deref() {
            changed |= reported != format!("sha256:{digest}");
        } else {
            unknown_digest = true;
        }
    }
    if unknown_digest && !changed {
        return Err("Release API has no asset digest; current status is unknown".into());
    }
    Ok(ReleaseObservation {
        checked_at: Some(checked_at),
        blocked: changed,
        reason: changed
            .then(|| "Bundled release is withdrawn, incomplete or its assets changed".to_owned()),
    })
}

/// Explicit user-triggered metadata request. Never sends ROM, settings, device
/// identifiers or credentials; never downloads/runs firmware or opens HID.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn check_bundled_release() -> BackupResult<ReleaseObservation> {
    use std::io::Read;
    let checked_at = chrono::Utc::now().to_rfc3339();
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .redirects(0)
        .build();
    let response = match agent
        .get(RELEASE_API)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "AK-Configurator-release-check")
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(404 | 410, _)) => {
            return Ok(ReleaseObservation {
                checked_at: Some(checked_at),
                blocked: true,
                reason: Some("Bundled release is no longer published".into()),
            })
        }
        Err(_) => return Err("Release check is unavailable; last known status is retained".into()),
    };
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(512 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Release response read failed")?;
    if bytes.len() > 512 * 1024 {
        return Err("Release response exceeds the allowed size".into());
    }
    observe_api(&bytes, checked_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_ten_public_sources_and_default_settings_are_validated() {
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/ak820-beta2-defaults.json"))
                .unwrap();
        for image in beta::profiles() {
            let profile = image.profile;
            assert!(ReleaseAdmission::bundled(profile, false).is_err());
            let admission = ReleaseAdmission::bundled(profile, true).unwrap();
            assert_eq!(admission.version(), beta::BETA_RELEASE);
            let nvm = bundled_defaults(profile).unwrap();
            assert_eq!(sha256(&nvm), expected[profile.as_str()].as_str().unwrap());
            assert!(admission
                .validate_payload(profile, &vec![0; 51136])
                .is_err());
            // Public linked files do not substitute for a private OEM source.
            assert!(bundled_payload(profile, &[0; 65536], None).is_err());
        }
    }
    fn api() -> serde_json::Value {
        serde_json::json!({"tag_name":beta::BETA_RELEASE,"html_url":RELEASE_PAGE,"draft":false,"prerelease":true,
            "assets":[
                {"name":"AK820-Beta2-Catalog.zip","size":4034329,"state":"uploaded","digest":format!("sha256:{}",beta::SOURCE_ARCHIVE_SHA256),"browser_download_url":format!("https://github.com/dvino/ak820/releases/download/{}/AK820-Beta2-Catalog.zip",beta::BETA_RELEASE)},
                {"name":"release-catalog.json","size":29522,"state":"uploaded","digest":format!("sha256:{RELEASE_SHA256}"),"browser_download_url":format!("https://github.com/dvino/ak820/releases/download/{}/release-catalog.json",beta::BETA_RELEASE)},
                {"name":"release-manifest.json","size":29281,"state":"uploaded","digest":format!("sha256:{MANIFEST_SHA256}"),"browser_download_url":format!("https://github.com/dvino/ak820/releases/download/{}/release-manifest.json",beta::BETA_RELEASE)}]})
    }
    fn observe(value: &serde_json::Value) -> BackupResult<ReleaseObservation> {
        observe_api(
            &serde_json::to_vec(value).unwrap(),
            "2026-10-26T12:00:00Z".into(),
        )
    }
    #[test]
    fn withdrawal_changes_are_block_only_and_unknown_does_not_claim_freshness() {
        assert!(!observe(&api()).unwrap().blocked);
        for field in ["digest", "browser_download_url", "state"] {
            let mut changed = api();
            changed["assets"][0][field] = "changed".into();
            assert!(observe(&changed).unwrap().blocked);
        }
        let mut missing = api();
        missing["assets"].as_array_mut().unwrap().remove(0);
        let mut remembered = observe(&missing).unwrap();
        assert!(remembered.blocked);
        remembered.apply(observe(&api()).unwrap());
        assert!(remembered.blocked);
        assert!(ReleaseAdmission::bundled(ProfileId::Main, true)
            .unwrap()
            .with_observation(&remembered)
            .is_err());
        let mut unknown = api();
        unknown["assets"][0]["digest"] = serde_json::Value::Null;
        assert!(observe(&unknown).is_err());
        unknown["draft"] = true.into();
        assert!(observe(&unknown).unwrap().blocked);
        let mut mixed = api();
        mixed["assets"].as_array_mut().unwrap().remove(0);
        mixed["assets"][0]["digest"] = serde_json::Value::Null;
        assert!(observe(&mixed).unwrap().blocked);
        let mut foreign = api();
        foreign["html_url"] = "https://example.org".into();
        assert!(observe(&foreign).is_err());
    }
}
