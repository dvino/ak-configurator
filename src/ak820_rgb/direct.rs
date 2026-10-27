//! Exact beta.3 Direct v1 wire contract. No legacy fallback or write retries.
use super::{LED_COUNT, RGB_MAX_BRIGHTNESS};
use anyhow::{Result, bail, ensure};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) type Hsv = [[u8; 3]; LED_COUNT];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub generation: u32,
    pub flags: u8,
    pub hsv: Hsv,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Capability {
    version: u8,
    capabilities: u8,
    count: u8,
    encoding: String,
    mapping: u8,
    maximum_brightness: u8,
    read_chunk: u8,
    upload_chunk: u8,
    timeout_ms: u16,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Sidecar {
    format: String,
    capability: Capability,
    generation: u32,
    flags: u8,
    hsv_hex: String,
}
impl Snapshot {
    /// Firmware-compatible separate direct-ram.json; this is not an NVM backup.
    pub(crate) fn encode(&self) -> Result<String> {
        let hsv_hex = self
            .hsv
            .iter()
            .flatten()
            .map(|v| format!("{v:02x}"))
            .collect::<String>();
        Ok(serde_json::to_string_pretty(&Sidecar {
            format: "ak820-direct-ram-1".into(),
            capability: Capability {
                version: 1,
                capabilities: 3,
                count: 82,
                encoding: "HSV8".into(),
                mapping: 1,
                maximum_brightness: 160,
                read_chunk: 7,
                upload_chunk: 8,
                timeout_ms: 1000,
            },
            generation: self.generation,
            flags: self.flags,
            hsv_hex,
        })?)
    }
    pub(crate) fn decode(text: &str) -> Result<Self> {
        ensure!(text.len() <= 16 * 1024, "Direct RAM sidecar exceeds 16 KiB");
        let s: Sidecar = serde_json::from_str(text)?;
        let c = s.capability;
        ensure!(
            s.format == "ak820-direct-ram-1"
                && s.flags <= 7
                && c.version == 1
                && c.capabilities == 3
                && c.count == 82
                && c.encoding == "HSV8"
                && c.mapping == 1
                && c.maximum_brightness == 160
                && c.read_chunk == 7
                && c.upload_chunk == 8
                && c.timeout_ms == 1000,
            "Direct RAM sidecar contract mismatch"
        );
        ensure!(
            s.hsv_hex.len() == LED_COUNT * 6
                && s.hsv_hex
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Direct RAM colors must contain exactly 82 lowercase HSV triples"
        );
        let mut hsv = [[0; 3]; LED_COUNT];
        for (i, byte) in hsv.iter_mut().flatten().enumerate() {
            *byte = u8::from_str_radix(&s.hsv_hex[i * 2..i * 2 + 2], 16)?;
        }
        ensure!(
            hsv.iter().all(|v| v[2] <= 160),
            "Direct RAM brightness exceeds contract"
        );
        Ok(Self {
            generation: s.generation,
            flags: s.flags,
            hsv,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Status {
    pub state: u8,
    pub coverage: u8,
    pub frame_id: u16,
    pub generation: u32,
    pub flags: u8,
}

type Packet = [u8; 32];
#[derive(Debug)]
pub(crate) struct Cancelled;
impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Direct operation cancelled")
    }
}
impl std::error::Error for Cancelled {}
pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        return Err(Cancelled.into());
    }
    Ok(())
}
fn packet(op: u8, command: u8) -> Packet {
    let mut p = [0; 32];
    p[..2].copy_from_slice(&[op, command]);
    p
}

pub(crate) fn negotiate(send: &mut impl FnMut(&[u8]) -> Result<Packet>) -> Result<()> {
    let r = send(&packet(8, 0x45))?;
    ensure!(
        r[..14] == [8, 0x45, b'A', b'D', 1, 3, 82, 1, 1, 160, 7, 8, 0xe8, 3],
        "Exact Direct v1 runtime capabilities are unavailable"
    );
    Ok(())
}

pub(crate) fn status(send: &mut impl FnMut(&[u8]) -> Result<Packet>) -> Result<Status> {
    let r = send(&packet(8, 0x47))?;
    ensure!(
        r[..2] == [8, 0x47] && r[2] <= 6 && r[3] <= 82 && r[10] & !7 == 0,
        "Malformed Direct status"
    );
    Ok(Status {
        state: r[2],
        coverage: r[3],
        frame_id: u16::from_le_bytes(r[4..6].try_into()?),
        generation: u32::from_le_bytes(r[6..10].try_into()?),
        flags: r[10],
    })
}

/// Only a generation race restarts a read, at most three times. Every malformed
/// response or transport failure stops immediately. Final flags are observational.
pub(crate) fn snapshot(send: &mut impl FnMut(&[u8]) -> Result<Packet>) -> Result<Snapshot> {
    negotiate(send)?;
    for _ in 0..3 {
        let generation = status(send)?.generation;
        let mut hsv = [[0; 3]; LED_COUNT];
        let mut stale = false;
        for first in (0..LED_COUNT).step_by(7) {
            let count = (LED_COUNT - first).min(7);
            let mut p = packet(8, 0x46);
            p[2..6].copy_from_slice(&generation.to_le_bytes());
            p[6..8].copy_from_slice(&[first as u8, count as u8]);
            let r = send(&p)?;
            ensure!(
                r[..2] == [8, 0x46] && r[3..5] == [first as u8, count as u8] && r[5] & !7 == 0,
                "Malformed Direct palette response"
            );
            if r[2] == 3 {
                stale = true;
                break;
            }
            ensure!(
                r[2] == 0 && u32::from_le_bytes(r[6..10].try_into()?) == generation,
                "Direct palette read failed or generation mismatch"
            );
            for i in 0..count {
                hsv[first + i].copy_from_slice(&r[10 + i * 3..13 + i * 3]);
                ensure!(
                    hsv[first + i][2] <= RGB_MAX_BRIGHTNESS,
                    "Direct brightness exceeds contract"
                );
            }
        }
        if !stale {
            let final_status = status(send)?;
            if final_status.generation == generation {
                return Ok(Snapshot {
                    generation,
                    flags: final_status.flags,
                    hsv,
                });
            }
        }
    }
    bail!("Direct palette changed during bounded snapshot")
}

fn set(
    send: &mut impl FnMut(&[u8]) -> Result<Packet>,
    p: &Packet,
    cancel: &AtomicBool,
) -> Result<()> {
    check_cancel(cancel)?;
    let r = send(p)?;
    ensure!(
        r[..2] == p[..2] && r[3..] == p[3..] && r[2] == 0,
        "Direct setter failed or ACK mismatch"
    );
    Ok(())
}

/// Success establishes logical commit acceptance and RAM readback. The returned
/// state separately records conversion/publication progress; it is not LED proof.
pub(crate) fn commit(
    send: &mut impl FnMut(&[u8]) -> Result<Packet>,
    hsv: &Hsv,
    frame_id: u16,
    generation: u32,
    cancel: &AtomicBool,
) -> Result<Status> {
    ensure!(frame_id != 0, "Direct frame ID must be nonzero");
    negotiate(send)?;
    let normalized = hsv.map(|[h, s, v]| [h, s, v.min(RGB_MAX_BRIGHTNESS)]);
    let mut p = packet(7, 0x43);
    p[2..4].copy_from_slice(&[1, LED_COUNT as u8]);
    p[4..6].copy_from_slice(&frame_id.to_le_bytes());
    p[6..10].copy_from_slice(&generation.to_le_bytes());
    set(send, &p, cancel)?;
    for first in (0..LED_COUNT).step_by(8) {
        let count = (LED_COUNT - first).min(8);
        let mut p = packet(7, 0x44);
        p[2..4].copy_from_slice(&frame_id.to_le_bytes());
        p[4..6].copy_from_slice(&[first as u8, count as u8]);
        for i in 0..count {
            p[6 + i * 3..9 + i * 3].copy_from_slice(&normalized[first + i]);
        }
        set(send, &p, cancel)?;
    }
    let mut p = packet(7, 0x45);
    p[2..4].copy_from_slice(&frame_id.to_le_bytes());
    set(send, &p, cancel)?;
    let accepted = status(send)?;
    ensure!(
        accepted.frame_id == frame_id
            && accepted.coverage == 82
            && accepted.generation == generation.wrapping_add(1)
            && (2..=5).contains(&accepted.state),
        "Direct commit status mismatch"
    );
    let actual = snapshot(send)?;
    ensure!(
        actual.generation == accepted.generation && actual.hsv == normalized,
        "Direct committed palette readback mismatch"
    );
    Ok(accepted)
}

/// Read-only bounded waiting permits the next frame only after the prior accepted
/// transaction finishes. Cancellation or failure never sends abort or restoration.
pub(crate) fn wait_ready(
    send: &mut impl FnMut(&[u8]) -> Result<Packet>,
    expected_generation: u32,
    cancel: &AtomicBool,
) -> Result<bool> {
    let start = std::time::Instant::now();
    loop {
        check_cancel(cancel)?;
        let s = status(send)?;
        if s.generation != expected_generation {
            return Ok(false);
        }
        match s.state {
            0 | 5 | 6 => return Ok(true),
            2..=4 if start.elapsed() < std::time::Duration::from_millis(1000) => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            _ => bail!("Direct transaction did not become ready"),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) struct Fake {
        pub hsv: Hsv,
        pending: Hsv,
        pub generation: u32,
        pub flags: u8,
        frame_id: u16,
        coverage: u8,
        state: u8,
        pub fail_chunk: Option<u8>,
        pub bad_readback: bool,
        pub stale_reads: usize,
        pub writes: Vec<Packet>,
    }
    impl Default for Fake {
        fn default() -> Self {
            Self {
                hsv: [[17, 99, 73]; LED_COUNT],
                pending: [[0; 3]; LED_COUNT],
                generation: u32::MAX,
                flags: 3,
                frame_id: 0,
                coverage: 0,
                state: 0,
                fail_chunk: None,
                bad_readback: false,
                stale_reads: 0,
                writes: Vec::new(),
            }
        }
    }
    impl Fake {
        pub(crate) fn send(&mut self, bytes: &[u8]) -> Result<Packet> {
            let mut p = [0; 32];
            p[..bytes.len()].copy_from_slice(bytes);
            let mut r = p;
            match (p[0], p[1]) {
                (8, 0x45) => r[..14]
                    .copy_from_slice(&[8, 0x45, b'A', b'D', 1, 3, 82, 1, 1, 160, 7, 8, 0xe8, 3]),
                (8, 0x47) => {
                    r[2] = self.state;
                    r[3] = self.coverage;
                    r[4..6].copy_from_slice(&self.frame_id.to_le_bytes());
                    r[6..10].copy_from_slice(&self.generation.to_le_bytes());
                    r[10] = self.flags;
                }
                (8, 0x46) => {
                    let first = p[6] as usize;
                    let count = p[7] as usize;
                    r = packet(8, 0x46);
                    r[3] = first as u8;
                    r[4] = count as u8;
                    r[5] = self.flags;
                    if self.stale_reads > 0 {
                        self.stale_reads -= 1;
                        self.generation = self.generation.wrapping_add(1);
                        r[2] = 3;
                    }
                    r[6..10].copy_from_slice(&self.generation.to_le_bytes());
                    for i in 0..count {
                        r[10 + i * 3..13 + i * 3].copy_from_slice(&self.hsv[first + i]);
                    }
                    if self.bad_readback {
                        r[10] ^= 1;
                    }
                }
                (7, 0x43) => {
                    self.writes.push(p);
                    ensure!(
                        u32::from_le_bytes(p[6..10].try_into()?) == self.generation,
                        "stale begin"
                    );
                    self.frame_id = u16::from_le_bytes(p[4..6].try_into()?);
                    self.coverage = 0;
                    self.state = 1;
                    r[2] = 0;
                }
                (7, 0x44) => {
                    self.writes.push(p);
                    if self.fail_chunk == Some(p[4]) {
                        bail!("scripted chunk timeout");
                    }
                    ensure!(p[4] == self.coverage, "nonsequential chunk");
                    for i in 0..p[5] as usize {
                        self.pending[p[4] as usize + i].copy_from_slice(&p[6 + i * 3..9 + i * 3]);
                    }
                    self.coverage += p[5];
                    r[2] = 0;
                }
                (7, 0x45) => {
                    self.writes.push(p);
                    ensure!(self.coverage == 82, "incomplete commit");
                    self.hsv = self.pending;
                    self.generation = self.generation.wrapping_add(1);
                    self.state = 5;
                    r[2] = 0;
                }
                _ => bail!("unexpected Direct request"),
            }
            Ok(r)
        }
    }

    #[test]
    fn coherent_getter_reads_dark_palette_and_restarts_only_generation_races() {
        let mut fake = Fake::default();
        fake.stale_reads = 2;
        fake.flags = 0;
        let actual = snapshot(&mut |p| fake.send(p)).unwrap();
        assert_eq!(actual.generation, 1);
        assert_eq!(actual.flags, 0);
        assert_eq!(actual.hsv, fake.hsv);
        assert!(fake.writes.is_empty());
        fake.stale_reads = 3;
        assert!(snapshot(&mut |p| fake.send(p)).is_err());
        assert!(fake.writes.is_empty());
    }
    #[test]
    fn atomic_commit_wraps_generation_clamps_copy_and_checks_readback() {
        let mut fake = Fake::default();
        let input = [[20, 255, 255]; LED_COUNT];
        let status = commit(
            &mut |p| fake.send(p),
            &input,
            65535,
            u32::MAX,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(status.generation, 0);
        assert_eq!(input[0][2], 255);
        assert_eq!(fake.hsv, [[20, 255, 160]; LED_COUNT]);
        assert_eq!(fake.writes.len(), 13);
        assert_eq!(fake.writes.iter().filter(|p| p[1] == 0x45).count(), 1);
        let mut fake = Fake::default();
        fake.bad_readback = true;
        assert!(
            commit(
                &mut |p| fake.send(p),
                &input,
                1,
                u32::MAX,
                &AtomicBool::new(false)
            )
            .is_err()
        );
    }
    #[test]
    fn every_upload_failure_leaves_active_palette_and_stops_without_retry_abort_or_restore() {
        for first in (0..82).step_by(8) {
            let mut fake = Fake::default();
            fake.fail_chunk = Some(first);
            let original = fake.hsv;
            assert!(
                commit(
                    &mut |p| fake.send(p),
                    &[[0; 3]; LED_COUNT],
                    1,
                    u32::MAX,
                    &AtomicBool::new(false)
                )
                .is_err()
            );
            assert_eq!(fake.hsv, original);
            assert_eq!(fake.generation, u32::MAX);
            assert_eq!(fake.writes.len(), first as usize / 8 + 2);
            assert!(!fake.writes.iter().any(|p| p[1] == 0x45 || p[1] == 0x46));
        }
    }
    #[test]
    fn runtime_contract_mismatch_and_malformed_reads_fail_immediately() {
        for byte in 2..14 {
            let mut fake = Fake::default();
            assert!(
                negotiate(&mut |p| {
                    let mut r = fake.send(p)?;
                    r[byte] ^= 1;
                    Ok(r)
                })
                .is_err()
            );
        }
        let mut calls = 0;
        assert!(
            snapshot(&mut |p| {
                calls += 1;
                let mut f = Fake::default();
                let mut r = f.send(p)?;
                if p[1] == 0x46 {
                    r[3] ^= 1;
                }
                Ok(r)
            })
            .is_err()
        );
        assert_eq!(calls, 3);
    }

    #[test]
    fn accepted_commit_is_distinct_from_publication_and_wait_is_read_only() {
        let mut fake = Fake::default();
        let accepted = commit(
            &mut |p| {
                let mut r = fake.send(p)?;
                if p[..2] == [8, 0x47] {
                    r[2] = 3;
                }
                Ok(r)
            },
            &[[0; 3]; LED_COUNT],
            1,
            u32::MAX,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(accepted.state, 3);
        let writes = fake.writes.len();
        assert!(wait_ready(&mut |p| fake.send(p), 0, &AtomicBool::new(false)).unwrap());
        assert!(!wait_ready(&mut |p| fake.send(p), 1, &AtomicBool::new(false)).unwrap());
        assert_eq!(fake.writes.len(), writes);
    }

    #[test]
    fn begin_commit_and_status_failures_never_retry_or_abort() {
        for failed_request in [[7, 0x43], [7, 0x45], [8, 0x47]] {
            let mut fake = Fake::default();
            let mut attempts = Vec::new();
            assert!(
                commit(
                    &mut |p| {
                        attempts.push([p[0], p[1]]);
                        if p[..2] == failed_request {
                            bail!("scripted operation failure");
                        }
                        fake.send(p)
                    },
                    &[[0; 3]; LED_COUNT],
                    1,
                    u32::MAX,
                    &AtomicBool::new(false)
                )
                .is_err()
            );
            assert_eq!(attempts.last(), Some(&failed_request));
            assert_eq!(attempts.iter().filter(|p| **p == failed_request).count(), 1);
            assert!(!fake.writes.iter().any(|p| p[1] == 0x46));
            assert_eq!(
                fake.writes.iter().filter(|p| p[1] == 0x43).count(),
                usize::from(failed_request != [7, 0x43])
            );
        }
        let mut fake = Fake::default();
        assert!(
            commit(
                &mut |p| {
                    let mut r = fake.send(p)?;
                    if p[..2] == [7, 0x43] {
                        r[3] ^= 1;
                    }
                    Ok(r)
                },
                &[[0; 3]; LED_COUNT],
                1,
                u32::MAX,
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert_eq!(fake.writes.len(), 1);
    }

    #[test]
    fn sidecar_matches_firmware_schema_and_rejects_invalid_or_duplicate_fields() {
        let original = Snapshot {
            generation: u32::MAX,
            flags: 7,
            hsv: [[99, 200, 160]; LED_COUNT],
        };
        let encoded = original.encode().unwrap();
        assert_eq!(Snapshot::decode(&encoded).unwrap(), original);
        let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        for field in ["format", "capability", "generation", "flags", "hsv_hex"] {
            let mut changed = value.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert!(Snapshot::decode(&changed.to_string()).is_err());
        }
        let mut changed = value.clone();
        changed["capability"]["upload_chunk"] = 9.into();
        assert!(Snapshot::decode(&changed.to_string()).is_err());
        let mut changed = value.clone();
        changed["hsv_hex"] = "00ffa1".repeat(82).into();
        assert!(Snapshot::decode(&changed.to_string()).is_err());
        let mut changed = value.clone();
        changed["hsv_hex"] = "00FF00".repeat(82).into();
        assert!(Snapshot::decode(&changed.to_string()).is_err());
        let duplicate = encoded.replacen("\"flags\": 7", "\"flags\": 7, \"flags\": 7", 1);
        assert!(Snapshot::decode(&duplicate).is_err());
        let mut changed = value;
        changed["extra"] = true.into();
        assert!(Snapshot::decode(&changed.to_string()).is_err());
        assert!(Snapshot::decode(&" ".repeat(16 * 1024 + 1)).is_err());
    }
}
