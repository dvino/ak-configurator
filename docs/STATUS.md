Русский перевод: [Состояние проекта](ru/STATUS.md).

# Project status

Updated 2026-10-25. Native development is on `desktop-dev`, application version
`0.1.0-alpha.1`. Entropy commit `5b222b5d6349fb351f53b36c66169095b3039fc1`
remains the foundation, with its history and licenses retained. Browser work
starts after a reasonably usable desktop milestone.

## Implemented desktop scope

The executable and window are named AK Configurator. Settings, caches and
backups use a separate `ak-configurator` directory. The imported updater cannot
download Entropy as an application update, and startup does not enable global
typing hooks. A bundled Russian/English guide describes the supported firmware
and limitations.

The wired AK820 path accepts only the full permanent v0.6.0-alpha.3 firmware
referenced by `dvino/ak820` commit `8ffa3e6`. USB IDs and Vial UID identify a
candidate; they do not enable editing. Connection checks VIA/Vial identity,
the manifest and the SHA-256 of actual CODE bytes in `[0x2800, 0xC700)`, then
reads every required setting before enabling writes. The catalog pins are in
[assets/ak820/catalog.json](../assets/ak820/catalog.json).

| Area | Implemented behavior |
| --- | --- |
| Layout | All 6 layers, 83 physical keys, matrix mask and both encoder directions |
| Assignments | Firmware-specific keycode policy; unsupported and destructive new actions blocked |
| Macros | 16 slots, 8192-byte capacity, ASCII and Vial opcodes 1–7; complete validation, explicit unlock, chunked upload and final exact readback |
| Combo | 8 entries, two distinct supported input keys, firmware-valid output, fixed 40 ms term |
| RGB | Off, Direct, Solid, Breathing, Cycle all, Cycle left/right and Reactive; maximum brightness 160; saved state checked after save |
| Errors | One attempt per AK820 request; timeout, storage fault or readback mismatch stops the session until explicit reread |
| Linux access | AK820-only hidraw permission installer for USB `320f:505b` |
| Offline projects | `--demo-ak820` opens nine session-local example projects, profile-aware assignments, macros/Combo/RGB and schema03 panels without discovery or HID |

Read-only connection does not start an unlock, rename layers, initialize missing
settings, or write defaults. Unknown, custom, trial or inconsistent images fail
connection; the application does not present them as a usable editor.

## Verification boundary

A Linux x86_64 development executable was built with Rust 1.97.0 and started in
an isolated Xvfb display using `--demo-ak820`. Offline protocol tests exercise
required reads, wrong capacities, malformed responses, write readback,
macro limits, lock state and failure latching. Firmware-verification negative
fixtures reject a trial manifest, an unknown image and synthetic CODE that does
not match a known manifest. The successful settings-loader fixture starts after
image verification and does not prove a real image was read from USB.

The test and independent-review results are recorded in [REVIEW.md](REVIEW.md).
No physical keyboard commands, flash, reset, unlock or retention tests were run.
A successful model or GUI preview is not hardware qualification.

The `desktop-dev` workflow defines six raw development binaries with SHA-256
manifests: Linux and Windows x86_64/arm64, and macOS arm64/x86_64. Baseline commit
`7b88d59` passed run `40168550192`; its downloaded artifacts passed checksum
and binary-architecture checks. This is evidence for that baseline, not later
uncommitted changes or runtime support on all six targets.

The beta.1 catalog now supplies per-profile counts, active QSIDs, source-aware
keycode admission and exact settings identities. Only `work` has a packed
settings identity in the bundled matrix; all other identities remain absent.
In demo mode, profile selection opens independent editable example projects.
Session-local edits include layers, both encoder directions, macros, Combo,
RGB and the active typed schema03 panel. The local-project window saves and opens each profile as an
`.ak820project.json` file; opening validates the complete file before changing
state. Undo project import restores the preceding example in the current
session. Project files are separate from firmware backups. Outside demo mode,
selection remains catalog preview. Beta device settings and flashing stay
blocked; the live path still recognizes only the exact alpha.3 image.

New local backup modules validate strict archive envelopes, derive settings
from schema02/03 NVM and check ROM against the qualified beta catalog identity.
Typed codecs preserve inactive storage and neighboring bytes. These modules
are foundations: full backup capture/history, migration, restore execution and
UI are unfinished. Schema01 full derivation requires explicit migration and
is rejected. Protected ROM hashes identify a hardware revision; they do not
establish ownership of a physical keyboard. Positive full-ROM acceptance
coverage remains missing.

The offline packet passed the complete host suite (1593 passed, 0 failed,
14 ignored), independent integration review and the Linux demo checks described
in [REVIEW.md](REVIEW.md). No hardware qualification is implied.

## Remaining work

The Linux development binary is not a packaged or qualified release. Windows
and macOS execution, installers, signing, resource measurements, device
read/write behavior and retention still need evidence. Full backup/restore,
firmware updates, OEM recovery, per-LED palette editing and the optional OS
helper are not implemented here. `.entlayout` export remains a partial editor
snapshot; its device-settings import remains blocked until preservation and
compatibility checks exist. Local offline project import is a separate format. Direct-mode colors are volatile and cannot be read back through this
firmware API.

The next desktop milestone is per-image live loading and typed transaction
models (the remaining part of plan 3a), complete settings backup/history,
migration and guarded restore (plan 3b), then separately authorized hardware
qualification. Browser implementation remains deferred. See the [plan](PLAN.md) and
[desktop guide](DESKTOP.md).

Local source/package acceptance on 2026-10-25 is recorded in [REVIEW.md](REVIEW.md).
That historical import did not repeat Rust checks; current development uses an
isolated Rust 1.97.0 build container, with results recorded separately.
