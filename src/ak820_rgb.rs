//! Bounded static palette model for the AK820 beta.2 Direct RGB feature.
//!
//! The 82 physical LED slot addresses below are copied from
//! `ak820-firmware/qmk-8051-ak820/keyboards/ajazz/ak820/config.h`,
//! `AK820_RGB_SLOT_MAP`, at firmware v0.6.0-beta.2. The encoder press key
//! intentionally has no LED. Values remain an application-local preview until
//! explicitly applied; firmware Direct mode has no palette readback/persistence.

use serde::{Deserialize, Serialize};

pub(crate) const LED_COUNT: usize = 82;
pub(crate) const RGB_MAX_BRIGHTNESS: u8 = 160;
pub(crate) const MAPPING_ID: &str = "ak820-beta2-slot-map-v1";
const MAX_PRESET_BYTES: usize = 16 * 1024;
pub(crate) const SLOT_MAP: [u8; LED_COUNT] = [
    0, 8, 16, 24, 32, 40, 48, 56, 64, 72, 80, 88, 96, 114, 1, 9, 17, 25, 33, 41, 49, 57, 65, 73,
    81, 89, 97, 105, 121, 2, 10, 18, 26, 34, 42, 50, 58, 66, 74, 82, 90, 98, 106, 122, 3, 11, 19,
    27, 35, 43, 51, 59, 67, 75, 83, 91, 107, 115, 4, 20, 28, 36, 44, 52, 60, 68, 76, 84, 92, 108,
    116, 123, 5, 13, 21, 45, 77, 85, 93, 109, 117, 125,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Rgb(pub [u8; 3]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Palette {
    pub colors: [Rgb; LED_COUNT],
    pub brightness: u8,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            colors: [Rgb([0, 0, 0]); LED_COUNT],
            brightness: RGB_MAX_BRIGHTNESS,
        }
    }
}

impl Palette {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.brightness > RGB_MAX_BRIGHTNESS {
            return Err("brightness exceeds firmware limit");
        }
        Ok(())
    }
    pub(crate) fn rgb_frame(&self) -> Result<[[u8; 3]; LED_COUNT], &'static str> {
        self.validate()?;
        Ok(std::array::from_fn(|i| {
            self.colors[i]
                .0
                .map(|c| (u16::from(c) * u16::from(self.brightness) / 255) as u8)
        }))
    }
    pub(crate) fn encode_preset(&self) -> Result<String, &'static str> {
        self.validate()?;
        serde_json::to_string(&Preset {
            format: "ak820-rgb-palette".into(),
            version: 1,
            model: "AK820".into(),
            mapping: MAPPING_ID.into(),
            count: LED_COUNT as u8,
            brightness: self.brightness,
            colors: self.colors.to_vec(),
        })
        .map_err(|_| "could not encode preset")
    }
    pub(crate) fn decode_preset(text: &str) -> Result<Self, &'static str> {
        if text.len() > MAX_PRESET_BYTES {
            return Err("palette preset exceeds 16 KiB");
        }
        let p: Preset = serde_json::from_str(text).map_err(|_| "invalid palette preset")?;
        if p.format != "ak820-rgb-palette"
            || p.version != 1
            || p.model != "AK820"
            || p.mapping != MAPPING_ID
            || usize::from(p.count) != LED_COUNT
        {
            return Err("preset does not match this AK820 palette format");
        }
        let colors: [Rgb; LED_COUNT] = p
            .colors
            .try_into()
            .map_err(|_| "preset must contain exactly 82 colors")?;
        let next = Self {
            colors,
            brightness: p.brightness,
        };
        next.validate()?;
        Ok(next)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preset {
    format: String,
    version: u8,
    model: String,
    mapping: String,
    count: u8,
    brightness: u8,
    colors: Vec<Rgb>,
}

/// Build deterministic VialRGB Direct fast-set chunks: each payload carries at most 9 HSV triplets.
/// HSV conversion uses integer arithmetic and brightness has already been capped by `Palette`.
pub(crate) fn direct_packets(palette: &Palette) -> Result<Vec<Vec<u8>>, &'static str> {
    palette.validate()?;
    let rgb = palette.rgb_frame()?;
    frame_packets(&rgb)
}

