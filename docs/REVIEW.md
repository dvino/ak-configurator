Русский перевод: [Адаптация и ревью](ru/REVIEW.md).

# Desktop adaptation and review

Date: 2026-10-24. Milestone: AK Configurator `0.1.0-alpha.1`, `desktop-dev`.

## Source and scope

- Configurator base: `df52020fd39e96776617fbc017d63a3daf7a8da6`.
- Entropy base: `5b222b5d6349fb351f53b36c66169095b3039fc1`.
- Firmware reference: `dvino/ak820` main `8ffa3e6`, full permanent v0.6.0-alpha.3.
- Firmware evidence: `tools/installer/settings_backup.py`,
  `tools/ak820_device_protocol.py`, `qmk-8051-ak820/util/pack_ak820.py`,
  `quantum/{via,vial,vialrgb,dynamic_keymap}.c` and the AK820 keyboard sources.
- The firmware tree and OEM/recovery data are not copied. The small bundled
  layout and demo base-layer data come from the public keyboard definition and
  keymap. The demo is an example, not a device backup.

This milestone adapts the native settings path, selected editors, offline help,
Linux access and development CI. The browser, flash/restore state machines,
complete backups, release packaging and OS helper remain later work.

## Independent findings and closure

Independent review was performed separately from implementation, before commit
and publication. The reviewer inspected firmware source, the contract, transport,
connection loader, transaction tests, Linux permission flow and CI definition.

1. **CODE hash domain:** an initial catalog entry confused the linker start
   with the packed hash domain. It now uses `[0x2800, 0xC700)`, matching the
   packer and installer reference. Raw HID usage fields also use distinct names
   instead of being presented as vendor/product IDs.
2. **Combo loading:** an initial reply reconstruction put bytes at the wrong
   offset. The loader now validates the decoded four inputs and output against
   the compact two-key contract.
3. **RGB while off:** wire mode zero hides the selected mode. Connection now
   reads the selected saved mode from NVM. RGB save compares current settings
   with persistent RGB bytes, including the disabled case.
4. **Macro assignment bypass:** bytecode structure alone did not constrain
   keycode operands. The upload now validates little-endian extended operands
   and the firmware's low-zero encoding against the assignment policy before
   querying unlock or sending any setter. Delay operands remain delays.
5. **Readiness fixture:** the successful loader model originally started with
   editing enabled. It now starts verified but unloaded, and tests the transition
   only after every required read succeeds.
6. **Transaction coverage:** focused cases cover locked uploads, rejected
   actions before transport, complete logical macro chunks, timeout without
   retry, exact readback mismatch, persistent RGB mismatch and failure latching.

The follow-up independent review found no remaining blocking issue in those
fixes. This was a scoped source review, not an exhaustive audit of inherited
Entropy code or a claim of hardware qualification.

## Verification results

| Check | Result and boundary |
| --- | --- |
| Linux x86_64 build | `cargo build --locked --offline` passed with Rust 1.97.0; development profile, debug info and incremental compilation disabled |
| Complete host suite | `cargo test --locked --offline --all-targets`: **1530 passed, 0 failed, 14 ignored**, 149.54 s |
| AK820-specific cases | 26 contract, loader and transaction tests are included in the host result |
| Firmware identity rejection | Trial/unknown manifests fail before CODE reads; a known manifest paired with synthetic all-zero CODE fails after reading the complete 40,704-byte range |
| Native UI | Built executable started in isolated Xvfb with `--demo-ak820`; layout, layer navigation and offline help inspected |
| Linux setup | Shell syntax and narrow VID/PID rule checks passed; no rule was installed during verification |
| CI definition | YAML and four-target matrix checked; remote execution and platform qualification are separate evidence |
| Documentation | EN/RU links, requirement IDs and whitespace checked |

The first full run exposed two backup tests still expecting the old `entropy`
storage directory; their assertions were updated to the application ID. A
`/proc`-dependent window-parser test failed in the isolated process namespace
and passed independently. The complete final suite was rerun with a correct
host process namespace and passed. No application logic was weakened to make
that test pass.

The 14 ignored upstream cases require explicit native rendering inputs,
production C/LVGL harnesses, a supplied replay file, desktop integration or
subprocess-harness invocation. Their ignored status is retained. Build warnings
for retained upstream/optional code remain; this is not a warning-free claim.

## What this does not prove

All AK820 transport fixtures use an in-memory responder. The successful loader
fixture begins after image verification. There is no captured positive
full-image verification fixture or real USB transaction trace in this milestone.
The negative full-verification tests and independent source review do not
establish a successful physical connection.

