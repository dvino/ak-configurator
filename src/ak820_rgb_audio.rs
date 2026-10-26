//! CAVA integration, not an audio analysis engine.
//! The bundled MIT-licensed CAVA owns capture, FFT, banding and smoothing.
//! Only bounded bar values cross its stdout pipe; raw audio is never retained.
use crate::ak820_rgb::{LED_COUNT, RGB_MAX_BRIGHTNESS};
use crate::ak820_rgb_compositor::LED_POINTS;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(crate) const BAR_COUNT: usize = 16;
const MAX_LINE: usize = 128;
const MAX_DIAGNOSTIC: usize = 2048;
const START_TIMEOUT: Duration = Duration::from_secs(10);
const FRAME_TIMEOUT: Duration = Duration::from_secs(3);
pub(crate) type Bars = [u8; BAR_COUNT];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Display {
    #[default]
    Spectrum,
    Pulse,
    Bass,
}

#[derive(Clone, Debug)]
pub(crate) struct Options {
    pub automatic_gain: bool,
    pub sensitivity: u16,
    pub smoothing: u8,
    pub display: Display,
    pub color: [u8; 3],
}

impl Default for Options {
    fn default() -> Self {
        Self {
            automatic_gain: true,
            sensitivity: 100,
            smoothing: 77,
            display: Display::Spectrum,
            color: [50, 140, 255],
        }
    }
}

impl Options {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !(1..=300).contains(&self.sensitivity) || self.smoothing > 95 {
            return Err("Invalid CAVA sensitivity or smoothing");
        }
        Ok(())
    }

    fn config(&self) -> Result<String, &'static str> {
        self.validate()?;
        #[cfg(target_os = "linux")]
        let input = "method = pulse\nsource = auto\n";
        #[cfg(target_os = "macos")]
        let input = "method = coreaudio\nsource = tap\n";
        // CAVA's Windows method field is an internal integer enum. Omitting it
        // selects the existing WASAPI loopback backend, never a microphone.
        #[cfg(target_os = "windows")]
        let input = "source = auto\n";
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        return Err("System audio capture is unavailable on this platform");
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        Ok(format!(
            "[general]\nbars = {BAR_COUNT}\nframerate = 10\nautosens = {}\nsensitivity = {}\nlower_cutoff_freq = 50\nhigher_cutoff_freq = 10000\nsleep_timer = 0\n[input]\n{input}[output]\nmethod = raw\nraw_target = /dev/stdout\ndata_format = ascii\nascii_max_range = 255\nbar_delimiter = 59\nframe_delimiter = 10\nchannels = mono\nmono_option = average\n[smoothing]\nnoise_reduction = {}\n",
            u8::from(self.automatic_gain), self.sensitivity, self.smoothing
        ))
    }
}

pub(crate) fn bundled_helper() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = executable
        .parent()
        .ok_or("Application directory is unavailable")?;
    #[cfg(target_os = "windows")]
    let name = "cava.exe";
    #[cfg(not(target_os = "windows"))]
    let name = "cava";
    let path = parent.join("audio").join(name);
    if !path.is_file() {
        return Err("Bundled CAVA is missing; use the complete AK Configurator package".into());
    }
    Ok(path)
}

#[derive(Default)]
struct Shared {
    latest: Option<Bars>,
    received: Option<Instant>,
    fault: Option<&'static str>,
    diagnostic: Vec<u8>,
}

/// One owned helper, two bounded pipe readers and one latest spectrum.
/// Drop kills/reaps the process and joins its readers before removing config.
pub(crate) struct Capture {
    child: Option<Child>,
    shared: Arc<Mutex<Shared>>,
    readers: Vec<JoinHandle<()>>,
    started: Instant,
    _config: tempfile::TempDir,
}

impl Capture {
    pub(crate) fn start(options: &Options) -> Result<Self, String> {
        Self::start_at(&bundled_helper()?, options)
    }

