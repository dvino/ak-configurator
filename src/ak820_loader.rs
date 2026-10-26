// SPDX-License-Identifier: GPL-3.0-or-later
//! OEM loader wire codec, ported from dvino/ak820 util/ak820_tool.py at
//! bef20f89d3da01d770767919bc7c8fc3c3a1a808. No USB adapter is implemented.
//! The fixed application window is the only representable write request.
use crate::ak820_image_pack::{APP_END, APP_START};
use crate::ak820_settings_backup::BackupResult;
use std::time::Duration;

pub(crate) const BLOCK_SIZE: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Identity = 1,
    Erase = 2,
    Begin = 3,
    Checksum = 4,
    Restart = 5,
}

/// Includes the leading HID report ID; send_feature_report must return 65.
pub(crate) fn command_report(command: Command, checksum: u32) -> [u8; 65] {
    let mut report = [0; 65];
    report[1..5].copy_from_slice(&[command as u8, 0xaa, 0x55, 0]);
    match command {
        Command::Begin => {
            report[5..9].copy_from_slice(&(APP_START as u32).to_le_bytes());
            report[9..13]
                .copy_from_slice(&(((APP_END - APP_START) / BLOCK_SIZE) as u32).to_le_bytes());
        }
        Command::Checksum => report[9..13].copy_from_slice(&checksum.to_le_bytes()),
        _ => {}
    }
    report
}
pub(crate) fn validate_reply(reply: &[u8], command: Command) -> BackupResult<()> {
    if reply.len() != 65
        || reply[..5] != [0, command as u8, 0xaa, 0x55, 0]
        || reply[5..9] != [0xfa; 4]
    {
        return Err("Unexpected loader reply or failed status".into());
    }
    if command == Command::Identity && reply[9..13] != [0, 0x11, 0x22, 0x33] {
        return Err("Unexpected OEM loader identity".into());
    }
    Ok(())
}
pub(crate) fn response_delay(command: Command) -> Duration {
    Duration::from_millis(match command {
        Command::Erase => 7000,
        Command::Checksum => 1000,
        _ => 100,
    })
}
/// Pure packetization; arbitrary full-ROM writes and partial images are rejected.
pub(crate) struct PayloadReports<'a> {
    blocks: std::slice::ChunksExact<'a, u8>,
}
impl<'a> PayloadReports<'a> {
    pub(crate) fn new(payload: &'a [u8]) -> BackupResult<Self> {
        if payload.len() != APP_END - APP_START {
            return Err("Loader requires the complete application window".into());
        }
        Ok(Self {
            blocks: payload.chunks_exact(BLOCK_SIZE),
        })
    }
}
impl Iterator for PayloadReports<'_> {
    type Item = [u8; 65];
    fn next(&mut self) -> Option<Self::Item> {
        self.blocks.next().map(|block| {
            let mut r = [0; 65];
            r[1..].copy_from_slice(block);
            r
        })
    }
}
pub(crate) fn checksum(payload: &[u8]) -> u32 {
    payload
        .iter()
        .fold(0u32, |sum, b| sum.wrapping_add(*b as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reviewed_feature_vectors_and_fixed_bounds() {
        let mut expected = [0; 65];
        expected[1..13].copy_from_slice(&[3, 0xaa, 0x55, 0, 0, 0x28, 0, 0, 0x1f, 3, 0, 0]);
        assert_eq!(command_report(Command::Begin, 99), expected);
        assert_eq!(
            &command_report(Command::Checksum, 0x12345678)[9..13],
            &[0x78, 0x56, 0x34, 0x12]
        );
        assert_eq!(response_delay(Command::Erase), Duration::from_secs(7));
        let payload = vec![0xff; APP_END - APP_START];
        let reports: Vec<_> = PayloadReports::new(&payload).unwrap().collect();
        assert_eq!(reports.len(), 799);
        assert!(reports.iter().all(|r| r[0] == 0 && r[1..] == [0xff; 64]));
        assert_eq!(checksum(&payload), (payload.len() * 255) as u32);
        for size in [0, 64, 65536, APP_END - APP_START - 1] {
            assert!(PayloadReports::new(&vec![0; size]).is_err());
        }
    }
    #[test]
    fn replies_reject_every_header_status_and_identity_corruption() {
        for command in [
            Command::Identity,
            Command::Erase,
            Command::Begin,
            Command::Checksum,
        ] {
            let mut r = command_report(command, 0);
            r[5..9].fill(0xfa);
            if command == Command::Identity {
                r[9..13].copy_from_slice(&[0, 0x11, 0x22, 0x33]);
            }
            assert!(validate_reply(&r, command).is_ok());
            for i in 0..if command == Command::Identity { 13 } else { 9 } {
                let mut bad = r;
                bad[i] ^= 1;
                assert!(validate_reply(&bad, command).is_err());
            }
            assert!(validate_reply(&r[..64], command).is_err());
            assert!(validate_reply(&[0; 66], command).is_err());
        }
    }
}
