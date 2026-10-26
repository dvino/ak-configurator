# RGB lighting plan

This plan separates the current AK820 beta2 RGB contract from later application features. It is a design and implementation plan, not a release claim or authorization to operate a keyboard. The configurator's current status remains in [STATUS](STATUS.md); project-wide milestones remain in [PLAN](PLAN.md).

## Verified baseline and constraints

The current firmware release is **[v0.6.0-beta.2](https://github.com/dvino/ak820/releases/tag/v0.6.0-beta.2)**, published 2026-10-26 from peeled commit `2990ef877c5c6c54013b8821450c5a91878b8c3a`; annotated tag object: `2e0b28b384872c482e0e358a4b353f00ddd976a7`. The catalog has ten profiles: Main, Game, Mmo, Typing, Code, Pointer, Onehand, Creative, Shortcuts and Multios. VialRGB Direct is present only in Main, Game, Mmo and Creative. All profiles retain built-in effects. The application must derive available controls from the exact image/profile contract, never from the keyboard name or an assumed global capability.

The board has 82 RGB LEDs and 83 physical keys. Matrix position (13,0), the encoder press action, has no LED. HSV channel values use 8-bit fields and the current global VialRGB value ceiling is 160. Direct writes use the existing VialRGB packet form `[7, 0x42, first, 0, count, HSV…]`; each packet carries at most nine LED entries, so a full 82-LED frame is sent as ten HID transactions. There is no Direct palette getter, atomic frame commit, lease/watchdog, or persistence. Direct colors are volatile RAM state. Do not write NVM per animation frame. The current application stops without automatically restoring the previous built-in effect; disconnect, error or uncertain state also stops without blind restoration. A built-in effect must be selected explicitly.

The native editor/effect engine should own AK820 lighting through the existing VialRGB path. OpenRGB's SDK is a host-side TCP protocol (port 6742 by default), not a USB keyboard protocol. No second MCU protocol is needed. An optional later OpenRGB SDK server bridge can expose this keyboard to external OpenRGB integrations while the companion retains HID ownership. An OpenRGB client mode can control devices already managed by OpenRGB; it does not expose the AK820 as an OpenRGB device.

These facts were checked against the beta2 firmware source and release contract, the current configurator source, and the OpenRGB SDK/release documentation. The OpenRGB source reviewed locally is clone `../openrgb-research` at `84d872e8ce1cdbcbd7ca17c7a68cfa1c536bfdde`. This is source inspection, not physical keyboard qualification. OpenRGB 1.1 (2026-10-16) documents SDK protocol version 6 and Plugin API version 5. A custom experimental QMKVialRGBDevices registration could be explored against the existing OpenRGB driver contract, but the repository has no confirmed registration for AK820 VID 320F/PID 505B and no compatible hardware run. Do not present that route as already supported.

## Product direction

Treat the keyboard companion as the source of truth for profiles, capabilities and the HID session. Use one scheduler that composes colors for all requested layers into a latest-frame buffer. Bound frame size, queue depth and transaction count; discard stale frames rather than letting latency accumulate. The current source caps playback at 10 frames/s as a conservative application limit, not a hardware-qualified rate; measure full-cycle timing on supported hosts before changing that limit. Pause or lower priority during settings reads/writes, backup, firmware preparation and any device-critical operation. A dropped or malformed transaction stops the stream and reports unknown volatile lighting state; there is no readback to claim success.

Expose plain controls for per-key colors, group selection, effect intensity/speed and local presets. Make unsupported Direct controls unavailable for the six profiles without Direct; built-in effect selection remains available there. Identify per-key Direct colors as session-only, unsaved, and unrecoverable after disconnect. Backup and restore must explicitly exclude Direct palette data.

The editor should compose a base effect, optional spatial or audio-reactive layer, optional utility indicators, and a profile/game override under a documented priority order. Initial priorities should be safety and explicit Off, user static choice, selected profile/game override, optional visualizer, then idle built-in effect. Provide explicit controls to pause layers, mute/calm animations, restore the prior supported mode when possible, and clear local presets. A layer that depends on Caps/Num Lock, volume, timer, application layout, notifications or temperature needs a real host data source and an opt-in/privacy decision; the firmware alone does not provide those host events. Keyboard-layer indicators require firmware events or a justified polling source and must not be inferred from a stale editor view.

## Feature research and applicability

These products show useful feature patterns, not AK820 compatibility or a commitment to reproduce a whole suite. The comparison below separates currently shipped examples from future proposals.

| Product/reference | Documented feature pattern | AK Configurator applicability |
| --- | --- | --- |
| [Razer Chroma Configurator](https://dl.razerzone.com/master-guides/RazerSynapse/BlackWidowChromaV2-00000545-en.pdf), p. 22 | Select multiple zones, add effect layers, and set a color for each key | Reference for per-key/group selection and layered effects; native AK820 scenes now cover local masks and basic layers, while spatial/audio/game inputs remain future work |
| [Corsair Murals](https://www.corsair.com/us/en/explorer/gamer/accessories/inside-the-new-murals/) (updated 2026-09-22) | Spatial canvas, per-device playback, audio-reactive accents and screen mirroring | Possible future compositor inputs; custom-coded/community effects are described as coming later, not as shipped features |
| [Logitech LIGHTSYNC](https://www.logitechg.com/en-us/software/guides/lightsync-rgb) | Freestyle color painting, custom animations, audio visualizer, screen sampler and game-responsive modes | Useful future host-side patterns, subject to capture permissions and source availability |
| [SteelSeries GameSense](https://github.com/SteelSeries/gamesense-sdk/blob/master/doc/api/sending-game-events.md) | Event API carries values such as health and ammo to device handlers | Requires a specific game integration that supplies real events; process detection alone is not an event source |
| [OpenRGB Effects and plugins](https://openrgb.org/plugin_effects.html) | Effects table includes standard animations, audio visualization, Ambilight and GLSL shaders; SDK and community plugins extend integrations | Existing external project/API to interoperate with later; do not imply an AK820 driver or copy an engine with incompatible terms |

[OpenRGB SDK integrations](https://openrgb.org/sdk.html) include Artemis for Windows/Linux, Aurora for Windows, and KeyboardVisualizer for Windows/Linux/macOS. KeyboardVisualizer accepts input or loopback audio; these are candidates for external interoperability tests once the SDK bridge exists.

Community projects can inform research, but their OS support is not AK820 support. OpenRGB documents GPL v2, Aurora MIT, and Artemis PolyForm Noncommercial 1.0.0; implementation should use documented external interfaces and must not copy an incompatible engine.

Game executable detection can select a local profile, subject to operating-system permissions and supported process enumeration. It does not produce game events. Health, ammo, cooldown or objective effects require a documented supported local game API/plugin; do not inject into games or claim generic event support. E1.31 is a documented OpenRGB feature; Art-Net support has not been confirmed for this project and must not be promised. Screen sampling requires explicit capture consent and platform-specific APIs. Wayland portal capture is user-mediated and may have compositor limitations; Windows loopback audio and screen capture have different permissions and APIs. Audio sampling should be local and opt-in, with a clear source selector and a stop control.

## Phased implementation

### Phase 1 — exact beta2 contract, built-ins and static Direct

**State:** the static editor and local presets are implemented. A separate additive scene format v2 does not change the v1 palette format. The editor has click/Ctrl/Cmd/Shift selection and bounded undo/redo; drag selection is not implemented. Host/model checks and Linux offline UI verification are recorded in [REVIEW.md](REVIEW.md); hardware qualification remains open.

Use the exact beta2 catalog contract to gate each RGB mode by profile. The implemented static Direct editor provides individual LED colors, matrix/key mapping that excludes the encoder press position, group selection, brightness, local presets and explicit apply. Retain the built-in mode controls. Validate index bounds, profile feature availability, HSV range and HID payload sizing before sending. Preserve the current one-request/one-response stop behavior. The scene editor's undo/redo history is bounded to 32 draft states; key selection remains click/modifier based.

**Acceptance:** final host/source validation covers all ten profile capability rows, Direct allow/deny, 82 LED indices and encoder-without-LED mapping, boundary HSV values, nine-entry packet boundaries across the 82-LED/ten-transaction frame, malformed/short replies and stop-on-first-failure. UI checks show volatile-state and no-readback limits. Physical writes, reconnect behavior and retained RAM state remain separate hardware qualification.

### Implementation map

| Area | Current or proposed module | Responsibility and boundary |
| --- | --- | --- |
| Static color model | `src/ak820_rgb.rs` | Source of truth for the 82-LED geometry/map, preset validation, brightness scaling and chunking into packets of at most nine LEDs |
| Static palette and scene UI | `src/ui/ak820_rgb_palette.rs` | Static palette v1 and additive scene v2, per-key/group editor, local presets, 32-state undo/redo and explicit apply; selection is click/modifier based |
| HID transport | `src/hid_ak820.rs` | Owns profile-gated VialRGB traffic and ACK checks through the existing device session |
| Frame composition (implemented; focused host-tested) | `src/ak820_rgb_compositor.rs` | Scene v2 composes the base palette with up to 16 ordered/enabled layers, LED masks, Solid/Gradient/Wave/Breathing/Rain effects, horizontal/vertical axes, opacity and quiet-hours brightness cap |
| Frame scheduling and HID ownership (implemented; focused host-tested) | `src/ak820_rgb_scheduler.rs`, `src/ui/ak820_rgb_runtime.rs` and existing HID session | Latest-frame queue capacity one; 100 ms app interval (max 10 frames/s, not hardware-qualified); pause/drop/resume, monotonic/wall-clock deadlines, cancellation and lifecycle/mode checks |
| Local audio (planned) | `src/ak820_rgb_audio.rs` | Opt-in host-local capture/feature extraction; no raw audio storage or transmission |
| OpenRGB bridge (planned) | `src/ak820_openrgb_bridge.rs` | Optional SDK adapter that feeds the compositor/scheduler; loopback bridge by default, with external interface exposure isolated behind a separate explicit setting and validation boundary |

All inputs converge on the single companion scheduler and HID owner. None may create a parallel USB loop or bypass the profile contract, queue bound or firmware-critical pause.

### Other companion work

Existing editor scope includes key remapping, layers, macros, Tap Dance, Combo and Key Overrides where the exact firmware profile declares them. Later host-side conveniences may include per-application presets and opt-in indicators for Caps Lock, mute, timers, volume or shortcut hints. These need real OS/application data sources and explicit user controls. A firmware-layer indicator requires an actual keyboard event or another validated source; it must not be inferred from a stale editor selection. No keylogging or telemetry is introduced, consistent with [privacy requirement AKW-070](REQUIREMENTS.md).

### Phase 2 — native compositor, masks, effects and schedules

**Implementation and source/host checks:** the native scene editor/compositor and one HID-owner playback path are implemented. Scene v2 is additive to the static palette v1 format. It supports up to 16 ordered/enabled layers, masks over the fixed 82-LED map, Solid/Gradient/Wave/Breathing/Rain, horizontal/vertical axes, opacity, periods from 250 to 60000 ms, local preview, and quiet-hours brightness cap. Quiet hours use local wall-clock minutes, include the start and exclude the end, and support overnight intervals. The app does not run a persistent scheduler daemon or start playback automatically. Thirty-five focused RGB host tests passed. Linux demo verification showed animated preview and exact JSON save/reload for 82 colors and two layers; detailed evidence is recorded in [REVIEW.md](REVIEW.md).

Playback uses a queue of one where the latest frame replaces stale work, with a conservative maximum of 10 frames/s as an application bound only. Settings, backup, modal and connection-priority operations pause playback and drop pending frames; resume emits a fresh frame only for the same intact session. A monotonic UI gap over two seconds, a wall-clock jump over two seconds, or a backward wall-clock jump stops playback and requires explicit restart. A pending Off request expires across the same gap checks, so it cannot send a stale black frame after a long suspension. Disconnect, generation/profile change or error stops without retry or restore. Every frame checks mode ownership; physical RGB mode/speed/HSV changes stop the stream without overwrite. Stop cancels between packets and leaves already-sent RAM colors. Explicit Off sends one black frame only when the session is safely intact. No built-in effect is automatically restored; select it explicitly. No per-frame NVM save, palette getter or atomic frame commit exists. These are application source behaviors; device behavior and maximum safe/visible rate remain unqualified.

**Remaining qualification:** source/host checks cover composition, masks, bounded latest-frame scheduling, deadlines, pause/resume, cancellation, Off and mode ownership. Measure complete HID cycle time and CPU/memory on target platforms before choosing any hardware-qualified rate. Complete per-OS UI/runtime checks and physical device validation, including retained RAM behavior and failure/disconnect cases. Record source tests, platform checks and physical behavior separately. The 10 frames/s limit is a conservative source ceiling, not a measured or qualified hardware rate.

### Phase 3 — local audio-reactive effects

Add optional local FFT/VU, bass, beat and stereo response with user gain, smoothing, attack/release and quiet-hours controls. Audio capture must be opt-in and visibly active; raw audio is not saved or transmitted. Provide a no-capture test source for repeatable host-side tuning.

**Acceptance:** tests use synthetic audio signals and prove silence, bounds, smoothing, mute and stop semantics. Platform capture setup and permission denial are checked separately on Linux, Windows and macOS. Report latency and CPU/memory measurements. If the platform source is unavailable or permission is denied, lighting falls back cleanly without retry loops.

### Phase 4 — interoperability, game events, ambient capture and profiles

Consider an optional OpenRGB SDK server bridge for OpenRGB clients, negotiating protocol version 6, stable device/zone/LED IDs and bounded messages; default to loopback-only binding and require an explicit setting to expose other interfaces. Separately consider an OpenRGB client that controls devices already present in an OpenRGB server. Evaluate an experimental QMKVialRGBDevices registration only after the native contract is stable and with a test device. Add local game profile switching from process detection; expose event-driven game effects only for specifically supported local integrations. Add opt-in screen ambient capture and host profile changes after permission and focus/foreground behavior have been checked.

**Acceptance:** interoperability tests cover incompatible version rejection, stable identifiers, bounded frame parsing, disconnect cleanup and default local-only network binding. No external source can issue unbounded USB traffic or bypass the companion scheduler. Game tests distinguish process-based profile selection from actual event input. Screen tests record permission flows and Wayland/compositor limits. No game injection or network telemetry is used.

### Phase 5 — optional firmware frame commit or lease

Only consider firmware protocol changes if measured transaction rate, visual tearing, device disconnect or stuck-RAM effects demonstrate a material problem that cannot be resolved by bounded host scheduling. A firmware proposal may add bounded frame buffering, commit semantics and a lease/watchdog that returns control to a built-in effect after the host disappears. It must respect controller RAM/ROM/stack constraints, settings/NVM boundaries and existing Vial compatibility. No per-frame NVM writes.

**Acceptance:** require measured evidence of the limitation, firmware resource and interrupt review, protocol versioning/backward compatibility, fail-safe host/firmware models, exact beta release contract updates and physical tests for partial frames, disconnect, watchdog, built-in fallback and NVM invariance. Until these gates pass, remain on the existing VialRGB Direct packet path.

## Verification and release boundary

Every phase records source inspection, host tests, platform capture/network checks, and physical device evidence as distinct evidence classes. Synthetic frames, a successful build, an SDK handshake or existing third-party driver code do not establish AK820 physical support. The application remains `0.1.0-beta.2`; no release is published. Candidate packaging exists, but RGB source/host verification evidence must be recorded in [REVIEW.md](REVIEW.md) before completion or test counts are claimed. Six-target execution, installers, signing, runtime qualification, all-profile qualification and RGB hardware acceptance remain open.

## Primary references

- [OpenRGB SDK](https://openrgb.org/sdk.html) — host protocol and integration model.
- [OpenRGB releases](https://openrgb.org/releases.html) — release and protocol/API version references.
- [OpenRGB plugin effects](https://openrgb.org/plugin_effects.html), [plugins](https://openrgb.org/plugins.html), [visual maps](https://openrgb.org/plugin_visual_map.html), and [E1.31 receiver](https://openrgb.org/plugin_e131_receiver.html) — feature and extension examples.
- [SteelSeries GameSense color handlers](https://github.com/SteelSeries/gamesense-sdk/blob/master/doc/api/json-handlers-color.md) — event-driven game-lighting model.
- [Corsair Murals](https://www.corsair.com/us/en/explorer/gamer/accessories/inside-the-new-murals/), [Logitech LIGHTSYNC](https://www.logitechg.com/en-us/software/guides/lightsync-rgb) — spatial, screen and game-responsive lighting patterns.
- [XDG ScreenCast portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html), [Windows loopback recording](https://learn.microsoft.com/windows/win32/coreaudio/loopback-recording) — platform capture boundaries.
- [SignalRGB documentation](https://docs.signalrgb.com/) — current end-user feature reference, not an AK820 compatibility claim.

- [Aurora license](https://github.com/Aurora-RGB/Aurora/blob/master/LICENSE), [Artemis license](https://github.com/Artemis-RGB/Artemis/blob/master/LICENSE).
- [OpenRGB QMK/VialRGB driver at the inspected commit](https://gitlab.com/CalcProgrammer1/OpenRGB/-/tree/84d872e8ce1cdbcbd7ca17c7a68cfa1c536bfdde/Controllers/QMKController/QMKVialRGBController).
