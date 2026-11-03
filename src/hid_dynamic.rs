use super::hid_parse::{
    parse_alt_repeat_response, parse_combo_response, parse_key_override_response,
    parse_tap_dance_response,
};
use super::hid_protocol::*;
use super::HidDevice;
use anyhow::Result;

type TapDanceData = (u16, u16, u16, u16, u16);
type ComboData = ([u16; 4], u16);
type DynamicEntryCounts = (u8, u8, u8, u8, u8);

/// Raw Vial key-override fields; encoding does not impose typed-storage policy.
pub(crate) struct KeyOverrideRecord {
    pub(crate) trigger: u16,
    pub(crate) replacement: u16,
    pub(crate) layers: u16,
    pub(crate) trigger_mods: u8,
    pub(crate) negative_mod_mask: u8,
    pub(crate) suppressed_mods: u8,
    pub(crate) options: u8,
}

fn parse_dynamic_entry_counts_response(
    command: &[u8],
    resp: &[u8; MSG_LEN],
) -> Result<DynamicEntryCounts> {
    // Some QMK-Vial builds echo an unsupported dynamic-entry query, while
    // others replace only its command byte with the generic 0xFF marker.
    // Without this check, either reply is misread as hundreds of entries.
    if matches!(resp[0], CMD_VIA_VIAL_PREFIX | u8::MAX)
        && super::response_echoes_vial_command(command, resp)
    {
        anyhow::bail!("Vial dynamic entries are unsupported");
    }

    // Vial GUI trusts the firmware-provided one-byte dynamic entry counts.
    // Do not impose Entropy-only caps here: the firmware/Vial protocol is authoritative.
    Ok((resp[0], resp[1], resp[2], resp[3], resp[31]))
}

fn verify_combo_writeback(index: u8, requested: ComboData, readback: ComboData) -> Result<()> {
    if readback == requested {
        return Ok(());
    }

    anyhow::bail!(
        "combo writeback mismatch at index {}: wrote {:?}, read back {:?}",
        index,
        requested,
        readback
    )
}

fn verify_tap_dance_writeback(
    index: u8,
    requested: TapDanceData,
    readback: TapDanceData,
) -> Result<()> {
    if readback == requested {
        return Ok(());
    }

    anyhow::bail!(
        "tap dance writeback mismatch at index {}: wrote {:?}, read back {:?}",
        index,
        requested,
        readback
    )
}

#[cfg(not(target_arch = "wasm32"))]
impl HidDevice {
    /// Get Vial dynamic entry counts and optional feature bits.
    /// Returns (tap_dance, combo, key_override, alt_repeat, feature_bits).
    pub fn get_dynamic_entry_counts(&self) -> Result<(u8, u8, u8, u8, u8)> {
        let command = [
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_GET_NUM_ENTRIES,
        ];
        let resp = self.usb_send(&command)?;
        parse_dynamic_entry_counts_response(&command, &resp)
    }

    /// Get number of combo entries available
    #[cfg(test)]
    pub fn get_combo_count(&self) -> Result<u8> {
        let (_, combo, _, _, _) = self.get_dynamic_entry_counts()?;
        Ok(combo)
    }

    /// Get combo entry: ([trigger_keys; 4], output_keycode)
    pub fn get_combo(&self, idx: u8) -> Result<([u16; 4], u16)> {
        let resp = self.usb_send(&[
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_COMBO_GET,
            idx,
        ])?;
        parse_combo_response(&resp)
    }