    fn start_at(helper: &Path, options: &Options) -> Result<Self, String> {
        let config = options.config().map_err(str::to_owned)?;
        let directory = tempfile::Builder::new()
            .prefix("ak-configurator-audio-")
            .tempdir()
            .map_err(|e| e.to_string())?;
        let path = directory.path().join("cava.conf");
        std::fs::write(&path, config).map_err(|e| e.to_string())?;
        let mut command = Command::new(helper);
        command
            .arg("-p")
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("Cannot start CAVA: {e}"))?;
        let stdout = child.stdout.take().ok_or("CAVA stdout is unavailable")?;
        let stderr = child.stderr.take().ok_or("CAVA stderr is unavailable")?;
        let shared = Arc::new(Mutex::new(Shared::default()));
        let output = Arc::clone(&shared);
        let reader = thread::Builder::new()
            .name("ak820-cava-frames".into())
            .spawn(move || {
                let mut pipe = BufReader::new(stdout);
                loop {
                    match read_frame(&mut pipe) {
                        Ok(Some(bars)) => {
                            let mut state = output.lock().unwrap_or_else(|e| e.into_inner());
                            state.latest = Some(bars);
                            state.received = Some(Instant::now());
                        }
                        Ok(None) => {
                            output.lock().unwrap_or_else(|e| e.into_inner()).fault =
                                Some("CAVA output ended");
                            break;
                        }
                        Err(message) => {
                            output.lock().unwrap_or_else(|e| e.into_inner()).fault = Some(message);
                            break;
                        }
                    }
                }
            });
        let reader = match reader {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Cannot start CAVA output reader: {error}"));
            }
        };
        let errors = Arc::clone(&shared);
        let error_reader = thread::Builder::new()
            .name("ak820-cava-errors".into())
            .spawn(move || {
                let mut stderr = stderr;
                let mut buffer = [0u8; 512];
                while let Ok(count) = stderr.read(&mut buffer) {
                    if count == 0 {
                        break;
                    }
                    let mut state = errors.lock().unwrap_or_else(|e| e.into_inner());
                    let left = MAX_DIAGNOSTIC.saturating_sub(state.diagnostic.len());
                    state
                        .diagnostic
                        .extend_from_slice(&buffer[..count.min(left)]);
                }
            });
        let error_reader = match error_reader {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(format!("Cannot start CAVA diagnostic reader: {error}"));
            }
        };
        Ok(Self {
            child: Some(child),
            shared,
            readers: vec![reader, error_reader],
            started: Instant::now(),
            _config: directory,
        })
    }

    /// A timeout, bad frame or helper exit is terminal. The caller stops RGB
    /// and must obtain a new explicit start; no helper is silently respawned.
    pub(crate) fn poll(&mut self) -> Result<Option<Bars>, String> {
        let child = self.child.as_mut().ok_or("CAVA is stopped")?;
        let exited = child.try_wait().map_err(|e| e.to_string())?;
        let mut state = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let error = if let Some(fault) = state.fault {
            Some(fault)
        } else if exited.is_some() {
            Some("CAVA exited")
        } else if state
            .received
            .map_or(self.started.elapsed() > START_TIMEOUT, |t| {
                t.elapsed() > FRAME_TIMEOUT
            })
        {
            Some("CAVA stopped providing spectrum data")
        } else {
            None
        };
        if let Some(error) = error {
            let diagnostic = String::from_utf8_lossy(&state.diagnostic);
            let diagnostic = diagnostic.trim();
            return Err(if diagnostic.is_empty() {
                error.into()
            } else {
                format!("{error}: {diagnostic}")
            });
        }
        Ok(state.latest.take())
    }

    pub(crate) fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if !matches!(child.try_wait(), Ok(Some(_))) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).latest = None;
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn read_frame(reader: &mut impl BufRead) -> Result<Option<Bars>, &'static str> {
    let mut line = Vec::with_capacity(MAX_LINE);
    loop {
        let buffer = reader.fill_buf().map_err(|_| "Cannot read CAVA output")?;
        if buffer.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err("Truncated CAVA frame")
            };
        }
        let end = buffer.iter().position(|&b| b == b'\n');
        let count = end.map_or(buffer.len(), |at| at + 1);
        if line.len() + count > MAX_LINE {
            return Err("CAVA frame exceeds the expected size");
        }
        line.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if end.is_some() {
            return decode_frame(&line).map(Some);
        }
    }
}

fn decode_frame(bytes: &[u8]) -> Result<Bars, &'static str> {
    let line = bytes
        .strip_suffix(b"\n")
        .ok_or("CAVA frame is missing its delimiter")?;
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let line = line
        .strip_suffix(b";")
        .ok_or("CAVA frame is missing its last bar delimiter")?;
    let mut bars = [0; BAR_COUNT];
    let mut count = 0;
    for token in line.split(|&b| b == b';') {
        if count >= BAR_COUNT
            || token.is_empty()
            || token.len() > 3
            || !token.iter().all(u8::is_ascii_digit)
        {
            return Err("CAVA frame has invalid bars");
        }
        let value = token
            .iter()
            .fold(0u16, |v, &b| v * 10 + u16::from(b - b'0'));
        bars[count] = u8::try_from(value).map_err(|_| "CAVA bar is out of range")?;
        count += 1;
    }
    if count != BAR_COUNT {
        return Err("CAVA frame has the wrong number of bars");
    }
    Ok(bars)
}

