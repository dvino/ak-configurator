# RGB implementation and reuse plan

Russian translation: [План RGB](ru/RGB-PLAN.md).

## P1 application scope

Baseline: `8b1f67f`, 2026-10-26. The supported firmware remains the exact `v0.6.0-beta.2` catalog and application version remains `0.1.0-beta.2`. This continuation integrates existing audio processing and the OpenRGB wire format into one native configurator. It does not introduce another audio analysis engine or require the user to launch a separate visualizer window.

| Item | Current implementation | Boundary |
| --- | --- | --- |
| Music | Bundled CAVA 1.0.0 owns system capture, FFT, logarithmic bands, automatic sensitivity and smoothing; native controls, preview and stop manage its hidden process | Native system capture and permission flows still require per-OS execution |
| Musical display | Spectrum, pulse and bass map CAVA's 16 ready band values onto the existing 82-LED geometry; color, sensitivity and smoothing controls; explicit no-capture test spectrum | Test spectrum is synthetic band data, not evidence of system capture or beat detection |
| OpenRGB interoperability | Optional loopback SDK server exposes one keyboard, one Direct mode and one 17×6 physical matrix approximation; SDK 0/1 compatibility, maximum negotiated version 1 | It is a compatible server endpoint for clients, not an OpenRGB/Qt plugin host or a general RGB device manager |
| Previous effect | Explicit **Stop and restore effect** returns the actual supported built-in effect, including Off, speed and HSV, observed before streaming | Same intact verified session only; no restore after failure, cancelled frame or ownership loss; previous Direct colors cannot be recovered |
| Current Direct palette getter | Unavailable in beta2 | Requires a versioned firmware change; host-requested colors and local presets are not readback |
| Atomic frame apply | Unavailable in beta2 | Requires firmware buffering/commit support; ten acknowledged packets are not an atomic frame |

The two firmware-dependent items remain open. Application integration, source/host tests, native capture, physical keyboard qualification and release readiness are separate. Exact checks belong in [REVIEW.md](REVIEW.md).

## Fixed firmware contract

Firmware `v0.6.0-beta.2` is pinned to peeled commit `2990ef877c5c6c54013b8821450c5a91878b8c3a`, annotated tag object `2e0b28b384872c482e0e358a4b353f00ddd976a7`. Main, Game, Mmo and Creative advertise Direct; the other six profiles retain built-in effects without Direct. All live sources require the verified exact image and fully loaded settings. A profile name never enables writes.

There are 82 LEDs and 83 physical keys. Encoder press at matrix position (13,0) has no LED. Existing SLOT_MAP and LED_POINTS retain the firmware LED indices. A frame uses ten VialRGB transactions of at most nine LEDs, with HSV V bounded to 160. There is no Direct palette getter, atomic commit, palette persistence or host lease. No frame saves NVM. Disconnect/error/ordinary Stop can leave already acknowledged colors or a partial frame in RAM.

The existing scene v2 editor and static palette v1 remain compatible. Scene v2 has up to 16 ordered/enabled layers, masks, Solid/Gradient/Wave/Breathing/Rain, axes, opacity, 250–60000 ms periods, 32-state undo/redo and local-time quiet-hours caps. Selection uses click/Ctrl/Cmd/Shift; drag selection remains unimplemented. Music controls are session controls and do not change those preset formats or become device-backup content.

## Reuse decision

