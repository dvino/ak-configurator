//! Deterministic, bounded host-side scene composition for the AK820's 82 RGB LEDs.
//! Preset format is deliberately separate from the stable v1 static palette format.

use crate::ak820_rgb::{Palette, Rgb, LED_COUNT, MAPPING_ID, RGB_MAX_BRIGHTNESS};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_SCENE_BYTES: usize = 64 * 1024;
const MAX_LAYERS: usize = 16;
const MODEL: &str = "AK820";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Effect {
    Solid,
    Gradient,
    Wave,
    Breathing,
    Rain,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Layer {
    pub name: String,
    pub enabled: bool,
    pub mask: Vec<usize>,
    pub effect: Effect,
    pub color: Rgb,
    pub color2: Rgb,
    pub opacity: u8,
    pub period_ms: u32,
    pub axis: Axis,
}
impl Default for Layer {
    fn default() -> Self {
        Self {
            name: "Layer".into(),
            enabled: true,
            mask: (0..LED_COUNT).collect(),
            effect: Effect::Solid,
            color: Rgb([255, 255, 255]),
            color2: Rgb([0, 0, 0]),
            opacity: 255,
            period_ms: 1000,
            axis: Axis::Horizontal,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuietHours {
    pub start_minute: u16,
    pub end_minute: u16,
    pub brightness: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scene {
    pub base: Palette,
    pub layers: Vec<Layer>,
    pub quiet_hours: Option<QuietHours>,
}
impl Default for Scene {
    fn default() -> Self {
        Self {
            base: Palette::default(),
            layers: Vec::new(),
            quiet_hours: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenePreset {
    format: String,
    version: u8,
    model: String,
    mapping: String,
    base: PaletteData,
    layers: Vec<Layer>,
    quiet_hours: Option<QuietHours>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaletteData {
    colors: Vec<Rgb>,
    brightness: u8,
}
impl From<&Palette> for PaletteData {
    fn from(p: &Palette) -> Self {
        Self {
            colors: p.colors.to_vec(),
            brightness: p.brightness,
        }
    }
}
impl PaletteData {
    fn into_palette(self) -> Result<Palette, &'static str> {
        let colors: [Rgb; LED_COUNT] = self
            .colors
            .try_into()
            .map_err(|_| "base palette must contain exactly 82 colors")?;
        let p = Palette {
            colors,
            brightness: self.brightness,
        };
        p.validate()?;
        Ok(p)
    }
}

impl Scene {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        self.base.validate()?;
        if self.layers.len() > MAX_LAYERS {
            return Err("scene exceeds 16 layers");
        }
        for l in &self.layers {
            if l.name.len() > 64 || l.name.chars().any(char::is_control) {
                return Err("layer name is invalid");
            }
            if l.mask.len() > LED_COUNT {
                return Err("layer mask exceeds 82 LEDs");
            }
            let mut seen = [false; LED_COUNT];
            for &i in &l.mask {
                if i >= LED_COUNT {
                    return Err("layer mask index out of range");
                }
                if seen[i] {
                    return Err("layer mask contains duplicates");
                }
                seen[i] = true;
            }
            if !(250..=60_000).contains(&l.period_ms) {
                return Err("layer period must be 250..60000 ms");
            }
        }
        if let Some(q) = &self.quiet_hours {
            if q.start_minute >= 1440 || q.end_minute >= 1440 {
                return Err("quiet hours minute must be below 1440");
            }
            if q.start_minute == q.end_minute {
                return Err("quiet hours endpoints must differ");
            }
            if q.brightness > RGB_MAX_BRIGHTNESS {
                return Err("quiet hours brightness exceeds firmware limit");
            }
        }
        Ok(())
    }

    pub(crate) fn render(
        &self,
        elapsed_ms: u64,
        local_minute: u16,
    ) -> Result<[[u8; 3]; LED_COUNT], &'static str> {
        self.validate()?;
        if local_minute >= 1440 {
            return Err("local minute must be below 1440");
        }
        let mut frame = std::array::from_fn(|i| self.base.colors[i].0);
        for layer in &self.layers {
            if !layer.enabled {
                continue;
            }
            let p = layer.period_ms as u64;
            let t = elapsed_ms % p;
            for &i in &layer.mask {
                let (x, y) = geometry(i);
                let (axis, orth) = match layer.axis {
                    Axis::Horizontal => (normalize(x, 7, 224), normalize(y, 0, 64)),
                    Axis::Vertical => (normalize(y, 0, 64), normalize(x, 7, 224)),
                };
                let value = effect_color(layer, t, p, axis, orth);
                let alpha = u16::from(layer.opacity);
                for c in 0..3 {
                    frame[i][c] = ((u32::from(frame[i][c]) * u32::from(255 - alpha)
                        + u32::from(value[c]) * u32::from(alpha))
                        / 255) as u8;
                }
            }
        }
        let brightness = self.brightness_at(local_minute);
        Ok(frame.map(|rgb| rgb.map(|v| (u16::from(v) * u16::from(brightness) / 255) as u8)))
    }

    /// Shared cap for native scenes, CAVA bars and OpenRGB input.
    pub(crate) fn brightness_at(&self, local_minute: u16) -> u8 {
        let mut brightness = self.base.brightness;
        if let Some(q) = &self.quiet_hours {
            let active = if q.start_minute < q.end_minute {
                local_minute >= q.start_minute && local_minute < q.end_minute
            } else {
                local_minute >= q.start_minute || local_minute < q.end_minute
            };
            if active {
                brightness = brightness.min(q.brightness);
            }
        }
        brightness.min(RGB_MAX_BRIGHTNESS)
    }
    pub(crate) fn encode_preset(&self) -> Result<String, &'static str> {
        self.validate()?;
        let s = serde_json::to_string(&ScenePreset {
            format: "ak820-rgb-scene".into(),
            version: 2,
            model: MODEL.into(),
            mapping: MAPPING_ID.into(),
            base: PaletteData::from(&self.base),
            layers: self.layers.clone(),
            quiet_hours: self.quiet_hours.clone(),
        })
        .map_err(|_| "could not encode scene")?;
        if s.len() > MAX_SCENE_BYTES {
            return Err("scene preset exceeds size limit");
        }
        Ok(s)
    }
    pub(crate) fn decode_preset(text: &str) -> Result<Self, &'static str> {
        if text.len() > MAX_SCENE_BYTES {
            return Err("scene preset exceeds size limit");
        }
        let p: ScenePreset = serde_json::from_str(text).map_err(|_| "invalid scene preset")?;
        if p.format != "ak820-rgb-scene"
            || p.version != 2
            || p.model != MODEL
            || p.mapping != MAPPING_ID
        {
            return Err("preset does not match this AK820 scene format");
        }
        let scene = Self {
            base: p.base.into_palette()?,
            layers: p.layers,
            quiet_hours: p.quiet_hours,
        };
        scene.validate()?;
        Ok(scene)
    }
}

// Exact physical LED centers, aligned by index with the beta2 SLOT_MAP. Source:
// firmware v0.6.0-beta.2 keymap.c ak820_led_points[] at commit
// 2990ef877c5c6c54013b8821450c5a91878b8c3a.
pub(crate) const LED_POINTS: [(u16, u16); LED_COUNT] = [
    (7, 0),
    (25, 0),
    (39, 0),
    (53, 0),
    (68, 0),
    (85, 0),
    (100, 0),
    (114, 0),
    (128, 0),
    (146, 0),
    (160, 0),
    (174, 0),
    (188, 0),
    (203, 0),
    (7, 13),
    (21, 13),
    (36, 13),
    (50, 13),
    (64, 13),
    (78, 13),
    (92, 13),
    (107, 13),
    (121, 13),
    (135, 13),
    (149, 13),
    (164, 13),
    (178, 13),
    (199, 13),
    (224, 13),
    (11, 26),
    (28, 26),
    (43, 26),
    (57, 26),
    (71, 26),
    (85, 26),
    (100, 26),
    (114, 26),
    (128, 26),
    (142, 26),
    (156, 26),
    (171, 26),
    (185, 26),
    (203, 26),
    (224, 26),
    (12, 38),
    (32, 38),
    (46, 38),
    (60, 38),
    (75, 38),
    (89, 38),
    (103, 38),
    (117, 38),
    (132, 38),
    (146, 38),
    (160, 38),
    (174, 38),
    (197, 38),
    (224, 38),
    (16, 51),
    (39, 51),
    (53, 51),
    (68, 51),
    (82, 51),
    (96, 51),
    (110, 51),
    (124, 51),
    (139, 51),
    (153, 51),
    (167, 51),
    (187, 51),
    (210, 51),
    (224, 51),
    (9, 64),
    (27, 64),
    (44, 64),
    (98, 64),
    (149, 64),
    (164, 64),
    (178, 64),
    (196, 64),
    (210, 64),
    (224, 64),
];
fn geometry(i: usize) -> (u16, u16) {
    LED_POINTS[i]
}
fn normalize(value: u16, min: u16, max: u16) -> u32 {
    (u32::from(value.saturating_sub(min)) * 255 / u32::from(max - min)).min(255)
}
fn lerp(a: u8, b: u8, n: u32, d: u32) -> u8 {
    ((u32::from(a) * (d - n) + u32::from(b) * n) / d) as u8
}
fn effect_color(l: &Layer, t: u64, p: u64, axis: u32, orth: u32) -> [u8; 3] {
    match l.effect {
        Effect::Solid => l.color.0,
        Effect::Gradient => std::array::from_fn(|c| lerp(l.color.0[c], l.color2.0[c], axis, 255)),
        Effect::Wave => {
            // One smooth triangle crosses the selected physical axis. The 512-step
            // temporal phase closes at the period boundary without a phase jump.
            let phase = (axis + (t * 512 / p) as u32) % 512;
            let n = if phase <= 256 {
                phase.min(255)
            } else {
                (512 - phase).min(255)
            };
            std::array::from_fn(|c| lerp(l.color.0[c], l.color2.0[c], n, 255))
        }
        Effect::Breathing => {
            let u = (t * 510 / p) as u32;
            let gain = if u <= 255 { u } else { 510 - u };
            l.color.0.map(|v| (u32::from(v) * gain / 255) as u8)
        }
        Effect::Rain => {
            // Lane timing varies across the perpendicular physical axis. Each head
            // wraps continuously and leaves a short, linearly fading 48/256 trail.
            let head = ((t * 256 / p) as u32 + orth * 128 / 255) % 256;
            let behind = (head + 256 - axis) % 256;
            let amount = if behind < 48 {
                (48 - behind) * 255 / 48
            } else {
                0
            };
            std::array::from_fn(|c| lerp(l.color2.0[c], l.color.0[c], amount, 255))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composition_is_ordered_and_masked() {
        let mut s = Scene::default();
        s.base.colors[0] = Rgb([20, 0, 0]);
        s.base.brightness = 160;
        let mut a = Layer::default();
        a.color = Rgb([255, 0, 0]);
        a.mask = vec![0];
        let mut b = Layer::default();
        b.color = Rgb([0, 0, 255]);
        b.mask = vec![0];
        s.layers = vec![a, b];
        let f = s.render(0, 0).unwrap();
        assert_eq!(f[0], [0, 0, 160]);
        assert_eq!(f[1], [0, 0, 0]);
    }
    #[test]
    fn brightness_applied_once_and_quiet_hours_wrap() {
        let mut s = Scene::default();
        s.base.brightness = 128;
        s.base.colors[0] = Rgb([255; 3]);
        s.quiet_hours = Some(QuietHours {
            start_minute: 1380,
            end_minute: 360,
            brightness: 64,
        });
        assert_eq!(s.render(0, 120).unwrap()[0], [64; 3]);
        assert_eq!(s.render(0, 500).unwrap()[0], [128; 3]);
    }
    #[test]
    fn effects_progress_and_u64_extreme_is_safe() {
        let mut s = Scene::default();
        let mut l = Layer::default();
        l.effect = Effect::Breathing;
        l.color = Rgb([255; 3]);
        s.layers.push(l);
        assert_ne!(s.render(0, 0).unwrap(), s.render(500, 0).unwrap());
        let _ = s.render(u64::MAX, 1439).unwrap();
    }
    #[test]
    fn spatial_gradient_uses_physical_led_coordinates() {
        let mut s = Scene::default();
        let mut l = Layer::default();
        l.effect = Effect::Gradient;
        l.color = Rgb([0; 3]);
        l.color2 = Rgb([255; 3]);
        l.mask = vec![1, 13];
        s.layers.push(l);
        let f = s.render(0, 0).unwrap();
        assert!(f[1][0] < f[13][0]);
        assert_eq!(geometry(1), (25, 0));
        assert_eq!(geometry(13), (203, 0));
    }
    #[test]
    fn wave_is_a_smooth_spatial_phase_and_closes_its_period() {
        let mut s = Scene::default();
        let mut l = Layer::default();
        l.effect = Effect::Wave;
        l.color = Rgb([0; 3]);
        l.color2 = Rgb([255; 3]);
        s.layers.push(l);
        let start = s.render(0, 0).unwrap();
        let end = s.render(1000, 0).unwrap();
        let almost = s.render(999, 0).unwrap();
        assert_ne!(start[0], start[13]);
        assert_eq!(start, end);
        assert!(start
            .iter()
            .zip(almost.iter())
            .all(|(a, b)| a.iter().zip(b).all(|(x, y)| (*x).abs_diff(*y) <= 1)));
    }
    #[test]
    fn rain_has_staggered_orthogonal_lanes_and_repeats_deterministically() {
        let mut s = Scene::default();
        let mut l = Layer::default();
        l.effect = Effect::Rain;
        l.color = Rgb([255, 0, 0]);
        l.color2 = Rgb([0, 0, 0]);
        l.axis = Axis::Horizontal;
        s.layers.push(l);
        let start = s.render(0, 0).unwrap();
        assert_ne!(start[0], start[14]); // both x=7; y selects a different lane phase
        assert_eq!(start, s.render(1000, 0).unwrap());
        assert_ne!(start, s.render(250, 0).unwrap());
    }
    #[test]
    fn strict_bounded_preset_roundtrip() {
        let s = Scene::default();
        let encoded = s.encode_preset().unwrap();
        assert_eq!(Scene::decode_preset(&encoded).unwrap(), s);
        let extra = encoded.replacen("\"version\":2", "\"version\":2,\"oops\":0", 1);
        assert!(Scene::decode_preset(&extra).is_err());
        let duplicate = encoded.replacen("\"base\":", "\"base\":{},\"base\":", 1);
        assert!(Scene::decode_preset(&duplicate).is_err());
    }
    #[test]
    fn bounds_and_quiet_hours_invalid_values_rejected() {
        let mut s = Scene::default();
        s.layers.push(Layer::default());
        s.layers[0].mask = vec![2, 2];
        assert!(s.validate().is_err());
        s.layers[0].mask = vec![0];
        s.quiet_hours = Some(QuietHours {
            start_minute: 2,
            end_minute: 2,
            brightness: 1,
        });
        assert!(s.validate().is_err());
    }
}
