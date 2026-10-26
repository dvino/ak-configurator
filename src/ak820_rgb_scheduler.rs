//! A single replaceable frame, never an accumulating animation command queue.
//! Timings are monotonic milliseconds supplied by the owner for deterministic tests.

use crate::ak820_rgb::{LED_COUNT, RGB_MAX_BRIGHTNESS};

pub(crate) type Frame = [[u8; 3]; LED_COUNT];
/// Conservative source limit, not a measured device frame-rate claim.
pub(crate) const FRAME_INTERVAL_MS: u64 = 100;
pub(crate) const SUSPEND_GAP_MS: u64 = 2_000;

#[derive(Default)]
pub(crate) struct FrameScheduler {
    pending: Option<Frame>,
    enabled: bool,
    next_due: u64,
}

impl FrameScheduler {
    pub(crate) fn resume(&mut self, now_ms: u64) {
        if !self.enabled {
            self.pending = None;
            self.next_due = self.next_due.max(now_ms);
            self.enabled = true;
        }
    }

    pub(crate) fn pause(&mut self) {
        self.enabled = false;
        self.pending = None;
    }

    pub(crate) fn submit(&mut self, frame: Frame) -> Result<(), &'static str> {
        if frame.iter().flatten().any(|&v| v > RGB_MAX_BRIGHTNESS) {
            return Err("RGB frame exceeds firmware brightness limit");
        }
        if self.enabled {
            self.pending = Some(frame);
        }
        Ok(())
    }

    pub(crate) fn take_due(&mut self, now_ms: u64) -> Option<Frame> {
        if !self.enabled || now_ms < self.next_due {
            return None;
        }
        let frame = self.pending.take()?;
        self.next_due = now_ms.saturating_add(FRAME_INTERVAL_MS);
        Some(frame)
    }

    /// Wait after completion as well as after dispatch: a slow device must
    /// never trigger catch-up bursts or starve user settings operations.
    pub(crate) fn completed(&mut self, now_ms: u64) {
        self.next_due = now_ms.saturating_add(FRAME_INTERVAL_MS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_frame_wins_without_backlog() {
        let mut s = FrameScheduler::default();
        s.resume(0);
        for n in 0..=160 {
            s.submit([[n, 0, 0]; LED_COUNT]).unwrap();
        }
        assert_eq!(s.take_due(0).unwrap()[0], [160, 0, 0]);
        assert!(s.take_due(10_000).is_none());
    }

    #[test]
    fn pause_discards_old_frame_and_requires_fresh_submission() {
        let mut s = FrameScheduler::default();
        s.resume(0);
        s.submit([[1; 3]; LED_COUNT]).unwrap();
        s.pause();
        s.submit([[2; 3]; LED_COUNT]).unwrap();
        assert!(s.take_due(100).is_none());
        s.resume(300);
        assert!(s.take_due(300).is_none());
        s.submit([[3; 3]; LED_COUNT]).unwrap();
        assert_eq!(s.take_due(300).unwrap()[0], [3; 3]);
    }

    #[test]
    fn slow_completion_does_not_catch_up() {
        let mut s = FrameScheduler::default();
        s.resume(0);
        s.submit([[1; 3]; LED_COUNT]).unwrap();
        assert!(s.take_due(0).is_some());
        s.submit([[2; 3]; LED_COUNT]).unwrap();
        s.completed(500);
        assert!(s.take_due(599).is_none());
        assert!(s.take_due(600).is_some());
        assert!(s.take_due(10_000).is_none());
    }

    #[test]
    fn invalid_frame_cannot_replace_pending_valid_frame() {
        let mut s = FrameScheduler::default();
        s.resume(0);
        s.submit([[1; 3]; LED_COUNT]).unwrap();
        assert!(s.submit([[161; 3]; LED_COUNT]).is_err());
        assert_eq!(s.take_due(0).unwrap()[0], [1; 3]);
    }

    #[test]
    fn brief_priority_pause_cannot_bypass_completion_interval() {
        let mut s = FrameScheduler::default();
        s.resume(0);
        s.submit([[1; 3]; LED_COUNT]).unwrap();
        assert!(s.take_due(0).is_some());
        s.completed(500);
        s.pause();
        s.resume(520);
        s.submit([[2; 3]; LED_COUNT]).unwrap();
        assert!(s.take_due(520).is_none());
        assert!(s.take_due(599).is_none());
        assert!(s.take_due(600).is_some());
    }
}
