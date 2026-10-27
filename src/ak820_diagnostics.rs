//! FD01 v1 RAM diagnostic snapshot, pinned to firmware beta.3 (88d190ab).
//! No ROM contents, serials, host paths, macros or typing data are exposed.
use anyhow::{ensure, Result};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct IapAttempt {
    pub command: u8,
    pub status_before: u8,
    pub status_after: u8,
    pub address: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Status {
    pub version: u8,
    pub storage_failed: bool,
    pub page_valid: bool,
    pub page_dirty: bool,
    pub batch_depth: u8,
    pub cached_page: Option<u16>,
    pub first_failure_stage: u8,
    pub last_iap_attempt: Option<IapAttempt>,
    pub unlocked: bool,
    pub unlock_in_progress: bool,
    pub usb_enumeration: u8,
    pub usb_sleep_flags: u8,
    pub raw_hid_state: u8,
}

impl Status {
    pub(crate) fn decode(reply: &[u8]) -> Result<Self> {
        ensure!(
            reply.len() == 32 && reply[..4] == [0xfd, 1, 1, 0xa8],
            "AK820 diagnostic status has an unknown format"
        );
        ensure!(
            [4, 5, 6, 11, 17, 18].iter().all(|&i| reply[i] <= 1)
                && reply[10] <= 16
                && reply[22..].iter().all(|&b| b == 0),
            "AK820 diagnostic status is malformed"
        );
        ensure!(
            reply[11] != 0 || reply[12..17].iter().all(|&b| b == 0),
            "AK820 diagnostic status has an invalid absent IAP record"
        );
        let word = |i| u16::from_le_bytes([reply[i], reply[i + 1]]);
        Ok(Self {
            version: 1,
            storage_failed: reply[4] == 1,
            page_valid: reply[5] == 1,
            page_dirty: reply[6] == 1,
            batch_depth: reply[7],
            cached_page: (reply[5] == 1).then(|| word(8)),
            first_failure_stage: reply[10],
            last_iap_attempt: (reply[11] == 1).then(|| IapAttempt {
                command: reply[12],
                status_before: reply[13],
                status_after: reply[14],
                address: word(15),
            }),
            unlocked: reply[17] == 1,
            unlock_in_progress: reply[18] == 1,
            usb_enumeration: reply[19],
            usb_sleep_flags: reply[20],
            raw_hid_state: reply[21],
        })
    }

    pub(crate) fn ensure_ready(&self) -> Result<()> {
        ensure!(
            !self.storage_failed && self.first_failure_stage == 0,
            "AK820 storage failure is latched"
        );
        ensure!(
            !self.page_dirty && self.batch_depth == 0 && !self.unlock_in_progress,
            "AK820 storage or unlock is busy"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean() -> [u8; 32] {
        let mut reply = [0; 32];
        reply[..4].copy_from_slice(&[0xfd, 1, 1, 0xa8]);
        reply[19] = 1;
        reply[21] = 3;
        reply
    }

    #[test]
    fn diagnostic_status_decodes_last_attempt_and_failure_without_clearing_it() {
        let mut reply = clean();
        reply[4..17]
            .copy_from_slice(&[1, 1, 1, 0, 0x34, 0x12, 12, 1, 0x9a, 0x40, 0x80, 0x56, 0xc7]);
        let status = Status::decode(&reply).unwrap();
        assert_eq!(status.cached_page, Some(0x1234));
        assert_eq!(status.first_failure_stage, 12);
        assert_eq!(status.last_iap_attempt.unwrap().address, 0xc756);
        assert!(Status::decode(&reply).unwrap().ensure_ready().is_err());
    }

    #[test]
    fn diagnostic_status_blocks_fault_dirty_batch_and_unlock_but_reports_snapshots() {
        Status::decode(&clean()).unwrap().ensure_ready().unwrap();
        for offset in [4, 6, 7, 10, 18] {
            let mut reply = clean();
            reply[offset] = 1;
            assert!(Status::decode(&reply).unwrap().ensure_ready().is_err());
        }
    }

    #[test]
    fn diagnostic_status_rejects_wrong_version_lengths_reserved_and_boolean_values() {
        assert!(Status::decode(&clean()[..31]).is_err());
        for (offset, value) in [
            (2, 2),
            (4, 2),
            (5, 2),
            (6, 2),
            (11, 2),
            (17, 2),
            (18, 2),
            (10, 17),
            (22, 1),
            (31, 1),
            (12, 1),
        ] {
            let mut reply = clean();
            reply[offset] = value;
            assert!(Status::decode(&reply).is_err(), "offset {offset}");
        }
    }
}
