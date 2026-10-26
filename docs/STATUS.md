Русский перевод: [Состояние проекта](ru/STATUS.md).

# Project status

Updated 2026-10-26. Application source version: `0.1.0-beta.2`. The active AK820 firmware target is v0.6.0-beta.2, published 2026-10-26T04:52:40Z at commit `2e0b28b384872c482e0e358a4b353f00ddd976a7`. The target release defines ten supported exact profile images. This is release preparation, not a fully qualified first release. Exact build and test evidence is recorded in [REVIEW.md](REVIEW.md).

## Current source contract

The active source catalog supports Main, Game, Mmo, Typing, Code, Pointer, Onehand, Creative, Shortcuts and Multios. Every profile uses schema03 (0x082003), with exact packed CODE/settings identity, counts, QSIDs, source-aware keycode restrictions and explicit directional transitions. A full-image/settings loader verifies the target before enabling edits, reads required settings and fails closed. Unchanged defaults do not cause a setter; typed-module changes compare the full NVM before and after to preserve neighboring data. A timeout, storage fault or failed readback stops the session.

The offline project format is v2 and pins target release, catalog and Intel HEX. Beta1 v1 projects are rejected for deliberate manual migration. The historical source registry is limited to three schema02 backup identities and has no aliases or live beta1 path. Alpha3 remains an exact separate compatibility path.

The editor provides ten profile examples, schema03 typed controls, and profile-specific built-in RGB effects. The static Direct editor and local presets are implemented in source; undo/redo and drag selection remain UI refinements pending. Final validation and hardware qualification are separate. Direct palette editing is limited to Main, Game, Mmo and Creative. The keyboard has 82 LEDs for 83 physical keys; encoder press has no LED. Direct sends 82 HSV colors in ten packets of at most nine LEDs, caps V at 160 and requires explicit RAM apply. There is no palette getter or persistence API, so Direct edits are volatile and do not trigger automatic NVM save. Saved mode 5 maps to Direct for those four profiles and Reactive for the other six.

The RGB research and five-phase implementation guide is [RGB-PLAN.md](RGB-PLAN.md); its Russian counterpart is [ru/RGB-PLAN.md](ru/RGB-PLAN.md). The RGB guide remains the detailed planning source.

## Verification boundary and release gates

Current source build/test results and review findings are recorded in [REVIEW.md](REVIEW.md). Tests, synthetic fixtures and CI establish source/model properties only. They do not establish physical input behavior, settings retention, HID timing, or flash behavior. A positive released packed-image/full-ROM fixture is unavailable because OEM erase-helper bytes are not available; public Intel HEX alone cannot build that image. OEM-private bytes are not added.

The backup-directory implementation currently permits private file I/O only on Linux. Full backup capture/history, complete restoration UI and executor, installer UI and executor, and exact physical-unit ownership remain open. Physical retention and HID timing require qualification. The versioned six-target candidate ZIP/provenance/SHA256 pipeline is implemented; local/current-platform evidence is recorded in [REVIEW.md](REVIEW.md). The successful CI run `40174635838` for `1afd0f1` is historical and does not verify current-source execution on all six targets. Current Ubuntu 24.04 source-build evidence does not establish Debian 12 compatibility from a prior binary, nor Windows/macOS/arm64 runtime support. Six-target execution, installers, signing and per-target runtime/function checks remain release gates.

No full first-release qualification is claimed. Firmware writes and installation remain fail-closed pending required image and hardware evidence. Browser work remains a later stage. Read the [plan](PLAN.md), [desktop guide](DESKTOP.md), and [installation guide](INSTALL.md).