    /// Set combo entry
    pub fn set_combo(&self, idx: u8, keys: [u16; 4], output: u16) -> Result<()> {
        let requested = (keys, output);
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_DYNAMIC_ENTRY_OP;
        cmd[2] = DYNAMIC_VIAL_COMBO_SET;
        cmd[3] = idx;
        for (i, key) in keys.iter().enumerate() {
            let [lo, hi] = key.to_le_bytes();
            let off = 4 + i * 2;
            cmd[off] = lo;
            cmd[off + 1] = hi;
        }
        let [out_lo, out_hi] = output.to_le_bytes();
        cmd[12] = out_lo;
        cmd[13] = out_hi;
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("combo set error: {}", resp[0]);
        }
        if self.is_ak820_session() {
            return Ok(());
        }
        let readback = self.get_combo(idx)?;
        verify_combo_writeback(idx, requested, readback)
    }

    pub fn set_combo_binding(
        &self,
        idx: u8,
        keys: [u16; 4],
        output: crate::keyboard::KeyBinding,
    ) -> Result<()> {
        self.set_combo(idx, keys, output.vial_keycode())?;
        if let Some(action) = output.rmk_action() {
            self.set_rmk_combo_output(idx, action)?;
        }
        Ok(())
    }

    /// Get number of key override entries available
    #[cfg(test)]
    pub fn get_key_override_count(&self) -> Result<u8> {
        let (_, _, key_override, _, _) = self.get_dynamic_entry_counts()?;
        Ok(key_override)
    }

    /// Get number of alt repeat key entries available
    #[cfg(test)]
    pub fn get_alt_repeat_key_count(&self) -> Result<u8> {
        let (_, _, _, alt_repeat, _) = self.get_dynamic_entry_counts()?;
        Ok(alt_repeat)
    }

    /// Get key override entry:
    /// (trigger, replacement, layers, trigger_mods, negative_mod_mask, suppressed_mods, options)
    pub fn get_key_override(&self, idx: u8) -> Result<(u16, u16, u16, u8, u8, u8, u8)> {
        let resp = self.usb_send(&[
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_KEY_OVERRIDE_GET,
            idx,
        ])?;
        parse_key_override_response(&resp)
    }

    /// Set key override entry
    pub fn set_key_override(&self, idx: u8, record: KeyOverrideRecord) -> Result<()> {
        let KeyOverrideRecord {
            trigger,
            replacement,
            layers,
            trigger_mods,
            negative_mod_mask,
            suppressed_mods,
            options,
        } = record;
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_DYNAMIC_ENTRY_OP;
        cmd[2] = DYNAMIC_VIAL_KEY_OVERRIDE_SET;
        cmd[3] = idx;
        cmd[4..6].copy_from_slice(&trigger.to_le_bytes());
        cmd[6..8].copy_from_slice(&replacement.to_le_bytes());
        cmd[8..10].copy_from_slice(&layers.to_le_bytes());
        cmd[10] = trigger_mods;
        cmd[11] = negative_mod_mask;
        cmd[12] = suppressed_mods;
        cmd[13] = options;
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("key override set error: {}", resp[0]);
        }
        Ok(())
    }

    /// Get alt repeat key entry: (last_key, alt_key, allowed_mods, options)
    pub fn get_alt_repeat_key(&self, idx: u8) -> Result<(u16, u16, u8, u8)> {
        let resp = self.usb_send(&[
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_ALT_REPEAT_KEY_GET,
            idx,
        ])?;
        parse_alt_repeat_response(&resp)
    }

    /// Set alt repeat key entry
    pub fn set_alt_repeat_key(
        &self,
        idx: u8,
        keycode: u16,
        alt_keycode: u16,
        allowed_mods: u8,
        options: u8,
    ) -> Result<()> {
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_DYNAMIC_ENTRY_OP;
        cmd[2] = DYNAMIC_VIAL_ALT_REPEAT_KEY_SET;
        cmd[3] = idx;
        cmd[4..6].copy_from_slice(&keycode.to_le_bytes());
        cmd[6..8].copy_from_slice(&alt_keycode.to_le_bytes());
        cmd[8] = allowed_mods;
        cmd[9] = options;
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("alt repeat key set error: {}", resp[0]);
        }
        Ok(())
    }

    /// Get number of tap dance entries available
    #[cfg(test)]
    pub fn get_tap_dance_count(&self) -> Result<u8> {
        let (tap_dance, _, _, _, _) = self.get_dynamic_entry_counts()?;
        Ok(tap_dance)
    }

    /// Get a tap dance entry: (on_tap, on_hold, on_double_tap, on_tap_hold, tapping_term)
    pub fn get_tap_dance(&self, idx: u8) -> Result<TapDanceData> {
        let resp = self.usb_send(&[
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_TAP_DANCE_GET,
            idx,
        ])?;
        // resp[0] = status (0=ok), resp[1..] = entry data
        parse_tap_dance_response(&resp)
    }

    /// Set a tap dance entry
    pub fn set_tap_dance(
        &self,
        idx: u8,
        on_tap: u16,
        on_hold: u16,
        on_double_tap: u16,
        on_tap_hold: u16,
        tapping_term: u16,
    ) -> Result<()> {
        let requested = (on_tap, on_hold, on_double_tap, on_tap_hold, tapping_term);
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_DYNAMIC_ENTRY_OP;
        cmd[2] = DYNAMIC_VIAL_TAP_DANCE_SET;
        cmd[3] = idx;
        cmd[4..6].copy_from_slice(&on_tap.to_le_bytes());
        cmd[6..8].copy_from_slice(&on_hold.to_le_bytes());
        cmd[8..10].copy_from_slice(&on_double_tap.to_le_bytes());
        cmd[10..12].copy_from_slice(&on_tap_hold.to_le_bytes());
        cmd[12..14].copy_from_slice(&tapping_term.to_le_bytes());
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("tap dance set error: {}", resp[0]);
        }
        if self.is_ak820_session() {
            // The AK820 transaction checked the getter and complete NVM while
            // holding its session lock. Do not add an unlatched extra read.
            return Ok(());
        }
        let readback = self.get_tap_dance(idx)?;
        verify_tap_dance_writeback(idx, requested, readback)
    }

    pub fn set_tap_dance_bindings(
        &self,
        idx: u8,
        actions: [crate::keyboard::KeyBinding; 4],
        tapping_term: u16,
    ) -> Result<()> {
        self.set_tap_dance(
            idx,
            actions[0].vial_keycode(),
            actions[1].vial_keycode(),
            actions[2].vial_keycode(),
            actions[3].vial_keycode(),
            tapping_term,
        )?;
        for (field, binding) in actions.into_iter().enumerate() {
            if let Some(action) = binding.rmk_action() {
                self.set_rmk_tap_dance_action(idx, field as u8, action)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_count_wrappers_request_fresh_authoritative_fields() {
        let (hid, log) = HidDevice::test_device();
        let responses = [
            [10, 255, 12, 13],
            [20, 21, 0, 23],
            [30, 31, 32, 250],
            [200, 41, 42, 43],
        ]
        .map(|fields| {
            let mut response = [0; MSG_LEN];
            response[..4].copy_from_slice(&fields);
            response[31] = 0xA5;
            response
        });
        log.respond_with(responses);
        assert_eq!(hid.get_combo_count().unwrap(), 255);
        assert_eq!(hid.get_key_override_count().unwrap(), 0);
        assert_eq!(hid.get_alt_repeat_key_count().unwrap(), 250);
        assert_eq!(hid.get_tap_dance_count().unwrap(), 200);
        let mut expected = [0; MSG_LEN];
        expected[..3].copy_from_slice(&[0xFE, 0x0D, 0]);
        assert_eq!(log.requests(), vec![expected; 4]);
    }

    #[test]
    fn reference_count_wrappers_propagate_unsupported_and_transport_errors() {
        let getters: [fn(&HidDevice) -> Result<u8>; 4] = [
            HidDevice::get_combo_count,
            HidDevice::get_key_override_count,
            HidDevice::get_alt_repeat_key_count,
            HidDevice::get_tap_dance_count,
        ];
        for get in getters {
            for marker in [0xFE, 0xFF] {
                let (hid, log) = HidDevice::test_device();
                let mut response = [0; MSG_LEN];
                response[..3].copy_from_slice(&[marker, 0x0D, 0]);
                log.respond_with([response]);
                assert_eq!(
                    get(&hid).unwrap_err().to_string(),
                    "Vial dynamic entries are unsupported"
                );
                assert_eq!(log.requests().len(), 1);
            }
            let (hid, log) = HidDevice::test_device();
            log.respond_by(|_| Some(Err("scripted count transport fault".into())));
            let error = get(&hid).unwrap_err();
            assert!(format!("{error:#}").contains("scripted count transport fault"));
            assert_eq!(log.requests().len(), 1);
        }
    }

    #[test]
    fn key_override_wire_record_preserves_all_bits_and_padding() {
        let (hid, log) = HidDevice::test_device();
        log.respond_with([[0; MSG_LEN]; 2]);
        hid.set_key_override(
            0xAB,
            KeyOverrideRecord {
                trigger: 0x1234,
                replacement: 0xABCD,
                layers: 0xFFFF,
                trigger_mods: 0x96,
                negative_mod_mask: 0x69,
                suppressed_mods: 0xA5,
                options: 0xFF,
            },
        )
        .unwrap();
        hid.set_key_override(
            0,
            KeyOverrideRecord {
                trigger: 0,
                replacement: 0,
                layers: 0,
                trigger_mods: 0,
                negative_mod_mask: 0,
                suppressed_mods: 0,
                options: 0,
            },
        )
        .unwrap();
        let mut expected_bits = [0; MSG_LEN];
        expected_bits[..14].copy_from_slice(&[
            0xFE, 0x0D, 0x06, 0xAB, 0x34, 0x12, 0xCD, 0xAB, 0xFF, 0xFF, 0x96, 0x69, 0xA5, 0xFF,
        ]);
        let mut expected_zero = [0; MSG_LEN];
        expected_zero[..3].copy_from_slice(&[0xFE, 0x0D, 0x06]);
        assert_eq!(log.requests(), vec![expected_bits, expected_zero]);
    }

    #[test]
    fn key_override_wire_record_preserves_device_error_without_retry() {
        let (hid, log) = HidDevice::test_device();
        log.respond_with([[7; MSG_LEN]]);
        let error = hid
            .set_key_override(
                0,
                KeyOverrideRecord {
                    trigger: 4,
                    replacement: 5,
                    layers: 1,
                    trigger_mods: 0,
                    negative_mod_mask: 0,
                    suppressed_mods: 0,
                    options: 1,
                },
            )
            .unwrap_err();
        assert_eq!(error.to_string(), "key override set error: 7");
        assert_eq!(log.requests().len(), 1);
    }

    #[test]
    fn key_override_wire_record_preserves_transport_error_without_retry() {
        let (hid, log) = HidDevice::test_device();
        log.respond_by(|_| Some(Err("scripted key-override transport fault".into())));
        let error = hid
            .set_key_override(
                0,
                KeyOverrideRecord {
                    trigger: 4,
                    replacement: 5,
                    layers: 1,
                    trigger_mods: 0,
                    negative_mod_mask: 0,
                    suppressed_mods: 0,
                    options: 1,
                },
            )
            .unwrap_err();
        assert!(format!("{error:#}").contains("scripted key-override transport fault"));
        assert_eq!(log.requests().len(), 1);
    }

    #[test]
    fn dynamic_entry_counts_reject_an_unsupported_qmk_echo() {
        let command = [
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_GET_NUM_ENTRIES,
        ];
        let mut response = [0u8; MSG_LEN];
        response[..command.len()].copy_from_slice(&command);

        let error = parse_dynamic_entry_counts_response(&command, &response).unwrap_err();
        assert!(error.to_string().contains("unsupported"));

        response[0] = u8::MAX;
        let error = parse_dynamic_entry_counts_response(&command, &response).unwrap_err();
        assert!(error.to_string().contains("unsupported"));
    }

    #[test]
    fn dynamic_entry_counts_keep_firmware_authoritative_without_local_caps() {
        let command = [
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_GET_NUM_ENTRIES,
        ];
        let mut response = [0u8; MSG_LEN];
        response[..4].copy_from_slice(&[200, 201, 202, 203]);
        response[31] = 3;

        assert_eq!(
            parse_dynamic_entry_counts_response(&command, &response).unwrap(),
            (200, 201, 202, 203, 3)
        );

        // A real count payload can share the echoed subcommand bytes. It is
        // not an unsupported marker unless byte zero is 0xFE or 0xFF.
        let mut response = [0u8; MSG_LEN];
        response[0] = 8;
        response[1] = CMD_VIAL_DYNAMIC_ENTRY_OP;
        assert_eq!(
            parse_dynamic_entry_counts_response(&command, &response).unwrap(),
            (8, CMD_VIAL_DYNAMIC_ENTRY_OP, 0, 0, 0)
        );
    }

    #[test]
    fn tap_dance_writeback_accepts_matching_entry() {
        let entry = (0x002C, 0x0202, 0, 0, 150);

        assert!(verify_tap_dance_writeback(0, entry, entry).is_ok());
    }

    #[test]
    fn tap_dance_writeback_rejects_stale_entry() {
        let requested = (0x002C, 0x0202, 0, 0, 150);
        let stale = (0, 0, 0, 0, 200);

        let error = verify_tap_dance_writeback(0, requested, stale).unwrap_err();

        assert!(error.to_string().contains("tap dance writeback mismatch"));
        assert!(error.to_string().contains("index 0"));
    }

    #[test]
    fn combo_writeback_accepts_matching_entry() {
        let entry = ([0x002C, 0x002D, 0, 0], 0x002E);

        assert!(verify_combo_writeback(3, entry, entry).is_ok());
    }

    #[test]
    fn combo_writeback_rejects_stale_entry() {
        let requested = ([0x002C, 0x002D, 0, 0], 0x002E);
        let stale = ([0x002C, 0x002D, 0, 0], 0);

        let error = verify_combo_writeback(3, requested, stale).unwrap_err();

        assert!(error.to_string().contains("combo writeback mismatch"));
        assert!(error.to_string().contains("index 3"));
    }
}
