//! Application-only update delivery. This module has no device or preferences APIs.
//! SHA256 establishes integrity against GitHub metadata, not publisher authenticity.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
const API: &str = "https://api.github.com/repos/dvino/ak-configurator/releases?per_page=100";
const BASE: &str = "https://github.com/dvino/ak-configurator/releases";
const INDEX: &str = "app-update-index.json";
const META_LIMIT: u64 = 2 * 1024 * 1024;
const PACKAGE_LIMIT: u64 = 512 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Channel {
    #[default]
    Stable,
    IncludePrereleases,
}
#[derive(Clone, Debug)]
pub(crate) struct Offer {
    pub tag: String,
    pub version: String,
    pub release_url: String,
    pub name: String,
    pub size: u64,
    pub sha256: String,
    url: String,
}
impl Offer {
    pub(crate) fn asset_url(&self) -> &str {
        &self.url
    }
}
#[derive(Debug)]
pub(crate) struct VerifiedDownload {
    pub path: PathBuf,
}
impl VerifiedDownload {
    pub(crate) fn installer_instructions(&self) -> &'static str {
        "Close AK Configurator, extract the verified archive, and follow its INSTALL guide to install for the current user. Keep the application settings directory. No installer is started automatically."
    }
}
#[derive(Debug)]
pub(crate) struct Job<T> {
    receiver: mpsc::Receiver<Result<T, String>>,
    cancelled: Arc<AtomicBool>,
}
impl<T> Job<T> {
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
    pub(crate) fn poll(&self) -> Option<Result<T, String>> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Some(Err("Application update cancelled".into()));
        }
        match self.receiver.try_recv() {
            Ok(v) => Some(v),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err("Application update worker stopped".into())),
        }
    }
}
impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn spawn<T: Send + 'static>(
    work: impl FnOnce(Arc<AtomicBool>) -> Result<T, String> + Send + 'static,
) -> Job<T> {
    let (sender, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    std::thread::spawn(move || {
        let result = work(flag);
        let _ = sender.send(result);
    });
    Job {
        receiver,
        cancelled,
    }
}
pub(crate) fn start_check(channel: Channel) -> Job<Offer> {
    spawn(move |cancel| check(channel, &cancel))
}
pub(crate) fn start_download(offer: Offer) -> Job<VerifiedDownload> {
    spawn(move |cancel| {
        let root = crate::cache_dir()
            .ok_or("No application cache directory")?
            .join("ak-configurator")
            .join("app-updates");
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let current = check(
            if canonical_version(&offer.tag).is_some_and(|v| !v.pre.is_empty()) {
                Channel::IncludePrereleases
            } else {
                Channel::Stable
            },
            &cancel,
        )?;
        if current.tag != offer.tag
            || current.name != offer.name
            || current.size != offer.size
            || current.sha256 != offer.sha256
            || current.url != offer.url
        {
            return Err("Release changed or was withdrawn; check for updates again".into());
        }
        if canonical_version(&offer.tag)
            .ok_or("Invalid offered version")?
            .cmp_precedence(
                &semver::Version::parse(env!("CARGO_PKG_VERSION"))
                    .map_err(|_| "Invalid running version")?,
            )
            != std::cmp::Ordering::Greater
        {
            return Err("Application update must be newer than the running version".into());
        }
        let response = request(&offer.url, &cancel)?;
        save_verified(response.into_reader(), &offer, &root, &cancel)
    })
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    schema_version: u32,
    tag: String,
    version: String,
    prerelease: bool,
    assets: Vec<Entry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    name: String,
    target: String,
    platform: String,
    arch: String,
    url: String,
    sha256: String,
    size: u64,
}
fn canonical_version(tag: &str) -> Option<semver::Version> {
    if tag.len() > 96 {
        return None;
    }
    let version = semver::Version::parse(tag.strip_prefix('v')?).ok()?;
    (version.build.is_empty() && format!("v{version}") == tag).then_some(version)
}
fn select_release(bytes: &[u8], channel: Channel) -> Result<Release, String> {
    if bytes.len() as u64 > META_LIMIT {
        return Err("Release metadata exceeds limit".into());
    }
    let releases: Vec<Release> =
        serde_json::from_slice(bytes).map_err(|e| format!("Invalid release metadata: {e}"))?;
    if releases.len() > 100 {
        return Err("Too many releases".into());
    }
    releases
        .into_iter()
        .filter(|r| !r.draft && (channel == Channel::IncludePrereleases || !r.prerelease))
        .filter_map(|r| {
            let v = canonical_version(&r.tag_name)?;
            if r.prerelease != !v.pre.is_empty()
                || r.html_url != format!("{BASE}/tag/{}", r.tag_name)
                || r.assets.len() > 64
            {
                return None;
            }
            Some((v, r))
        })
        .max_by(|a, b| a.0.cmp_precedence(&b.0))
        .map(|(_, r)| r)
        .ok_or_else(|| {
            "No eligible published application release; pre-releases require opt-in".into()
        })
}
fn platform(os: &str, arch: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match (os, arch) {
        ("linux", "x86_64") => Some(("x86_64", "x86_64-unknown-linux-gnu", "tar.gz")),
        ("linux", "aarch64") => Some(("arm64", "aarch64-unknown-linux-gnu", "tar.gz")),
        ("windows", "x86_64") => Some(("x86_64", "x86_64-pc-windows-msvc", "zip")),
        ("windows", "aarch64") => Some(("arm64", "aarch64-pc-windows-msvc", "zip")),
        ("macos", "x86_64") => Some(("x86_64", "x86_64-apple-darwin", "app.zip")),
        ("macos", "aarch64") => Some(("arm64", "aarch64-apple-darwin", "app.zip")),
        _ => None,
    }
}
fn hash_valid(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn asset<'a>(release: &'a Release, name: &str) -> Result<&'a Asset, String> {
    let mut matches = release.assets.iter().filter(|a| a.name == name);
    let a = matches.next().ok_or("Required release asset missing")?;
    if matches.next().is_some()
        || a.browser_download_url != format!("{BASE}/download/{}/{name}", release.tag_name)
        || a.size == 0
        || a.size > PACKAGE_LIMIT
    {
        return Err("Invalid or duplicate release asset".into());
    }
    Ok(a)
}
fn digest(a: &Asset) -> Result<&str, String> {
    a.digest
        .as_deref()
        .and_then(|s| s.strip_prefix("sha256:"))
        .filter(|s| hash_valid(s))
        .ok_or_else(|| "Release asset lacks a valid GitHub SHA256 digest".into())
}
fn offer_from_index(
    release: &Release,
    bytes: &[u8],
    os: &str,
    arch: &str,
) -> Result<Offer, String> {
    let index_asset = asset(release, INDEX)?;
    verify(bytes, index_asset.size, digest(index_asset)?)?;
    let index: Index = serde_json::from_slice(bytes)
        .map_err(|e| format!("Invalid application update index: {e}"))?;
    let version = canonical_version(&release.tag_name)
        .ok_or("Invalid release version")?
        .to_string();
    if index.schema_version != 1
        || index.tag != release.tag_name
        || index.version != version
        || index.prerelease != release.prerelease
        || index.assets.len() != 6
    {
        return Err("Update index release identity or six-target matrix mismatch".into());
    }
    let mut targets = std::collections::HashSet::new();
    for entry in &index.assets {
        let (normalized, target, ext) = platform(
            &entry.platform,
            if entry.arch == "arm64" {
                "aarch64"
            } else {
                &entry.arch
            },
        )
        .ok_or("Unsupported index platform")?;
        let expected = format!(
            "ak-configurator-{version}-{}-{normalized}.{ext}",
            entry.platform
        );
        let api = asset(release, &expected)?;
        if entry.name != expected
            || entry.target != target
            || entry.arch != normalized
            || !targets.insert(target)
            || entry.url != api.browser_download_url
            || entry.size != api.size
            || !hash_valid(&entry.sha256)
            || entry.sha256 != digest(api)?
        {
            return Err("Update index asset identity, size or digest mismatch".into());
        }
    }
    let (normalized, _, _) = platform(os, arch).ok_or("Unsupported application update platform")?;
    let entry = index
        .assets
        .into_iter()
        .find(|e| e.platform == os && e.arch == normalized)
        .ok_or("No package for this platform")?;
    Ok(Offer {
        tag: release.tag_name.clone(),
        version,
        release_url: release.html_url.clone(),
        name: entry.name,
        size: entry.size,
        sha256: entry.sha256,
        url: entry.url,
    })
}
fn cancelled(flag: &AtomicBool) -> Result<(), String> {
    if flag.load(Ordering::SeqCst) {
        Err("Application update cancelled".into())
    } else {
        Ok(())
    }
}
fn request(url: &str, flag: &AtomicBool) -> Result<ureq::Response, String> {
    cancelled(flag)?;
    // Disable automatic redirects so every hop is checked before its request.
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(30))
        .build();
    let mut location = url.to_owned();
    for _ in 0..5 {
        cancelled(flag)?;
        let parsed = url::Url::parse(&location).map_err(|_| "Invalid update URL")?;
        if parsed.scheme() != "https"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.port().is_some()
            || parsed.fragment().is_some()
            || !matches!(
                parsed.host_str(),
                Some(
                    "api.github.com"
                        | "github.com"
                        | "release-assets.githubusercontent.com"
                        | "objects.githubusercontent.com"
                )
            )
        {
            return Err("Untrusted application update redirect".into());
        }
        let response = agent
            .get(&location)
            .set(
                "User-Agent",
                concat!("AK-Configurator/", env!("CARGO_PKG_VERSION")),
            )
            .set("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| format!("Application update request failed: {e}"))?;
        if matches!(response.status(), 301 | 302 | 303 | 307 | 308) {
            location = parsed
                .join(
                    response
                        .header("Location")
                        .ok_or("Redirect missing location")?,
                )
                .map_err(|_| "Invalid redirect")?
                .to_string();
            continue;
        }
        if response.status() != 200 {
            return Err("Unexpected application update response".into());
        }
        return Ok(response);
    }
    Err("Too many application update redirects".into())
}
fn bounded(mut reader: impl Read, limit: u64, flag: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buffer = [0; 16384];
    let start = Instant::now();
    loop {
        cancelled(flag)?;
        if start.elapsed() > Duration::from_secs(120) {
            return Err("Application update read timed out".into());
        }
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if out.len() as u64 + n as u64 > limit {
            return Err("Application update response exceeds limit".into());
        }
        out.extend_from_slice(&buffer[..n]);
    }
    Ok(out)
}
fn check(channel: Channel, flag: &AtomicBool) -> Result<Offer, String> {
    let bytes = bounded(request(API, flag)?.into_reader(), META_LIMIT, flag)?;
    let release = select_release(&bytes, channel)?;
    let index = asset(&release, INDEX)?;
    if index.size > META_LIMIT {
        return Err("Update index exceeds limit".into());
    }
    let bytes = bounded(
        request(&index.browser_download_url, flag)?.into_reader(),
        META_LIMIT,
        flag,
    )?;
    offer_from_index(
        &release,
        &bytes,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}
fn verify(bytes: &[u8], size: u64, hash: &str) -> Result<(), String> {
    if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(bytes)) != hash {
        Err("Application update size or SHA256 mismatch".into())
    } else {
        Ok(())
    }
}
fn save_verified(
    mut reader: impl Read,
    offer: &Offer,
    root: &std::path::Path,
    flag: &AtomicBool,
) -> Result<VerifiedDownload, String> {
    if !hash_valid(&offer.sha256)
        || offer.size == 0
        || offer.size > PACKAGE_LIMIT
        || !offer
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        || offer.name.contains("..")
    {
        return Err("Invalid application download identity".into());
    }
    let destination = root.join(format!("{}-{}", offer.sha256, offer.name));
    if let Ok(metadata) = std::fs::symlink_metadata(&destination) {
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() != offer.size
        {
            return Err("Existing application cache object is invalid".into());
        }
        let mut existing = std::fs::File::open(&destination).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut buffer = [0; 65536];
        loop {
            cancelled(flag)?;
            let n = existing.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > offer.size {
                return Err("Existing application cache exceeds expected size".into());
            }
            hash.update(&buffer[..n]);
        }
        if count != offer.size || format!("{:x}", hash.finalize()) != offer.sha256 {
            return Err("Existing application cache SHA256 mismatch".into());
        }
        return Ok(VerifiedDownload { path: destination });
    }
    let mut stage = tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut count = 0u64;
    let mut buf = [0; 65536];
    let start = Instant::now();
    loop {
        cancelled(flag)?;
        if start.elapsed() > Duration::from_secs(600) {
            return Err("Application download timed out".into());
        }
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > offer.size {
            return Err("Application download exceeds expected size".into());
        }
        stage.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        hasher.update(&buf[..n]);
    }
    if count != offer.size || format!("{:x}", hasher.finalize()) != offer.sha256 {
        return Err("Application update size or SHA256 mismatch".into());
    }
    stage.as_file().sync_all().map_err(|e| e.to_string())?;
    stage.as_file_mut().rewind().map_err(|e| e.to_string())?;
    let mut reread_hash = Sha256::new();
    let mut reread_size = 0u64;
    loop {
        cancelled(flag)?;
        let n = stage.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        reread_size += n as u64;
        if reread_size > offer.size {
            return Err("Cached archive grew during verification".into());
        }
        reread_hash.update(&buf[..n]);
    }
    if reread_size != offer.size || format!("{:x}", reread_hash.finalize()) != offer.sha256 {
        return Err("Cached archive verification failed".into());
    }
    cancelled(flag)?;
    // Never overwrite an existing cache object, including a symlink.
    stage.persist_noclobber(&destination).map_err(|e| {
        format!(
            "Could not publish verified application archive: {}",
            e.error
        )
    })?;
    Ok(VerifiedDownload { path: destination })
}
use std::io::Seek;

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Release, Vec<u8>) {
        let tag = "v99.0.0";
        let version = "99.0.0";
        let mut entries = vec![];
        let mut assets = vec![];
        for os in ["linux", "windows", "macos"] {
            for arch in ["x86_64", "aarch64"] {
                let (normalized, target, ext) = platform(os, arch).unwrap();
                let name = format!("ak-configurator-{version}-{os}-{normalized}.{ext}");
                let url = format!("{BASE}/download/{tag}/{name}");
                let hash = format!("{:x}", Sha256::digest(b"archive"));
                entries.push(serde_json::json!({"name":name,"target":target,"platform":os,"arch":normalized,"url":url,"sha256":hash,"size":7}));
                assets.push(Asset {
                    name,
                    browser_download_url: url,
                    size: 7,
                    digest: Some(format!("sha256:{hash}")),
                });
            }
        }
        let bytes=serde_json::to_vec(&serde_json::json!({"schema_version":1,"tag":tag,"version":version,"prerelease":false,"assets":entries})).unwrap();
        assets.push(Asset {
            name: INDEX.into(),
            browser_download_url: format!("{BASE}/download/{tag}/{INDEX}"),
            size: bytes.len() as u64,
            digest: Some(format!("sha256:{:x}", Sha256::digest(&bytes))),
        });
        (
            Release {
                tag_name: tag.into(),
                html_url: format!("{BASE}/tag/{tag}"),
                draft: false,
                prerelease: false,
                assets,
            },
            bytes,
        )
    }
    #[test]
    fn verified_index_maps_all_six_and_rejects_corruption() {
        let (release, bytes) = fixture();
        for os in ["linux", "windows", "macos"] {
            for arch in ["x86_64", "aarch64"] {
                let offer = offer_from_index(&release, &bytes, os, arch).unwrap();
                assert!(offer.name.contains(os));
                assert_eq!(offer.size, 7);
            }
        }
        assert!(offer_from_index(&release, &bytes, "linux", "i686").is_err());
        let mut corrupt = bytes.clone();
        corrupt[0] ^= 1;
        assert!(offer_from_index(&release, &corrupt, "linux", "x86_64").is_err());
    }
    #[test]
    fn verified_index_requires_api_digest_and_unique_assets() {
        let (mut release, bytes) = fixture();
        release.assets[0].digest = None;
        assert!(offer_from_index(&release, &bytes, "linux", "x86_64").is_err());
        let (mut release, bytes) = fixture();
        release.assets[0].browser_download_url = "https://evil.invalid/payload".into();
        assert!(offer_from_index(&release, &bytes, "linux", "x86_64").is_err());
        let (mut release, bytes) = fixture();
        release.assets.push(Asset {
            name: INDEX.into(),
            browser_download_url: format!("{BASE}/download/v99.0.0/{INDEX}"),
            size: bytes.len() as u64,
            digest: None,
        });
        assert!(offer_from_index(&release, &bytes, "linux", "x86_64").is_err());
    }
    #[test]
    fn verified_stable_default_excludes_prerelease_and_draft() {
        let release = |tag: &str, pre: bool, draft: bool| serde_json::json!({"tag_name":tag,"html_url":format!("{BASE}/tag/{tag}"),"prerelease":pre,"draft":draft,"assets":[]});
        let bytes = serde_json::to_vec(&vec![
            release("v1.0.0", false, false),
            release("v2.0.0-rc.1", true, false),
            release("v9.0.0", false, true),
        ])
        .unwrap();
        assert_eq!(
            select_release(&bytes, Channel::default()).unwrap().tag_name,
            "v1.0.0"
        );
        assert_eq!(
            select_release(&bytes, Channel::IncludePrereleases)
                .unwrap()
                .tag_name,
            "v2.0.0-rc.1"
        );
        assert!(select_release(
            &serde_json::to_vec(&vec![release("v1.0.0-beta.1", true, false)]).unwrap(),
            Channel::Stable
        )
        .is_err());
    }
    #[test]
    fn verified_download_publishes_only_complete_matching_bytes() {
        let (release, bytes) = fixture();
        let offer = offer_from_index(&release, &bytes, "linux", "x86_64").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let flag = AtomicBool::new(false);
        assert!(save_verified(&b"bad"[..], &offer, dir.path(), &flag).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        let verified = save_verified(&b"archive"[..], &offer, dir.path(), &flag).unwrap();
        assert_eq!(std::fs::read(&verified.path).unwrap(), b"archive");
        assert_eq!(
            save_verified(&b"archive"[..], &offer, dir.path(), &flag)
                .unwrap()
                .path,
            verified.path
        );
        assert_eq!(std::fs::read(&verified.path).unwrap(), b"archive");
    }
    #[test]
    fn verified_download_cancel_and_read_failure_leave_no_archive() {
        let (release, bytes) = fixture();
        let offer = offer_from_index(&release, &bytes, "linux", "x86_64").unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert!(
            save_verified(&b"archive"[..], &offer, dir.path(), &AtomicBool::new(true)).is_err()
        );
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("transport failed"))
            }
        }
        assert!(save_verified(Broken, &offer, dir.path(), &AtomicBool::new(false)).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    #[test]
    fn verified_index_rejects_unknown_duplicate_and_traversal_fields() {
        for kind in 0..3 {
            let (mut release, bytes) = fixture();
            let changed = if kind == 0 {
                String::from_utf8(bytes)
                    .unwrap()
                    .replacen(
                        "\"schema_version\":1",
                        "\"schema_version\":1,\"schema_version\":1",
                        1,
                    )
                    .into_bytes()
            } else {
                let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                if kind == 1 {
                    value["unexpected"] = true.into();
                } else {
                    value["assets"][0]["name"] = "../payload".into();
                }
                serde_json::to_vec(&value).unwrap()
            };
            let metadata = release.assets.iter_mut().find(|a| a.name == INDEX).unwrap();
            metadata.size = changed.len() as u64;
            metadata.digest = Some(format!("sha256:{:x}", Sha256::digest(&changed)));
            assert!(offer_from_index(&release, &changed, "linux", "x86_64").is_err());
        }
    }
    #[test]
    fn verified_metadata_bounds_and_canonical_versions() {
        assert!(bounded(&b"1234"[..], 3, &AtomicBool::new(false)).is_err());
        assert!(canonical_version("v1.0.0+other").is_none());
        assert!(canonical_version("v01.0.0").is_none());
        let flag = AtomicBool::new(true);
        assert!(request("https://evil.invalid", &flag).is_err());
    }
}
