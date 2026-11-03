use super::*;

// Python stdlib lzma vectors for generic Vial JSON, not an admitted firmware image.
const XZ: &[u8] = b"\xfd\x37\x7a\x58\x5a\x00\x00\x04\xe6\xd6\xb4\x46\x02\x00\x21\x01\x16\x00\x00\x00\x74\x2f\xe5\xa3\x01\x00\x4d\x7b\x22\x6b\x65\x79\x62\x6f\x61\x72\x64\x22\x3a\x22\x6f\x66\x66\x6c\x69\x6e\x65\x2d\x72\x65\x66\x65\x72\x65\x6e\x63\x65\x22\x2c\x22\x6d\x61\x74\x72\x69\x78\x22\x3a\x7b\x22\x72\x6f\x77\x73\x22\x3a\x31\x2c\x22\x63\x6f\x6c\x73\x22\x3a\x32\x7d\x2c\x22\x6b\x65\x79\x6d\x61\x70\x22\x3a\x5b\x5b\x30\x2c\x31\x5d\x5d\x7d\x00\x00\x00\x3a\x43\x4f\xfe\x84\x0a\x00\xe1\x00\x01\x66\x4e\x9d\x15\x45\x82\x1f\xb6\xf3\x7d\x01\x00\x00\x00\x00\x04\x59\x5a";
const LZMA: &[u8] = b"\x5d\x00\x00\x80\x00\xff\xff\xff\xff\xff\xff\xff\xff\x00\x3d\x88\x89\x66\x94\x90\x74\xa7\x76\x89\xc1\xd4\xca\x61\xf1\x11\xc5\xe5\x16\x87\xde\x1a\x72\x48\x28\x52\xca\xfa\xc0\xe4\xac\xf8\x16\xa4\x20\x12\xce\xb8\x5f\x03\x10\xab\x1b\xcb\x2f\x6a\x9e\xa8\xf5\xc2\x93\x27\xa4\x0c\xd1\xfd\x75\x18\x73\xb6\xd0\x15\xa7\xfa\x62\x1b\x93\x26\x6e\x25\x94\x2b\x26\x4d\xfa\xbd\x90\xdb\xff\xf9\x0e\x11\x00";

#[test]
fn reference_layout_wrappers_decode_independent_xz_and_lzma_vectors() {
    for payload in [XZ, LZMA] {
        for read_size in [false, true] {
            let (hid, log) = HidDevice::test_device();
            let mut responses = Vec::new();
            let mut expected = Vec::new();
            if read_size {
                let mut size = [0; MSG_LEN];
                size[..4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
                responses.push(size);
                let mut request = [0; MSG_LEN];
                request[..2].copy_from_slice(&[0xFE, 0x01]);
                expected.push(request);
            }
            for (block, chunk) in payload.chunks(MSG_LEN).enumerate() {
                let mut response = [0xA5; MSG_LEN];
                response[..chunk.len()].copy_from_slice(chunk);
                responses.push(response);
                let mut request = [0; MSG_LEN];
                request[..2].copy_from_slice(&[0xFE, 0x02]);
                request[2..6].copy_from_slice(&(block as u32).to_le_bytes());
                expected.push(request);
            }
            log.respond_with(responses);
            let value = if read_size {
                hid.get_layout_json()
            } else {
                hid.get_layout_json_with_size(payload.len() as u32)
            }
            .unwrap();
            assert_eq!(
                value,
                serde_json::json!({
                    "keyboard": "offline-reference",
                    "matrix": {"rows": 1, "cols": 2},
                    "keymap": [[0, 1]],
                })
            );
            assert_eq!(log.requests(), expected);
        }
    }
}

#[test]
fn reference_layout_wrappers_reject_invalid_sizes_before_payload_io() {
    for size in [0, 2_000_001, u32::MAX] {
        for read_size in [false, true] {
            let (hid, log) = HidDevice::test_device();
            if read_size {
                let mut response = [0; MSG_LEN];
                response[..4].copy_from_slice(&size.to_le_bytes());
                log.respond_with([response]);
            }
            let error = if read_size {
                hid.get_layout_json()
            } else {
                hid.get_layout_json_with_size(size)
            }
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("Invalid definition size: {size}")
            );
            let expected = if read_size {
                let mut request = [0; MSG_LEN];
                request[..2].copy_from_slice(&[0xFE, 0x01]);
                vec![request]
            } else {
                Vec::new()
            };
            assert_eq!(log.requests(), expected);
        }
    }
}

#[test]
fn reference_layout_wrappers_stop_on_first_block_error_at_valid_bounds() {
    for size in [1u32, 2_000_000] {
        for read_size in [false, true] {
            let (hid, log) = HidDevice::test_device();
            if read_size {
                let mut response = [0; MSG_LEN];
                response[..4].copy_from_slice(&size.to_le_bytes());
                log.respond_with([response]);
            }
            log.respond_by(|request| {
                (request[..2] == [0xFE, 0x02])
                    .then(|| Err("scripted definition block fault".into()))
            });
            let error = if read_size {
                hid.get_layout_json()
            } else {
                hid.get_layout_json_with_size(size)
            }
            .unwrap_err();
            assert_eq!(error.to_string(), "failed to read Vial definition block 0");
            assert!(format!("{error:#}").contains("scripted definition block fault"));
            let mut expected = Vec::new();
            if read_size {
                let mut request = [0; MSG_LEN];
                request[..2].copy_from_slice(&[0xFE, 0x01]);
                expected.push(request);
            }
            let mut request = [0; MSG_LEN];
            request[..2].copy_from_slice(&[0xFE, 0x02]);
            expected.push(request);
            assert_eq!(log.requests(), expected);
        }
    }
}