[CAVA](https://github.com/karlstav/cava/tree/15f89867908fa66baaaec8d4b9a2d684a329a0fb) explicitly supports consuming its raw stdout in other applications. The bundled helper is built from that pinned 1.0.0 source. CAVA performs capture, FFT, frequency grouping, sensitivity and smoothing; application code only manages its lifecycle, validates bounded bar values and places colors on this keyboard. Source, dependency archives, licenses and integration patches are recorded in [THIRD-PARTY-AUDIO.md](THIRD-PARTY-AUDIO.md) and `vendor/cava-provenance.json`.

[OpenRGB Effects](https://openrgb.org/plugin_effects.html) has ready audio effects, but its binary plugin requires the OpenRGB/Qt host. Bundling that full host merely to process one keyboard's audio would add a second device manager and plugin configuration. CAVA's documented pipe interface is the chosen narrower integration. External compatible clients can still use the optional SDK bridge.

Available Rust OpenRGB libraries inspected provide client implementations, rather than a bounded server exposing this HID owner. The bridge therefore adapts the official SDK 0/1 descriptor field order from OpenRGB `RGBController.cpp` and `NetworkProtocol.h` at `84d872e8ce1cdbcbd7ca17c7a68cfa1c536bfdde`. Upstream attribution and GPL-2.0-or-later notices are retained under the application's GPL-3.0-or-later terms. Only the listener, strict admission, latest-frame handoff and application lifecycle are new glue. Protocol 6, plugin hosting and profile persistence are not advertised.

## Audio lifecycle and platforms

**Music and OpenRGB** in the per-key RGB page offers system audio from the default output or a test spectrum without capture. Capture starts only after **Start music**. It does not fall back to a microphone. Color, spectrum/pulse/bass display, automatic or manual sensitivity and smoothing are selected before starting; scene brightness and quiet hours continue to apply while running.

| Platform | Bundled CAVA input | Requirement |
| --- | --- | --- |
| Linux | PulseAudio default sink monitor | PulseAudio or PipeWire's PulseAudio compatibility service; no X11 screen API is needed |
| Windows | Existing WASAPI loopback backend | Default playback endpoint; no microphone fallback |
| macOS | Public CoreAudio process tap | macOS 14.2+ and OS capture permission; older versions keep non-audio application functions |

The helper path is fixed beside the installed application, under `audio/cava` or `audio/cava.exe`; the macOS path is inside the app bundle. No PATH search, runtime download or user-selected executable is used. A private temporary configuration contains capture options only. No raw samples are written to disk, sent over the network or retained by the configurator. Only sixteen integers per frame reach the application. Local diagnostics are bounded.

The helper has no separate visible window. Missing helper, permission/backend failure, invalid output, process exit or stalled output stops the source without automatic retry. Stop, source replacement and application teardown terminate/reap the owned process and close its pipe readers. Capture never starts from a preset, application launch or reconnection. A synthetic FIFO test of the actual helper exercises its FFT without opening the system audio device; the UI's synthetic spectrum only exercises rendering.

## OpenRGB endpoint

**Receive OpenRGB** opens only `127.0.0.1`, default port 6742. A conflicting server reports an error; another local port can be selected. Exactly one client is accepted. After disconnect or a malformed request, a new explicit start is required. Clean static-color clients may remain idle; incomplete packets and blocked replies have two-second deadlines.

Supported operations are protocol negotiation, client name, controller count/description, Direct/custom mode, an exact advertised Direct mode update, full keyboard colors, zone-zero colors and one LED. There is no mode save, flash, device setting or arbitrary USB command. Headers, embedded sizes, controller/zone/LED IDs, color counts and reserved bytes are checked before accepting a frame; payloads are bounded to 16 KiB. Only one latest frame is retained.

The virtual matrix uses six physical rows and 17 approximate physical columns with all 82 LED indices represented once. It does not reuse the electrical 16×8 switch matrix. Descriptions return requested host colors, explicitly not keyboard readback. The retained unscaled color frame is scaled again when brightness or quiet hours change, even when the client sends no new color.

## Common ownership and stopping

Scenes, CAVA and OpenRGB use the existing single HID owner and one latest pending frame. The conservative application ceiling remains ten frames/s, including a completion cooldown; it is not a measured device rate. No independent USB loop is added. Each frame checks the live mode observation. Physical mode/speed/HSV changes stop the stream without overriding them.

Settings, backup and modal work take priority. A pause before dispatch drops pending work and may resume with fresh data in the intact session. Cancelling an in-flight frame retires ownership and stops its source; explicit restart is required. UI/clock gaps over two seconds, reconnects, errors and generation/profile changes also stop without retry. Explicit playback continues while the window is minimized or hidden in the tray and the GUI process remains alive. Quit or closing the interface into the lightweight layout helper stops RGB and capture; reopening does not restart them.

Ordinary **Stop source** cancels pending work without starting a mode-restoration or black-frame write. **RGB Off** is an explicit black-frame request. **Stop and restore effect** is a separate explicit action: it closes the source, waits for an intact in-flight frame, checks the same session and full Direct mode observation, then uses the existing checked setter once and reads it back. It expires across suspension, yields to other work and never saves NVM. Starting in Direct offers no restore because its former palette is unreadable.

## Remaining phases

Game events, application profile switching and screen ambient capture remain later work. They need a real event source or consented capture API, not inferred events or a new general-purpose effects engine. This P1 change does not claim those integrations.

Palette readback, frame commit and a host-loss lease require a separately versioned firmware design and exact catalog update, including 8051 RAM/ROM/stack/interrupt checks and hardware qualification. They are not simulated with a cached host palette or described as completed here. Physical RGB timing, visible smoothness, capture permission behavior, Windows/macOS/ARM64 execution, six-target CI and signing remain separate release gates.