No physical keyboard command, setting write, unlock, reset, loader transition,
flash, reconnect or power-cycle retention check was performed. Windows/macOS
runtime behavior, installers and signing remain unqualified. No test or review
result here authorizes an automatic hardware write.

## Previous repository migration

The earlier documentation-only migration retained Entropy history and licenses,
moved configurator planning to its own repository, established branch roles and
recorded independent review. Its original record is available at
[the migration commit](https://github.com/dvino/ak-configurator/blob/df52020fd39e96776617fbc017d63a3daf7a8da6/docs/REVIEW.md).

## Local acceptance — 2026-10-25

Source commit `fb16c196106881765a44679dec494f2111cb291d` was imported into
`desktop-dev` by fast-forward. Its tree
`1307d36f82cec78ba6e1a283c88a1de33dada27c` matches the Linux package's
`BUILD-INFO.txt`. All 14 entries in the package's SHA256SUMS passed; the bundled
EN/RU desktop guides and license match the source. These checks establish
consistency, not a reproducible build.

- Source archive SHA256: `d46dab063cfa98c192ac5f3cff1d934d85d633cce07e68297b2bebed9f24b712`.
- Linux archive SHA256: `f02615b10a2129b274e09995710f6b47fcda23e117e42f0e378d4aef59db91c8`.
- Translation checks, Linux setup shell syntax, Git connectivity and diff
  whitespace checks passed. Rust/Cargo are unavailable on this host; the build
  and 1530-test result above were not repeated during local acceptance.
- A new independent source review covered code, EN/RU documentation and the
  supplied verification record. No blocking findings remained. Acceptance is
  limited to development source; hardware and release qualification remain open.

No application launch, USB command or device test was performed during acceptance.

## Companion beta.1 catalog and CI update — 2026-10-25

- Firmware reference: AK820 `v0.6.0-beta.1`; the three bundled JSON files match
  the verified release archive byte-for-byte. Profile catalog selection is
  preview-only. Beta settings writes and flashing remain disabled.
- Independent review found one documentation gap: artifact-specific download,
  checksum and launch instructions were missing. The six artifact names and
  Linux/macOS/PowerShell verification and launch commands were added to
  `INSTALL.md` and `ru/INSTALL.md`.
- Host verification: `cargo test --locked --all-targets` on Linux x86_64 with
  Rust 1.97.0: **1531 passed, 0 failed, 14 ignored**. This does not exercise a
  physical keyboard. `git diff --check`, targeted rustfmt checks and workflow
  YAML parsing passed.
- CI defines Linux and Windows x86_64/arm64 and macOS x86_64/arm64 raw binary
  artifacts with SHA256SUMS. At the time of this initial note the remote run
  was pending; its completed result is recorded below.

## Final implementation review — 2026-10-25

- A fresh GPT-6.1 Sol Low reviewer checked the full change from `19df497`
  through commit `42bad4d` and the working tree, including source, tests,
  firmware assets, workflow, and EN/RU documentation. No new confirmed blocking
  defect was found. The reviewer independently verified catalog JSON against
  the published firmware archive and confirmed beta.1 remains preview-only;
  no hardware behavior was claimed.
- The architect rechecked the finished implementation against the revised plan
  and requirements. The bounded catalog preview, six-target CI preparation and
  generic editor regression coverage were accepted. Full beta.1 companion
  acceptance remains incomplete; the blockers are listed in
  `ARCHITECTURE-REVIEW.md`.
- After adding module coverage and guarding Mouse Keys against empty or invalid
  numeric input, the full host suite passed with Rust 1.97.0:
  `cargo test --locked --all-targets` — **1549 passed, 0 failed, 14 ignored**,
  260.88 s. The 14 ignored tests remain skipped; no physical keyboard was used.
- The new cases exercise generic Auto Shift and Mouse Keys editor paths with an
  in-memory HID recorder. They do not establish those settings as supported by
  beta.1. Beta profile selection still only changes local preview state.
- GitHub Actions run [`40167663036`](https://github.com/dvino/ak-configurator/actions/runs/40167663036)
  completed successfully for commit `86f2bbf0fd26ce55b6ecadd2c0a79485e8fe195a`.
  Linux x86_64 and arm64, Windows x86_64 and arm64, and macOS x86_64 and arm64
  jobs all succeeded. All six artifacts were downloaded to the local handoff
  directory and each included SHA256SUMS passed verification. `file` identified
  the ELF, Mach-O, and PE binaries with their expected 64-bit architectures.
  These are raw development binaries; CI does not qualify runtime behavior on
  each operating system or physical USB behavior.
