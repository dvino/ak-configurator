//! FD02 v1 is a volatile lease, independent of saved keyboard settings.
use serde::{Deserialize, Serialize};

pub(crate) const REFRESH_MS: u64 = 1000;
/// Host layout is sampled frequently; the firmware lease is still renewed at REFRESH_MS.
pub(crate) const SAMPLE_INTERVAL_MS: u64 = 100;

pub(crate) fn should_send(mode: Mode, previous: Option<Mode>, refresh_due: bool) -> bool {
    mode != previous.unwrap_or(Mode::Normal) || (mode != Mode::Normal && refresh_due)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    #[default]
    Normal,
    Off,
    On,
    Breathing,
}

impl Mode {
    pub(crate) fn wire(self) -> u8 {
        self as u8
    }
}

/// Stored only in application preferences. Missing/unknown layouts release
/// the override; several detected layouts may intentionally share one mode.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Settings {
    pub(crate) enabled: bool,
    pub(crate) layouts: std::collections::BTreeMap<String, Mode>,
}

impl Settings {
    pub(crate) fn mode(&self, layout: Option<&str>) -> Mode {
        if !self.enabled {
            return Mode::Normal;
        }
        layout
            .and_then(|l| self.layouts.get(l))
            .copied()
            .unwrap_or_default()
    }
}

pub(crate) fn request(mode: Mode) -> [u8; 32] {
    let mut p = [0; 32];
    p[..5].copy_from_slice(&[0xfd, 2, 1, 1, mode.wire()]);
    p
}

pub(crate) fn validate_reply(mode: Mode, reply: &[u8; 32]) -> Result<(), &'static str> {
    if reply[..10] != [0xfd, 2, 1, 0, mode.wire(), 0xa8, 0xb8, 0x0b, 0, 0x10]
        || reply[10..].iter().any(|b| *b != 0)
    {
        return Err("AK820 OS indicator reply differs from FD02 v1 contract");
    }
    Ok(())
}

/// Single outstanding exchange. Failure latches until a new verified session;
/// pausing drops the lease locally and never queues a stale refresh.
#[derive(Default)]
pub(crate) struct Scheduler {
    session: Option<u64>,
    last: Option<(u64, Mode)>,
    failed: bool,
}

impl Scheduler {
    pub(crate) fn failed(&self) -> bool {
        self.failed
    }

    pub(crate) fn bind(&mut self, generation: u64) {
        if self.session != Some(generation) {
            *self = Self {
                session: Some(generation),
                ..Self::default()
            };
        }
    }
    pub(crate) fn pause(&mut self) {
        self.last = None;
    }
    pub(crate) fn due(&self, now: u64, mode: Mode) -> bool {
        !self.failed
            && match self.last {
                None => mode != Mode::Normal,
                Some((at, previous)) => {
                    previous != mode
                        || (mode != Mode::Normal && now.saturating_sub(at) >= REFRESH_MS)
                }
            }
    }
    pub(crate) fn completed(&mut self, now: u64, mode: Mode, success: bool) {
        self.failed |= !success;
        if success {
            self.last = Some((now, mode));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_wire_contract_rejects_every_changed_reply_byte() {
        for mode in [Mode::Normal, Mode::Off, Mode::On, Mode::Breathing] {
            let p = request(mode);
            assert_eq!(&p[..5], &[0xfd, 2, 1, 1, mode.wire()]);
            assert!(p[5..].iter().all(|b| *b == 0));
            let mut reply = [0; 32];
            reply[..10].copy_from_slice(&[0xfd, 2, 1, 0, mode.wire(), 0xa8, 0xb8, 0x0b, 0, 0x10]);
            assert!(validate_reply(mode, &reply).is_ok());
            for i in 0..32 {
                let mut bad = reply;
                bad[i] ^= 1;
                assert!(validate_reply(mode, &bad).is_err());
            }
        }
    }
    #[test]
    fn heartbeat_stops_on_failure_until_new_identity_generation() {
        let mut s = Scheduler::default();
        s.bind(1);
        assert!(!s.due(0, Mode::Normal));
        assert!(s.due(0, Mode::Off));
        s.completed(0, Mode::Off, true);
        assert!(!s.due(999, Mode::Off));
        assert!(s.due(1000, Mode::Off));
        assert!(s.due(1, Mode::Normal));
        s.completed(1, Mode::Normal, false);
        s.pause();
        s.bind(1);
        assert!(!s.due(10000, Mode::On));
        s.bind(2);
        assert!(s.due(10000, Mode::On));
    }
    #[test]
    fn layout_transition_is_sampled_promptly_without_extra_same_mode_writes() {
        assert!(SAMPLE_INTERVAL_MS <= 100);
        assert!(SAMPLE_INTERVAL_MS * 2 < 2_000);
        assert!(should_send(Mode::Off, Some(Mode::On), false));
        assert!(should_send(Mode::On, Some(Mode::Off), false));
        assert!(!should_send(Mode::Off, Some(Mode::Off), false));
        assert!(should_send(Mode::Off, Some(Mode::Off), true));
        assert!(!should_send(Mode::Normal, Some(Mode::Normal), true));

        let mut scheduler = Scheduler::default();
        scheduler.bind(9);
        scheduler.completed(0, Mode::Off, true);
        assert!(!scheduler.due(SAMPLE_INTERVAL_MS, Mode::Off));
        assert!(scheduler.due(REFRESH_MS, Mode::Off));
    }
    #[test]
    fn unknown_disabled_and_shared_layouts_have_explicit_policy() {
        let mut s = Settings::default();
        s.layouts.insert("en".into(), Mode::Off);
        s.layouts.insert("ru".into(), Mode::Off);
        assert_eq!(s.mode(Some("en")), Mode::Normal);
        s.enabled = true;
        assert_eq!(s.mode(Some("ru")), Mode::Off);
        assert_eq!(s.mode(Some("fr")), Mode::Normal);
        assert_eq!(s.mode(None), Mode::Normal);
        let round: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(round.mode(Some("en")), Mode::Off);
    }
}