/// Build Direct fast-set chunks from a brightness-scaled RGB frame.
pub(crate) fn frame_packets(frame: &[[u8; 3]; LED_COUNT]) -> Result<Vec<Vec<u8>>, &'static str> {
    if frame
        .iter()
        .flatten()
        .any(|&channel| channel > RGB_MAX_BRIGHTNESS)
    {
        return Err("RGB channel exceeds firmware limit");
    }
    let mut packets = Vec::new();
    for start in (0..LED_COUNT).step_by(9) {
        let count = (LED_COUNT - start).min(9);
        let mut packet = vec![0x07, 0x42, start as u8, 0, count as u8];
        for color in &frame[start..start + count] {
            packet.extend(rgb_to_hsv(*color));
        }
        packets.push(packet);
    }
    Ok(packets)
}

fn rgb_to_hsv([r, g, b]: [u8; 3]) -> [u8; 3] {
    let (r, g, b) = (u16::from(r), u16::from(g), u16::from(b));
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    if max == 0 {
        return [0, 0, 0];
    }
    let hue = if d == 0 {
        0i32
    } else if max == r {
        60 * (g as i32 - b as i32) / d as i32
    } else if max == g {
        120 + 60 * (b as i32 - r as i32) / d as i32
    } else {
        240 + 60 * (r as i32 - g as i32) / d as i32
    };
    [
        ((hue.rem_euclid(360) * 255) / 360) as u8,
        ((d * 255) / max) as u8,
        max as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn map_is_unique_and_exactly_eighty_two_slots() {
        let mut m = SLOT_MAP.to_vec();
        m.sort();
        m.dedup();
        assert_eq!(m.len(), LED_COUNT);
        assert!(!m.contains(&104));
    }
    #[test]
    fn packets_cover_all_leds_with_firmware_bounds() {
        let p = direct_packets(&Palette::default()).unwrap();
        assert_eq!(p.len(), 10);
        assert_eq!(p[0].len(), 32);
        assert_eq!(p[9].len(), 8);
        assert_eq!(&p[0][..5], &[7, 0x42, 0, 0, 9]);
        assert_eq!(&p[9][..5], &[7, 0x42, 81, 0, 1]);
    }
    #[test]
    fn strict_preset_round_trip_and_rejects_bad_brightness() {
        let p = Palette::default();
        assert_eq!(
            Palette::decode_preset(&p.encode_preset().unwrap()).unwrap(),
            p
        );
        let mut bad = p.clone();
        bad.brightness = 161;
        assert!(bad.encode_preset().is_err());
    }
    #[test]
    fn brightness_scales_channels_within_firmware_bound() {
        let mut p = Palette::default();
        p.colors[0] = Rgb([255, 255, 255]);
        assert_eq!(p.rgb_frame().unwrap()[0], [160, 160, 160]);
        p.colors[1] = Rgb([255, 0, 0]);
        assert_eq!(p.rgb_frame().unwrap()[1], [160, 0, 0]);
        p.brightness = 0;
        assert_eq!(p.rgb_frame().unwrap()[0], [0, 0, 0]);
    }
    #[test]
    fn frame_packets_rejects_channels_over_limit() {
        let mut frame = [[0; 3]; LED_COUNT];
        frame[12][1] = 161;
        assert!(frame_packets(&frame).is_err());
    }
    #[test]
    fn preset_schema_rejects_wrong_shape_and_extra_or_duplicate_fields() {
        let valid = Palette::default().encode_preset().unwrap();
        let value: serde_json::Value = serde_json::from_str(&valid).unwrap();
        let mut short = value.clone();
        let _ = short["colors"].as_array_mut().unwrap().pop();
        assert!(Palette::decode_preset(&short.to_string()).is_err());
        let mut extra = value.clone();
        extra["extra"] = serde_json::json!(true);
        assert!(Palette::decode_preset(&extra.to_string()).is_err());
        let duplicate = valid.replacen(
            "\"brightness\":160",
            "\"brightness\":160,\"brightness\":160",
            1,
        );
        assert!(Palette::decode_preset(&duplicate).is_err());
        assert!(Palette::decode_preset(&" ".repeat(MAX_PRESET_BYTES + 1)).is_err());
    }
}