/// Pixel placement is the application-specific adapter. FFT, frequency bins,
/// automatic gain and temporal smoothing are all produced by upstream CAVA.
pub(crate) fn render(bars: &Bars, options: &Options, brightness: u8) -> [[u8; 3]; LED_COUNT] {
    let mean = |slice: &[u8]| {
        (slice.iter().map(|&v| u32::from(v)).sum::<u32>() / slice.len() as u32) as u8
    };
    let level = match options.display {
        Display::Spectrum => 0,
        Display::Pulse => mean(bars),
        Display::Bass => mean(&bars[..3]),
    };
    let mut frame = [[0; 3]; LED_COUNT];
    for (i, &(x, y)) in LED_POINTS.iter().enumerate() {
        let strength = if options.display == Display::Spectrum {
            let column = (usize::from(x) * BAR_COUNT / 225).min(BAR_COUNT - 1);
            let threshold = u32::from(64 - y) * 255 / 65;
            if u32::from(bars[column]) > threshold {
                255
            } else {
                0
            }
        } else {
            level
        };
        for (out, &color) in frame[i].iter_mut().zip(&options.color) {
            *out = (u32::from(color)
                * u32::from(strength)
                * u32::from(brightness.min(RGB_MAX_BRIGHTNESS))
                / (255 * 255)) as u8;
        }
    }
    frame
}

/// Explicit offline visual demo: synthetic band heights, no capture or FFT.
pub(crate) fn demo_bars(elapsed_ms: u64) -> Bars {
    std::array::from_fn(|i| {
        let phase = ((elapsed_ms / 8 + i as u64 * 47) % 510) as u16;
        if phase <= 255 {
            phase as u8
        } else {
            (510 - phase) as u8
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cava_frames_are_bounded_and_strict() {
        let valid = "0;1;2;3;4;5;6;7;8;9;10;11;12;13;254;255;\n";
        assert_eq!(read_frame(&mut valid.as_bytes()).unwrap().unwrap()[15], 255);
        assert!(read_frame(&mut &b""[..]).unwrap().is_none());
        for invalid in [
            valid.replace("255;", "256;"),
            valid.replace("254;", "-1;"),
            valid.replace("254;", "nan;"),
            valid.replace("254;", ""),
            valid.replace("254;", "1;2;"),
            valid.replace('\n', ""),
            valid.replace("0;", "\u{1b}]0;cava\u{7}0;"),
            "0".repeat(MAX_LINE + 1),
        ] {
            assert!(read_frame(&mut invalid.as_bytes()).is_err(), "{invalid:?}");
        }
    }
    #[test]
    fn musical_mapping_preserves_silence_caps_and_geometry() {
        for display in [Display::Spectrum, Display::Pulse, Display::Bass] {
            let options = Options {
                display,
                color: [255, 255, 255],
                ..Options::default()
            };
            assert_eq!(render(&[0; BAR_COUNT], &options, 160), [[0; 3]; LED_COUNT]);
            assert!(render(&[255; BAR_COUNT], &options, 255)
                .iter()
                .all(|rgb| *rgb == [160; 3]));
            assert_eq!(render(&[255; BAR_COUNT], &options, 0), [[0; 3]; LED_COUNT]);
        }
        let mut bars = [0; BAR_COUNT];
        bars[0] = 255;
        let frame = render(&bars, &Options::default(), 160);
        for (led, &(x, _)) in LED_POINTS.iter().enumerate() {
            assert_eq!(frame[led] != [0; 3], usize::from(x) * BAR_COUNT / 225 == 0);
        }
    }
    #[test]
    fn capture_configuration_selects_system_audio_not_microphone() {
        let options = Options::default();
        let config = options.config().unwrap();
        assert!(config.contains("raw_target = /dev/stdout"));
        assert!(config.contains("sleep_timer = 0"));
        #[cfg(target_os = "linux")]
        assert!(config.contains("method = pulse\nsource = auto"));
        #[cfg(target_os = "macos")]
        assert!(config.contains("method = coreaudio\nsource = tap"));
        #[cfg(target_os = "windows")]
        assert!(config.contains("[input]\nsource = auto\n[output]"));
        assert!(Options {
            sensitivity: 0,
            ..options
        }
        .config()
        .is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn managed_helper_is_latest_only_and_drop_reaps_it() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let helper = directory.path().join("fake-cava");
        std::fs::write(&helper, b"#!/usr/bin/python3\nimport sys,time\nwhile True:\n sys.stdout.write('42;'*16+'\\n');sys.stdout.flush();time.sleep(0.01)\n").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut capture = Capture::start_at(&helper, &Options::default()).unwrap();
        let pid = capture.child.as_ref().unwrap().id();
        let config = capture._config.path().to_owned();
        let deadline = Instant::now() + Duration::from_secs(3);
        let frame = loop {
            if let Some(frame) = capture.poll().unwrap() {
                break frame;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(frame, [42; BAR_COUNT]);
        drop(capture);
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
        assert!(!config.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn malformed_helper_output_is_terminal_without_respawn() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let helper = directory.path().join("fake-cava");
        std::fs::write(&helper, b"#!/usr/bin/python3\nimport sys,time\nsys.stdout.write('bad;\\n');sys.stdout.flush();time.sleep(10)\n").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut capture = Capture::start_at(&helper, &Options::default()).unwrap();
        let pid = capture.child.as_ref().unwrap().id();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if capture.poll().is_err() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(capture.poll().is_err());
        assert_eq!(capture.child.as_ref().unwrap().id(), pid);
        drop(capture);
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
}
