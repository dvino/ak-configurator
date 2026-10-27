Русский перевод: [Адаптация и ревью](ru/REVIEW.md).

# Desktop adaptation and review

## Provisional QS v2 offline model — 2026-10-27

A separate codec requires an explicit v1/v2 target, validates the 24-byte record, CRC, reserved bytes and bounds, accepts v1 to v2 normalization with defaults, and rejects v2 to v1. Four targeted tests passed against synthetic vectors generated from the provisional firmware Python reference. Independent source review found no parity defect. The model is not connected to current device, backup or installation paths and does not qualify a release or physical behavior. Final image identity, catalog, capability flags and explicit settings transitions are required before integration.

## Offline project correction — 2026-10-27

The Linux x86_64 candidate from CI `40534916601` was checked separately: outer and internal SHA256 files passed. Its executable SHA256 `cf4b602d8a8d3b6d982ef267a55e0dafa743e62c52401089c2f647fbaed70a24` passed the Xvfb/synthetic-tray runtime smoke again in an isolated Ubuntu 24.04 container. Its manifest identifies PR merge commit `0624d3a0398dda82c65d3ba0932196db1e120cb6`, whose parents are `fb4fb7b50c89d8fb99ed5e5cdac34aa3459ac4d7` and `b535d8feb93d5bfbdbf23455583befed7fe2ff0f`; it predates the offline-project correction. No physical keyboard was used.

CI run `40534916601` failed both Windows runtime smoke checks, but the original workflow did not retain their reports; the cause is unknown. The reviewed workflow change uploads only the failure JSON separately for seven days and retains the existing success gates. The targeted workflow contract test passed; a fresh CI run is needed to obtain diagnostics.

A fresh independent review found that the first proposed full-cache undo could discard later edits and that an initial regression fixture did not make its intended edits. Both were corrected and reviewed again. The final change restores only the imported profile and preserves other profiles, including later edits. Unix nonblocking open and handle checks reject FIFOs and directories. Nine targeted Linux x86_64 tests passed in an isolated container; the debug binary passed native Xvfb runtime smoke with a synthetic tray host and no live HID. The first smoke attempt failed because the container lacked `libxkbcommon-x11.so.0`; after adding the CI runtime dependency, it passed. Windows/macOS, real desktop panels, Wayland and hardware behavior were not tested by this local run. Reads and atomic writes now run in the dialog worker; parsing and applying the project remain on the UI thread. Thirteen targeted tests passed, including injected pending, completion and disconnect UI frames; native dialogs and slow network files were not exercised.

## Independent bundle follow-up — 2026-10-27

A fresh independent source review of commit `2bd5084` covered the MCP read-only boundary, diagnostic bootstrap, stopped sessions, trace, startup and window lifecycle, tray, CI and release evidence binding. No confirmed blocking findings were identified. This review did not execute tests or establish hardware behavior. Linux Xvfb and synthetic tray evidence does not establish a real panel, Wayland, Windows, macOS or arm64 behavior.

## Handoff fixes, tray and first MCP implementation — 2026-10-27

Base: `fb4fb7b50c89d8fb99ed5e5cdac34aa3459ac4d7` on `desktop-dev`. The supplied `docs/mcp-integration-plan` bundle was merged as reviewed documentation before implementation. Application version remains `0.1.0-alpha.1`; the firmware target and ten beta.3 profile identities are unchanged. This is an unpublished source bundle, not an alpha release. [MCP.md](MCP.md) documents the implemented preview. The plan-only review below is historical.

### Changes and independent review

- The Windows tray callback now copies its relay target and releases the mutex before native window/egui calls. This removes a plausible reentrant deadlock edge; the original Windows freeze was not reproduced on this Linux host.
- Temporary OS-indicator HID ownership no longer makes the RGB scene subpage disappear/reset. Layout sampling targets 100 ms; worker completion polls at 20 ms; unchanged active override renewal remains one second. The former five-second hidden heartbeat could exceed the firmware's three-second lease. No-op samples do not move the renewal deadline or repeatedly send unchanged modes. Physical response time remains unmeasured.
- The later whole-interface flicker report was traced to borrowing HID for every sample, including no-op samples. Sampling and writing are now separate jobs. Unchanged samples retain the UI owner; necessary transitions/renewals preserve cached pages while blocking interaction. The display snapshot checks verified identity, selected device and connection generation. Review caught an overly broad RGB capability fallback; the corrected snapshot preserves actual Direct capability, with Main/Typing and stale-generation regressions.
- Auto Shift exposed inherited switches for unsupported QSID 3 because timeout QSID 4 was present. Exact beta.3 [settings source](https://github.com/dvino/ak820/blob/88d190abb156960915a0f1f8ac7f602bb401eab3/qmk-8051-ak820/keyboards/ajazz/ak820/ak820_settings.c) rejects QSID 3; its [compact runtime](https://github.com/dvino/ak820/blob/88d190abb156960915a0f1f8ac7f602bb401eab3/qmk-8051-ak820/keyboards/ajazz/ak820/ak820_autoshift.c) uses assigned on/off/toggle key actions. The page now exposes only advertised controls and allows the Main/Typing timeout without a fictitious enabled flag. Generic supported flag edits preserve unrelated bits and require readback; failures stop further edits until reread. No firmware change or physical enable/disable is claimed.
- Visible launch paints the supplied logo before full editor initialization; cold HID discovery moved from the GUI constructor to the existing asynchronous scan worker. Phase timings distinguish window creation, splash, initialization and first editor frame. Independent review caught and closed an invisible Windows startup gate that could wait forever for an unrendered splash. Native inspection also caught a first editor frame using old root colors and missing glyphs. Theme setup now precedes root UI construction, and the startup gate waits for the next font/style-ready frame. The corrected native frames and focused tests passed. README in both languages uses the exact supplied image. Disk reads before window creation and graphics/OS loader delays remain outside the deferred initialization.
- Linux now uses the existing zbus dependency for StatusNotifierItem and dbusmenu Show/Quit. A missing host prevents hiding; a bounded background monitor requests restore on watcher/host loss. macOS also refuses to hide when tray creation fails. Independent review required standard grouped menu methods and bounded fixture calls; both were added and exercised through D-Bus. Native mouse routing and real panel rendering are separate checks.
- Indicator controls moved into application settings. The redundant close-interface action is gone. Normal close follows the tray preference; explicit Quit in the main menu, tray, or Ctrl+Q/Cmd+Q respects unlock/installation and pending-write guards and cancels helper handoff. EN/RU help was updated.
- Offline GUI modes initialize fresh temporary preferences before application services and reject mixed startup modes. Reviewed HID/discovery, autostart, foreground discovery/hooks, update, capture/bridge, installer and integration entry points fail closed. The earlier test storage guards remain. Independent isolation review closed five missed production paths. Intentional loopback and fake child-process fixtures remain; inherited explicit macOS privacy-settings launch actions are outside automatic smoke. This is not universal OS isolation.
- MCP uses official pinned rmcp 3.8.0, optional `mcp` and development-only `mcp-mock`, closed DTOs and a bounded device worker. Diagnostic bootstrap identifies exact CODE through FD00 before FD01/ordinary getters and never loads settings. Six tools expose only diagnostics, synthetic checks, operation lifecycle and bounded metadata traces. Review findings about global mock policy, unrelated cancellation, terminal-state publication, error mapping, quiet trace expiry and stale lease status were closed. Standard cancellation detaches a response; explicit operation cancellation and EOF control cooperative worker cancellation.

Independent reviewers approved the isolation scope and the partial MCP source/subprocess contract. Tray/UI review found no remaining source blocker after its menu correction, with execution evidence required before integration. The successful Linux runtime evidence below closes that local condition. Windows/macOS, arm64, Wayland, live ownership/helper cancellation and physical input/LED/retention remain open.

### Executed checks

Host: Linux x86_64, Rust 1.97.0, pinned Cargo.lock. Builds used `--offline --locked`, the development profile, `CARGO_PROFILE_DEV_DEBUG=0`, and `CARGO_INCREMENTAL=0`. These are development executables, not optimized release/install qualification. An initial incremental native link failed on hidden Rust drop symbols; rebuilding without incremental cache succeeded. No source workaround or SDK downgrade was made for that toolchain/cache failure.

| Check | Actual result and boundary |
| --- | --- |
| Default-feature, `mcp`, and `mcp-mock` builds | All three native development builds passed; default startup rejects MCP, and production MCP rejects mock startup |
| Full Rust unit suite with `mcp-mock` | 1875 passed, 0 failed, 14 ignored; all 1889 listed final-source tests accounted for across four isolated serial processes (419.25 s total wall time). Ignored tests are not passes. |
| Focused final regressions | 26 passed: startup gates and initial theme, indicator ownership/capabilities, Auto Shift, stable page rendering and Ctrl+Q; all filters matched actual tests |
| MCP stdio subprocess | Both legacy `2025-11-25` initialize and modern `2026-07-28` discover/per-request-metadata routes passed; all six tools, schemas/errors, resources, traces, malformed/oversized framing, stale sessions, EOF and invalid flags |
| Production HID-backend startup | Metadata-only handshake/discovery/catalog/suite listing passed for both versions; no enumeration, open or hardware read requested |
| Native default GUI under Xvfb | Passed launch, RGB/app-settings navigation, native hide/restore, production Linux D-Bus activation/menu contract, continued rendering, and restoration after the synthetic tray host disappeared |
| Native UI inspection | EN/RU indicator controls and dropdowns inspected in the actual offline window; EN Off/RU breathing selection exercised; focused native Ctrl+Q exited with status 0 |
| Default offline self-check | All ten bundled public profiles passed, without device access |
| Release verifier | 19 Python tests passed, including missing/reordered startup evidence, missing/wrong executable-bound native reports and all six target fixtures |
| Localization and diff hygiene | `scripts/check_i18n.py`, Python syntax checks and `git diff --check` passed |
| Windows/macOS/arm64, real panel/Wayland, physical device | Not executed on this host; no pass claimed |

The first broad unit run reported 1846 passed, 8 failed and 14 ignored. Four failures came from restricted loopback/uid execution; the uid fixture also required a test executable in a traversable temporary directory. Three failures were dedicated unit-test error-text contracts shadowed by the new isolation guard; the test-specific fail-closed paths were restored. The wmctrl parser test depended on reading the current process from `/proc`; it now injects a deterministic resolver. A subsequent full run passed 1859 tests with 14 ignored. A later serial rerun had an incomplete retained log and is not counted as complete evidence. Final coverage uses every listed test from the final executable, split across four independent processes with exact name filters and one test thread in each process. Each terminal summary must account for every assigned test. Local socket/uid support is available in the isolated environment; unit-test device guards remain active. Ignored rendering/manual-fixture tests are not represented as passes.

The final Linux smoke recorded window creation at 222.555 ms, submitted splash at 718.427 ms, editor initialization from 756.490 to 1401.398 ms, and first editor frame at 1623.979 ms from process entry. These development/offline observations include a synthetic tray fixture and are not a before/after benchmark or a Windows latency measurement. The report explicitly excludes display-fence timing. A separate frame capture confirmed the logo and corrected editor appearance; Ctrl+Q exited with status 0.

Retained evidence: [all-test summary](verification/2026-10-27-mcp/unit-tests.json) and [complete per-process output](verification/2026-10-27-mcp/unit-tests.txt), [source/build identities](verification/2026-10-27-mcp/source-evidence.json), [default native GUI report](verification/2026-10-27-mcp/linux-x86_64-runtime.json), [ten-profile self-check](verification/2026-10-27-mcp/linux-x86_64-self-check.json), [MCP subprocess result](verification/2026-10-27-mcp/mcp-stdio.txt), [English settings](verification/2026-10-27-mcp/settings-en.png) and [Russian settings](verification/2026-10-27-mcp/settings-ru.png). The [startup logo frame](verification/2026-10-27-mcp/startup-splash.png), [first editor frame](verification/2026-10-27-mcp/startup-editor.png), [focused regressions](verification/2026-10-27-mcp/focused-tests.txt) and [release verifier result](verification/2026-10-27-mcp/release-verifier.txt) are also retained. Source-input hashing is specified in the evidence JSON. The runtime report binds its executable SHA256; it labels its synthetic tray host and excludes real panel/mouse/hardware qualification.

### Release and MCP limits

The candidate workflow now runs actual native GUI/tray smoke on all six configured Linux/Windows/macOS targets and the verifier requires a matching platform/version/executable-hash report. Linux CI uses an isolated D-Bus host fixture. Configuring this workflow does not establish that its new Windows/macOS jobs have run. The user requires Windows/Linux runtime before even an alpha; no release, tag, push, installation or hardware command was performed here.

The basic agent diagnostic path is usable in the exercised Linux subprocess. P0 as a whole remains partial: the positive complete packed CODE fixture, broader replay/report acceptance and live helper/lease checks remain open. The synthetic FD01 backend is not a successful full bootstrap fixture. MCP does not manipulate an existing GUI, change settings, back up/restore or flash a device. P1/P2 have not started; UI lifecycle checks use the separate native runner. Reports/traces are bounded and process-local. A `protocol.smoke` pass explicitly skips physical behavior.

## MCP implementation plan — 2026-10-27

Documentation-only scope at configurator baseline
`f8a31854bae916d1b07f24bdcf5dde1d00fdbdce`; firmware reference is
`v0.6.0-beta.3`, commit `88d190abb156960915a0f1f8ac7f602bb401eab3`.
[MCP_IMPLEMENTATION_PLAN.md](MCP_IMPLEMENTATION_PLAN.md) and its complete Russian
translation describe proposed work; they do not introduce an MCP server.
Application/firmware source, Cargo inputs, workflows and release assets remain
unchanged. No build, application test suite or device operation was run for
this documentation package.

Independent architecture/source and firmware-protocol reviews found no
architecture blocker. Their findings were incorporated:

- Separate the development mock subprocess smoke from checks of real packaged
  artifacts; production must reject mock/replay startup.
- Apply opt-in sensitive-data rules to all action tables and to report/diff
  values. Specify macro, Tap Dance and Key Override DTOs and the schema gate.
- Distinguish cancellation before loader transition, after transition/before
  erase and after erase. `NotAttempted` firmware outcome does not imply that
  normal keyboard operation remains available; no automatic Restart is added.
- Require verified unlock for matrix observations and locked/lock-lost negative
  fixtures; an untouched reply cannot pass as all keys released.
- Retain Direct's three coherent-read attempts (two generation-race restarts),
  while transport errors and uncertain writes stop without retry.
- Bilingual review clarified loss of unlocked state, clean-storage compensation,
  beta.3 transition scope, the real-device fuzzing exclusion, pinned scenario
  inputs, Cargo `--locked` and sensitive-data redaction. The remaining SDK
  alternatives paragraph was translated.

Documentation checks parsed all 14 JSON/TOML examples, matched all 16 fenced
blocks between languages, checked 58 source-reference definitions and their
pinned repository paths/line anchors, checked local links and Markdown table
columns, and confirmed matching heading counts. Current official SDK release,
tagged manifests, protocol documentation and client guidance were inspected.
These are source/document checks, not evidence of an implemented MCP feature
or physical device behavior.

## P1 beta.3 / alpha.1 integration — 2026-10-27

Baseline: `5d565380b042d3b56f7c94a6393a26e4ff534680`. Application source version
is `0.1.0-alpha.1`. The previous `0.1.0-beta.2` was unpublished development
naming; alpha.1 sorts lower under SemVer, so the version transition is explicit.
Firmware target is AK820 `v0.6.0-beta.3`, tag object
`4cabae9af9bb68492a8cf079c59f8b388c5cf788`, peeled commit
`88d190abb156960915a0f1f8ac7f602bb401eab3`. GitHub release
[425114585](https://github.com/dvino/ak820/releases/tag/v0.6.0-beta.3) was
published at 2026-10-27 06:50:35 UTC. Exact published-input provenance is
recorded in `assets/ak820/beta3-provenance.json`.

The release catalog, firmware bundle, five JSON assets and `SHA256SUMS` were
downloaded and checked against GitHub API digests. All 230 published catalog
file-manifest records were verified; all ten IHX/settings identities match the reviewed
candidates, while all ten maps use published bytes matching the release matrix.
All ten profiles have release approval pointers, and the approval records pin
the exact installer-release hashes. The published independent-review file is a
machine-reproduced CI/source-review record tied to program-policy approval; it
does not claim a new human review or hardware qualification. Firmware CI
[40461867751](https://github.com/dvino/ak820/actions/runs/40461867751) passed
the shared-host and “Revalidate and assemble all ten profiles” jobs; profile
qualification and publication jobs were skipped. The public release was
observed later. The independently reconstructed defaults for all ten profiles
have SHA256 `e95e9f8de0b81810bdcce192cba086431799e0437817184d2b9a4632cd18a0a1`.

### Findings and closures

- Apply base palette now uses the existing asynchronous RGB frame worker.
  Direct v1 palette reads and writes check cancellation before each packet.
  Transport faults stop without retry or automatic abort; only a getter
  generation race permits a bounded reread.
  Seven-LED reads and eight-LED write chunks support coherent palette readback
  and atomic full-frame commit in firmware RAM. These establish logical data
  behavior only, not physical LED scan output. The captured prior HSV palette
  can be returned by **Stop and restore effect** within the same intact session;
  a separately chosen `direct-ram.json` sidecar restores volatile RAM without
  changing the selected mode.
- An attempted settings capture failure blocks preparation even if defaults or
  OEM rollback were selected, and keeps the owner blocked until a fresh reread.
  A fresh install-only/OEM owner without loaded project settings may skip an
  unsupported settings capture, but still must pass exact full-ROM and safety
  checks. The no-serial warning is limited to the first OEM source path.
- Installer result and status text is localized. Existing beta.2/alpha.3
  identities remain separate; the beta.2 backup/reset/restore path retains its
  historical targets, including its own exact reset defaults. Beta.2 backup
  migration is available only through an explicit reviewed source-to-target
  transition, never automatic promotion. The 330 directed settings transitions
  are preserved; beta.3-to-beta.2 migration and copied NVM are not inferred from
  a profile name.
- Release observations bind the exact tag and catalog. Missing or withdrawn
  beta.3 publication (404/410) blocks beta.3 admission; a beta.2 withdrawal
  block does not carry over to beta.3.
- The unreachable `LT(layer0..5, MC0..15)` chooser was placed under
  **Special → Macro → Hold layer / tap macro**. It offers the 96 valid layer/
  macro combinations and applies context gates: physical matrix only, not a
  macro stream, Combo or encoder. Macro key labels retain the MC operand.
  Combo uses two distinct ordinary-key triggers and supports CapsWord as a
  result, with the live QSID2 timing/interval hint. Beta.2 Main does not gain
  AutoShift QSID 4; beta.3 Main and Typing do.
- The key picker was corrected for clipping at 800×500 while retaining its
  reviewed larger-window geometry. The Combo page now scrolls inside its page
  rectangle, keeping its output field and interval above the fixed footer. The
  compact profile status no longer overlaps centered key hints. Package guide
  links were rewritten and the verifier checks actual packaged bytes. The README screenshot now reflects the
  current interface. Entropy remains pinned to
  `5b222b5d6349fb351f53b36c66169095b3039fc1` with its history and
  attribution preserved.

Independent reviews covered Direct behavior, release contracts, published
assets, the existing installer, delivery and the picker. Findings were corrected
and substantive changes received fresh source review. Native RU/EN screenshots
also closed the compact picker, footer and Combo findings. Final commit-level
review includes the source, matching documents and execution evidence below.

### Verification and remaining gates

| Check | Result and scope |
| --- | --- |
| Published firmware inputs | API digests checked; all 230 catalog file-manifest records, ten IHX/settings identities, ten published maps and approval pointers verified. The provenance summary separately records 130 per-profile file records. This is public-input identity evidence. |
| Profile defaults | All ten independently reconstructed in Python; SHA256 `e95e9f8de0b81810bdcce192cba086431799e0437817184d2b9a4632cd18a0a1`. |
| Final offline Rust suite | **1826 passed, 0 failed, 14 existing ignored tests, 1 separately executed test**, in 714.83 s. Rust 1.97.0, locked dependencies, offline Cargo, all targets and serial tests. The ignored cases require dedicated Macropad/native harnesses, export inputs or explicit desktop integration conditions. |
| Restrictive-umask test | **1 passed, 0 failed** in 0.04 s. The identical final test executable (SHA256 `3b0ae9cb321313ac99debbe6e5c00d0f13a7971287dd16e6af9dbdc4b777f625`) was copied to a traversable temporary directory so its UID/GID 65534 child could run. No test or workspace permissions were weakened. Total successful Rust tests: **1827**. |
| Focused UI corrections | All 96 LT layer/macro choices and source/policy exclusions passed in the full suite. Separate compact picker checks passed 3 tests; the Combo group passed 6, including actual scrolling/clicking in RU/EN, large-window layout and loaded QSID 2 timing. |
| Optimized Linux build | Final x86_64 release/LTO build passed in 5 m 11 s. Binary SHA256 `aba64df819a44274519d9cfdc7ec0ab07f6e978a37817e90637758491ac38883`. The build retains 164 compiler warnings; it is not warning-free. |
| Runtime self-check | Final Linux binary reports application `0.1.0-alpha.1`, firmware `v0.6.0-beta.3`, the pinned catalog and all ten profile inputs/defaults; status passed, hardware/network access false. |
| Installed package smoke | A pre-commit Linux trial containing that final binary passed installation, replacement, installed self-check and removal, preserving separate user data. Its manifest explicitly records dirty source; it is verification input, not the final release archive. The deliverable is repackaged from the clean reviewed commit and accompanied by its own checksum/installed report. |
| Native interface | Actual Linux Xvfb demo inspected in RU/EN at 800×500 and 1200×700: picker title/search/close and lower-row scrolling, LT assignment and undo, Combo Caps Word selection, output/interval/clear/undo above the footer, long Russian key hints, RGB controls and offline help. No capture or keyboard operation starts from this inspection. The README screenshot was refreshed from the final binary. |
| CAVA helper | Actual CAVA 1.0.0 passed 24 FIFO frames × 16 bounded bands from synthetic sine and silence. No system capture was opened. Twelve audio-builder tests also passed. |
| Packaging and release verifier | Four package tests and 15 aggregate release-verifier tests passed. Six synthetic executable headers check package formats/architectures; they do not establish six native executions. Internal guide links and transformed document bytes are checked. |
| Localization and source | EN/RU catalog checker, 87 relative links across 18 project documents and `git diff --check` passed. |

The full suite command was:

```sh
cargo test --locked --offline --all-targets -- --test-threads=1 \
  --skip ak820_backup_files::tests::restrictive_umasks_are_isolated_and_leave_no_stage_leaks
```

The single excluded test was run unchanged from the copied executable with
`--exact --nocapture`. Its initial permission-denied launch from the private
workspace parent was an environment limitation. The successful isolated run
replaces the earlier session's restricted-umask limitation recorded below.
Final logs are `ak-configurator-alpha-release-gate-tests.log`,
`ak-configurator-alpha-release-gate-umask.log` and
`ak-configurator-alpha-release-gate-build.log`. Runtime and installed reports
are included with the delivery evidence.

Linux execution does not qualify the other five targets. The complete six-target
native workflow, Windows/macOS/ARM64 runtime and capture permissions,
signing/notarization, positive private packed-ROM fixture, physical settings
retention/recovery and Direct LED output/timing remain separate qualification.
No physical keyboard writes, reset, unlock, loader transition, flashing or OEM
rollback were performed. No actual system-audio or screen capture was started.
The application is an unpublished alpha candidate, not a six-platform or
hardware-qualified release.

## UI integration and interaction — 2026-10-27

Baseline: `3c5fb77199fa7cce090267a9fd7df243f43bcf0b` on `desktop-dev`.
Application and firmware targets remain `0.1.0-beta.2` and `v0.6.0-beta.2`.
The additional AK toolbar is replaced by Entropy navigation and shared controls.
The change includes persistent RU/EN selection before connection, complete offline
help, a dedicated OS indicator page, explicit utility-window opening, separate
profile preview/application, and four RGB sections with persistent drafts and
visible source-stop controls. The default scene view starts with the keyboard;
layer/effect details are expandable.

Interaction checks cover shared popup closure, Escape ownership, suppression of
background layer/page shortcuts, language persistence through demo startup,
profile preview without application, and RGB section changes without source
startup or loss of scene/selection/undo. Existing installer ownership and close
protection, backup/restore confirmations and device-write gates remain in place.
The UI changes do not authorize or perform physical keyboard operations.

| Check | Result and scope |
| --- | --- |
| Linux Rust build | Native debug binary built with pinned Rust 1.97.0, locked dependencies and offline Cargo. Compiler warnings remain. |
| Full default offline suite | 1787 passed, 0 failed, 14 already-ignored tests, 1 explicitly excluded environment-limited test. Run with `cargo test --locked --offline --bin ak-configurator -- --skip ak820_backup_files::tests::restrictive_umasks_are_isolated_and_leave_no_stage_leaks`. |
| Excluded test | The unchanged restrictive-umask subprocess test attempts UID/GID 65534 when run as root. This environment rejects that child launch with OS error 22 (`Invalid argument`), including outside the filesystem sandbox. The test was not weakened or marked ignored in source. |
| Focused UI regressions | Help and profile windows checked at 800×500 in RU/EN; help also checked in both themes. Language picker/persistence, F1/Escape ownership, popup arbitration and RGB draft-preservation tests exercise production UI/state. After the final visual polish, the focused AK820 suite passed all 255 tests (1547 filtered out). |
| Native visual inspection | Linux Xvfb demo at 800×500: navigation, language, Help, catalog, OS indicator and all four RGB sections. No capture, OpenRGB listener or physical keyboard operation is started by inspection. |
| Localization/source checks | English/Russian catalog checker and `git diff --check` pass. Help covers all 30 feature IDs from the bundled beta2 catalog and 13 workflow topics. |

The initial review identified independent shell/upstream menus that could remain
open together. The shared open/close mechanism now includes shell popups, and
upstream hover menus yield while a popup is open. A shared-close regression test
covers both shell IDs. Verification also exposed catalog overflow and a restored
picker-language mismatch; bounded columns/scroll areas and language initialization
fix those cases. Their regressions were rerun. The English/Russian desktop guides,
status and plan describe the same implemented behavior.

This evidence covers Linux compilation, synthetic/offline tests and the native
demo interface. Physical HID/settings retention, audio-capture qualification,
firmware installation and native Windows/macOS execution were not performed.
Independent GPT-6.1 Sol Low review approved the exact code, matching EN/RU documentation, test logs and final native screenshots for commit with no actionable P1/P2 findings. The reviewer made no edits or device operations.

## Windows audio CI repair — 2026-10-26

Baseline: `1e1f33e58dd1acc7ffbf2142bbe89910a5ff4094` on `desktop-dev`.
In [run 40342461961](https://github.com/dvino/ak-configurator/actions/runs/40342461961),
Linux and macOS passed on both architectures. Both Windows jobs failed during
FFTW pkg-config fixup, before Rust compilation: every configured mirror returned
404 for `msys2-runtime-3.5.4-2-x86_64.pkg.tar.zst`.

The workflow now explicitly prepares official native pkgconf 3.0.10 for x64 or
arm64. Its MSI size and SHA256 are checked before administrative extraction;
the executable version is checked before CMake. `PKG_CONFIG` is passed to both
CMake commands and retained through `VCPKG_KEEP_ENV_VARS`. The
[pinned vcpkg lookup](https://github.com/microsoft/vcpkg/blob/64810c6bf2bdc91ebcd75f140aec77580113f219/scripts/cmake/vcpkg_find_acquire_program%28PKGCONFIG%29.cmake)
uses that override before MSYS acquisition. The matching
[vcpkg-tool 2025-04-16 code](https://github.com/microsoft/vcpkg-tool/blob/2025-04-16/src/vcpkg/base/system.process.cpp)
preserves the explicitly listed environment variables.

The vcpkg checkout/registry, FFTW `3.3.10#10`, pthreads `3.0.0#14`, static CRT,
static target triplets, six build targets and source/license archives remain
unchanged. A failed bootstrap now also stops its PowerShell step immediately.
The standalone source kit includes the updated builder and rebuild commands.

| Check | Result and scope |
| --- | --- |
| Audio setup regression tests | 12 passed, including both Windows targets, changed MSI rejection, extraction failure, missing/ambiguous/wrong-version tools and propagation to both CMake steps. Windows processes are mocked. |
| Package and release verifier tests | 3 package tests and 15 release tests passed. These construct all six packages with synthetic executable headers; they do not execute Windows binaries. |
| Existing CI-policy tests | 45 passed after fetching complete commit history. The initial shallow checkout correctly failed the existing full-history guard; no test or guard was weakened. |
| Official MSI assets | Both release assets were downloaded; sizes and SHA256 matched the pins. Linux MSI extraction found one `pkgconf.exe` each, with PE machine `0x8664` and `0xAA64` respectively. This is archive/architecture inspection, not Windows execution. |
| Actual pinned CMake lookup | CMake 4.4.4 selected the explicit path, including a space, without entering the MSYS branch. Removing `PKG_CONFIG` reached the deliberately failing MSYS sentinel. This executes the upstream lookup on a host model. |
| Source checks | Changed Python parsed, all workflow YAML parsed, localization and `git diff --check` passed. |

The repair was not pushed or executed on native Windows runners in this work.
MSI administrative extraction, MSVC builds and installed-package execution still
need the next Windows CI run after integration. Rust/application behavior and
physical keyboard operations were not changed or tested by this repair.

Independent GPT-6.1 Sol Low review approved the 12-file repair for commit with no
blocking findings. It checked the code, workflow, failure logs, pinned CMake
lookup and matching EN/RU documents; independently repeated all 12 setup tests
and the whitespace check passed. Native Windows qualification remains open.

## Bundle acceptance — 2026-10-26

The seven commits from `c207ad1` through `5820455` were imported from a verified
Git bundle, SHA256
`470c23ba0dd133ca6bb1ac7217e0ad3680edfbe3e83a14383c1e18b6d6c46820`.
A fresh independent GPT-6.1 Sol Low review found no blocking issue in the
inspected installation, restore, ownership, RGB/audio, application-update and
delivery paths or their documented limits. Source delivery to `desktop-dev`
is approved; release and physical-device qualification remain open.

Repeated offline checks passed on an Ubuntu WSL checkout with original LF
endings: 15 release-workflow tests, 3 package tests, 3 dependency-pin tests,
3 manual-workflow tests and 10 macOS packaging model tests. One native-DMG
test was skipped. Localization and Git whitespace checks passed.
These are host/synthetic checks, not native macOS or six-platform execution.
Initial Windows discovery and Unix tests against the CRLF Windows checkout
failed on shell, symlink and line-ending assumptions; those runs are not passes.
Native Windows test compilation with Rust 1.97.0 was blocked by missing
Visual C++ `link.exe`. No local Rust suite pass is claimed. Earlier results
below remain imported evidence for their stated checkpoints.

Native CI and hardware gates remain open. No tag, release publication, GUI
launch, keyboard write, reset or flash is included in this acceptance.

## P1 RGB reuse and integration — 2026-10-26

This continuation starts from `8b1f67f41628a7ec564c97c8cfa3b156655bbdd0` on `desktop-dev`. Application version remains `0.1.0-beta.2`; firmware remains the exact `v0.6.0-beta.2` catalog. Firmware code, image identities and preset/backup formats are unchanged. Application-side music, OpenRGB interoperability and explicit previous built-in effect restoration are implemented. Direct palette readback and atomic frame commit remain firmware-dependent open items.

### Reuse and independent findings

Music uses bundled CAVA 1.0.0 at `15f89867908fa66baaaec8d4b9a2d684a329a0fb`. CAVA owns capture, FFT, frequency bands, sensitivity and smoothing. The configurator manages one hidden process, validates sixteen bounded bar values and maps them to the existing keyboard geometry. The optional loopback OpenRGB endpoint adapts the upstream SDK 0/1 wire format and never opens its own HID connection. Explicit restore preserves the actual prior built-in mode, speed and HSV in an ownership token and uses a checked volatile setter once.

Independent RGB and delivery reviewers inspected source and matching EN/RU documents. The following findings were corrected and rereviewed:

- Retained OpenRGB colors are rescaled each tick, so brightness and quiet hours still affect a client that sends one static frame. Clean idle clients are allowed; incomplete requests and blocked replies retain bounded deadlines.
- The actual Python SDK's Direct-mode update command 1101 is accepted only for the advertised descriptor. Persistent mode save 1102 remains rejected.
- Cancellation of an in-flight HID frame is terminal for its source and token. An explicit pending Off cannot reuse the retired token. Restore waits for an intact frame, yields to backup/modal work and expires across suspension or ownership changes.
- Hiding the GUI in the tray no longer stops explicit RGB playback. The existing eframe background logic provides the tick; actual GUI teardown, lightweight layout-helper handoff and Quit still stop the source. No separate RGB daemon or automatic restart was added.
- Source delivery now includes the exact helper build script and reconstructable instructions, including installed macOS Resources. Final review also caught the upstream explicit registry overriding that intended pin. The manifest, explicit registry and Windows tool now use `64810c6bf2bdc91ebcd75f140aec77580113f219`, with FFTW `3.3.10#10` and pthreads `3.0.0#14`. The unused moving artifact registry is removed. The builder checks registry agreement on every platform and the actual Windows tool/archived port recipe versions before compilation.
- All 155 files from the complete pinned CAVA archive are retained, including files hidden by upstream ignore rules. Exactly seven source files differ for the documented integration patches. Git attributes preserve source bytes across platform newline settings and retain whitespace only in specifically identified unmodified upstream files.

### Verification

| Check | Result and scope |
| --- | --- |
| Final focused RGB suite | **74 passed, 0 failed**, 10.82 s. Includes strict audio output/configuration, synthetic helper lifetime/reaping, bridge admission and loopback sessions, restore ownership, source cancellation, live quiet hours, hidden-window playback, and RU/EN controls. |
| Existing AK820 HID suite | **51 passed, 0 failed**, 0.31 s, against simulated devices. It overlaps the RGB subset; these counts are not additive. The full historical P0 suite was not rerun. |
| Final test build | Offline test executable built in 37.79 s with Rust 1.97.0, 187 warnings. |
| Original OpenRGB client | Unmodified `jath03/openrgb-python` at `7b23654ff7e0fb2e705ea5262fcc24ae2b8bf7fd` passed against the actual Rust bridge. Negotiation **4 → 1**, one controller/zone, 82 LEDs, 6×17 matrix, Direct mode 1101, controller/zone/single-LED updates and disconnect cleanup all passed. The standalone probe has no HID code. |
| Actual CAVA helper | Native Linux helper produced **24 frames × 16 bounded values** for synthetic sine followed by silence through FIFO. No system capture was opened. Rebuilding the helper from the shipped source kit with the previously built pinned static dependency prefix produced an identical binary and passed the same FIFO check. After the registry correction, the final builder again produced that identical helper, which passed FIFO and installed-package checks. |
| Package/workflow regressions | **3 dependency-pin tests, 3 package tests and 15 workflow tests passed**; the final frozen workflow run took 130.927 s. Six-platform synthetic packages do not prove native platform execution. |
| Final native Linux build and UI | Development build passed in 23.66 s, 160 warnings. Isolated Xvfb demo showed animated no-capture music preview and working source stop; controls fit 800×500 with scrolling. No real device or audio capture was opened. |
| Final full Windows check | Production/test code passed offline `cargo check --target x86_64-pc-windows-gnu --tests` with real MinGW dependencies in 20.80 s, 284 warnings. This is typechecking, not Windows execution, MSVC or ARM64 qualification. |
| Actual Linux installation | A **57-member** development trial with dirty-source provenance passed isolated install, reinstall, installed application self-check, bundled CAVA `-v`/hash verification, source/license retention and uninstall. Separate preference data survived. |
| Localization and source hygiene | Localization checker, 82 changed-document relative links, Python compilation, workflow YAML parsing and Git whitespace checks passed. No assertion was weakened or newly ignored. |

Final test executable SHA256: `98004003e2f43e8b1fbbe0d6f1f5006a41397dd119ee7e4842601e54e5b4bba6`. Native application: `9d3597f3c7ca3a35aab34b9e6205be5bd474d6383b3ceb0f091807b829192504`. CAVA helper: `7cca67854a611f1d80a030f1e0892a92c45ec1e83031a2a6ee8260074bd1527f`. The Linux trial is 43,775,173 bytes, SHA256 `8343b2e76e2fbb4c737c541d33c6617add32cdf60742d3e6b0f5ab88d878a473`. It is test evidence, not a published release. The supplied original SDK source aggregate SHA256 is `5cfc125bc13ccb3e4639429d99b7f82f3c6d971846431ceb5777eeb8cfdc118a`; the reproducible probe/test scripts are in `scripts/`.

### Acceptance limits

Real system-loopback capture, macOS permission consent, native Windows/macOS/ARM64 helper builds and execution, tray behavior on those operating systems, physical RGB timing/reconnect/retention and resource use remain unexecuted. The six-target workflow has not run remotely for these changes. No keyboard operation, firmware change, push, tag, signing or public release occurred. Beta2 has no getter for the current Direct palette and no atomic frame commit; cached requested colors are not substituted for either feature.

Final independent integration and delivery reviews approved a local commit and cumulative Git bundle from `c207ad1`, with reviewed staged patch SHA256 `34fb9e7b87ff00090d6305b814d53202f1a921275384619e729a797eba3e43c2`. Only this acceptance record and its Russian counterpart were replaced after review. The completed checks support source delivery, not hardware or release qualification.


## P0 application delivery — 2026-10-26

This work starts from `8e8ff0cd9d29956dd83d9fa9736a68d402d8c201` on `desktop-dev`. Application version remains `0.1.0-beta.2`, with the unchanged exact firmware `v0.6.0-beta.2` catalog. The work implements application update delivery, per-user installation and stronger native CI gates. It does not declare all hardware/platform release gates passed.

### Changes and independent review

The application update window now checks stable releases by default, with explicit prerelease opt-in. A complete six-target index and matching GitHub asset sizes/SHA256, project URLs and platform identities are required. Before downloading, the chosen release is checked again. A bounded, cancellable download is synced, reread and published without replacing an existing cache file. The UI displays the verified archive and installation instructions; it never launches an installer or sends a keyboard command. Application update polling now also runs in demo mode. The old unverified application download browser button is removed.

Linux, Windows and macOS candidates include per-user install/uninstall scripts. They validate package inputs, stage replacement files, preserve the previous owned installation until activation, and leave separate preferences and backups intact. Stable installation paths preserve autostart targets. The CI installation probe installs, reinstalls, checks the installed binary and public inputs, verifies preservation of local preference data, and removes owned application files. It uses isolated paths and no GUI/device operations.

The binary's `--self-check` path returns before settings, instance signalling, HID/proxy initialization and GUI creation. It checks public source/default contracts for all ten profiles and the application icon. Each native CI target must run this probe. The aggregate verifier requires exact source/binary/archive hashes, public profile identities and matching native installation reports before generating `app-update-index.json`. The pipeline retains reports and artifacts without publishing a release. macOS bundle version metadata is numeric; declared deployment minima do not establish runtime acceptance of older macOS versions.

Separate independent reviewers inspected the update/UI/self-check scope and delivery/installer scope, with matching EN/RU documentation. Corrected findings include the nested updater module path exposed by the full Windows check; update jobs left unpolled in demo; unsafe old-installation deletion and cleanup of unowned staging paths; first-install/upgrade activation rollback; a moving Linux autostart target; Windows/macOS installer failure paths; invalid macOS version metadata; archive link/drive/ADS paths; incomplete runtime identity checks; and order-sensitive comparison of profile reports produced by separate processes. Semantic negative fixtures preserve all unrelated hashes so the intended rejection stage is tested.

### Verification

| Check | Result and scope |
| --- | --- |
| Full native offline suite | **1755 passed, 0 failed, 14 ignored**, 722.09 s. All automatic host tests ran sequentially; manual/special-environment checks remain ignored. |
| New update/window/self-check tests | Seven verified-update, two update-window and three self-check cases passed and are included in the full suite. Window bounds were checked in RU/EN at 800 × 500. |
| Python workflow/package suite | **31 tests: 30 passed, 1 skipped**, 44.179 s. The existing skipped case needs a native DMG. Synthetic six-platform package checks do not establish execution on those platforms. |
| Native Linux binary | Development build passed with Rust 1.97.0 on Ubuntu 24.04 x86_64 in 24.10 s, with 160 warnings. This is not an optimized release build. |
| Executable self-check | Actual Linux binary returned a passing report for ten profiles. A preexisting report was left unchanged with exit 1; combining self-check with the helper argument returned exit 2. No GUI, settings or HID initialization occurred on this path. |
| Full Windows application checks | Production and test code passed `cargo check --locked --offline --target x86_64-pc-windows-gnu` using real MinGW C/resource dependencies, in 12.89 s and 25.08 s respectively. No platform API stubs were used. This is typechecking, not linking, Windows execution or MSVC/ARM64 qualification. |
| Real Linux package and installation | A 42-member development trial archive was packaged with explicit dirty-source provenance. Install, reinstall, installed self-check and removal passed in an isolated prefix. Preference data remained. Installed binary and archive SHA256 are recorded in the smoke result; packaged scripts and guides match the final source byte for byte. |
| Installer failure models and archive validation | Independent Linux fresh-install and upgrade desktop-publication failures retained/restored the expected prior state. ZIP/TAR drive-relative, ADS, traversal, absolute, backslash, link and special-file cases were rejected before extraction. These do not establish Windows/macOS script execution. |
| Localization/documentation | Localization keys, changed EN/RU relative links, shell syntax and `git diff --check` passed. Python cache files are ignored so tests cannot create false dirty-source packaging failures. |

Full-suite executable SHA256: `c122b03936405dde102d3e8ec5a509e929497a907c3ab6e0156aea1b836b0ed3`. Native binary: `8efa46fdeabf19dad5acb29d72f1af172d2208f3dae0017533e47a310c733c70`. The Linux trial archive is 34,854,194 bytes, SHA256 `9ba103df9b02fbc79e8add7d1fe4d9dc27bd5240ed827dc80094330410e95297`. It is test evidence, not a published release artifact.

The native build initially exposed stale copied dependency metadata; a clean build resolved that environment problem without changing dependencies. Initial cross-platform attempts lacked a MinGW compiler and a correct nested module path; both were resolved before the passing Windows checks. A macOS ARM64 full check stopped in the `ring` C build because an Apple cross compiler/SDK is unavailable. Intermediate package test runs also caught source-snapshot races and fixtures whose newly bound hashes rejected mutations before the intended semantic assertion; the final tests use the corrected frozen source. No test assertion was weakened or newly ignored.

### Release boundaries

The updated six-target workflow has not been executed remotely for this commit. Windows/macOS/ARM64 executable linking, native installer/runtime behavior, GUI/tray/helper lifecycle, physical HID/reconnect/loader behavior, persistence, resource use and signed/notarized publication still need their own evidence. No signing credentials or Apple SDK are supplied. No keyboard operation, push, tag or public release occurred. SHA256 against GitHub metadata checks integrity, not independent publisher authenticity. The local packed-ROM fixture still requires unavailable private OEM bytes; none were added.

The software changes are ready for source delivery after independent review. Full P0 release acceptance remains explicitly open for these external/native and hardware gates.

Final independent integration acceptance: approved for a local commit and cumulative Git bundle from `c207ad1`, with reviewed staged patch SHA256 `e5d9d12b9145b1e5d0910e5bdc71bbed6476e56a817cc7c109db5faaaecb40e5`. No blocking findings remain. Only this acceptance record and its Russian counterpart were added after that review. Acceptance covers source handoff and the recorded local checks, not external CI, hardware, signing or release publication.

## Native installation, reset and OS helper — 2026-10-26

This continuation starts at `1c3dc1878e2f19e3abcd16c4b0a690a420b33796` on `desktop-dev`. Application version remains `0.1.0-beta.2`, with the exact AK820 `v0.6.0-beta.2` target. Firmware code, published catalogs, image identities and upstream Entropy history are unchanged. Twenty public IHX/map files from the pinned release are bundled and checked against its manifest; no private OEM data is included. Git attributes preserve exact firmware bytes across platform line-ending settings; upstream SDCC map padding is retained.

The native installer now owns preparation, an immutable preview and one complete attempt through the existing HID helper. It verifies a fresh complete source, release admission, prepared settings and a durable safety snapshot before erase, requires the source-specific fresh confirmation, retains the physical-port lease across loader transitions, and compares the complete ROM after reconnect. Cancellation ends at erase. Firmware and settings outcomes are separate, and saved backup paths survive errors and lost workers. Defaults are explicit; supported preservation/migration must match the source-derived NVM exactly. The first bundled OEM installation can use its exact verified fresh OEM capture locally; project updates and manually prepared payloads still require a supported personal OEM reference. An invalid supplied reference never falls back silently.

OEM rollback requires a locally recorded first-capture manifest and matching nonempty serial/USB-port binding. The record is atomically published, synced and reread before erase; ordinary settings saves retain worker-added records. Missing ownership evidence blocks rollback. Backup inspection adds a default-settings reset through the guarded restore executor and separately confirmed, digest-pinned deletion of only the validated archive files. Reset preserves inactive/reserved bytes. A filesystem error can leave a partial deletion and is reported without recursion or retry.

The opt-in FD02 layout helper maps EN/RU to normal/off/on/breathing, sends bounded volatile heartbeats and yields the single HID owner to other work. Unknown layouts release the override; failures latch until a new concrete session. `--ak820-helper` and opted-in minimized startup avoid creating a GUI. Normal close releases the app/renderer and enters the helper; reopening creates the GUI in the same process and explicit Quit exits. Global tray callbacks on Windows/macOS use a current-GUI relay so they retain no closed Context/HWND. Application updates now refer to AK Configurator and remain user initiated. Portable candidates use Linux `.tar.gz`, Windows `.zip`, and macOS `.app.zip`; the workflow verifies and retains all six artifacts without publishing a release.

### Independent findings and closure

Independent reviewers inspected code, firmware references, matching EN/RU documents and the recorded evidence. Corrections include:

- Prepared-file preservation previously trusted the selected file's own settings digest. Both UI and engine now require the independently derived source/target settings bytes; malformed length and mismatching bytes have regressions.
- A newly blocked release could leave a prepared preview executable. Pending checks now prevent Start, blocked observations retire previews, and execution requires renewed admission. Authoritative changed-release evidence is retained even when another API asset lacks a digest.
- Failures after snapshot publication could hide saved paths. Both settings and ROM references survive all terminal outcomes. First-OEM ownership publication is synchronous and durable before erase, and stale preference saves cannot discard its pin.
- SDCC maps truncate some unused long names. Those collisions no longer reject the ten public maps; every symbol consumed by the packer must still be unambiguous. The default NVM for all ten profiles matches independently derived public reference hashes.
- A first OEM installation unnecessarily required a preexisting dump. The bundled request now accepts the exact fresh validated OEM capture only; project sources and invalid supplied inputs retain strict rejection.
- The proposed tray cleanup used `set_event_handler(None)`, which the pinned crates' `OnceCell` cannot apply after registration. A process-once handler and cleared current-GUI relay replace it. Exact isolated Windows/macOS compilation and a fresh independent source pass cover this correction.

### Verification

| Check | Result and scope |
| --- | --- |
| Full sequential offline suite (`test-full-final.log`) | **1742 passed, 0 failed, 14 ignored, 0 filtered**, 675.77 s. Device/manual checks remain ignored. The unchanged executable ran from a traversable temporary path so the private-directory test could use its unprivileged child. |
| Final focused AK820 suite (`test-ak820-last.log`) | **174 passed, 0 failed, 0 ignored**, 18.88 s, including private-directory UID-child checks. This executable includes the final platform-only tray relay correction and the additional prepared-settings comparison regression. |
| Python workflow/package checks (`test-python-last.log`) | **26 tests: 25 passed, 1 skipped**, 29.183 s. The existing skipped case requires a native DMG. Six-platform package tests use synthetic executable headers and do not establish target execution. |
| Native Linux build (`build-native-final.log`) | `cargo build --locked --offline` passed on Ubuntu 24.04 x86_64 with Rust 1.97.0, in **19.69 s**, development profile with debug information and incremental compilation disabled; **136 warnings**. This is not a new optimized release build. |
| Platform adapter checks | Isolated current Windows GNU and macOS arm64 physical-port/ownership typechecks passed; Windows includes the current backup deletion module. Exact current tray relay methods also compiled on both targets against pinned `tray-icon 0.22.2`, `muda 0.17.2` and `egui 0.34.3`. Parent types are stubbed; these are not full-app builds, links or platform runtime tests. |
| Native demo UI | The native binary ran in isolated Xvfb with `--demo-ak820`. RU/EN installer controls and first-OEM/defaults guidance fit at **800 × 500**. Demo discovery guards and absence of HID/USB file descriptors were checked. Ordinary `WM_DELETE_WINDOW` closed the app. |
| No-device GUI/helper lifecycle | In an isolated profile with no hidraw nodes or sysfs HID class, explicit helper startup created no window. Two open/ordinary-close cycles retained the same process and recreated/released the GUI. Disabling the helper preference ended it successfully. This checks Linux process/window behavior without a device, not physical LED behavior or resource qualification. |
| Real Linux trial archive | A development-binary `.tar.gz` was packaged with `--allow-dirty`. All **39 members**, including **20 public firmware files**, and internal/outer SHA256 values were checked. Its manifest explicitly records dirty trial provenance; no published candidate is claimed. |
| Localization, documentation and whitespace | `scripts/check_i18n.py`, current relative documentation links and `git diff --check` passed. EN/RU installation commands match archive layouts; shell examples passed syntax checks. PowerShell examples were not run on Windows. A `core.autocrlf=true` checkout preserved all 26 pinned firmware files byte for byte. |

Full-suite executable SHA256: `3895f002178eb913adc57af3b90426293a82ce9e67e480da7dbd6cc4bebdd83c`. Final focused executable: `93978d4063a63e496699ce6097c02d4e64ae497db67ff594500af32f21db0f7f`. The broad run precedes only the Windows/macOS-only tray correction and one added pure regression; Linux production behavior is unchanged by that delta. Native UI/helper binary: `8fd66e041c97fcd974b1b211089c283f54fac87f092c747e0ea8f4fbbcb83ee6`.

Initial restricted-sandbox UID-child execution and Xvfb socket binding failed because of the execution environment. The unchanged offline test executables were copied to a traversable temporary path and run with the unprivileged child supported; virtual-display checks used an isolated local display with the required socket permission. An initial helper screenshot attempted to capture a not-yet-visible window; the completed lifecycle check waits for a visible window. No test assertion or source ignore was weakened.

### Remaining qualification

No keyboard setting write, reset, loader transition, erase, flash or OEM rollback was performed. A positive released packed-image/full-ROM fixture still needs unavailable private OEM erase-helper bytes; public defaults and synthetic transport models do not substitute for it. Actual port/serial behavior, physical hold, flash/retention/readback, minimum OS versions, timing, resource use, complete six-target builds/runtime, signing and browser work remain separate gates. Latest-stable/history browsing and arbitrary custom firmware are outside the pinned beta2 admission path. No push, tag or release publication occurred.

Final independent integration acceptance: approved for a local commit and cumulative Git bundle from `c207ad1`, with reviewed staged patch SHA256 `c180191aeb69b02b133c1455b4844f459899c4ff634e5a3f7baa86adca32af0e`; this acceptance covers source handoff and does not qualify hardware, release publication or full platform runtime.

## Guarded restore and desktop integration — 2026-10-26

This checkpoint starts at `bbd18ef6bfeb86422e7696d69701d3fa43f203f4` on `desktop-dev`. Application version remains `0.1.0-beta.2`; firmware target remains AK820 `v0.6.0-beta.2`. No firmware catalog, archive identity, or release version was changed.

The backup inspector now connects immutable restore planning to a guarded executor. Before writes, the executor captures the complete current ROM again and requires its exact image identity and complete NVM to match the preview target, creates and verifies a durable safety backup, requests a fresh Esc + left Control three-second hold through the lock/start handshake, and reopens the pinned source backup immediately before the first setter. Only whitelisted setters are available; each is attempted once, then checked by acknowledgement, applicable getter and full NVM. Macro publication is checked through its published getter. Final verification compares the actual full ROM, getters, repeated NVM and manifest. A failure or cancellation stops without retry or automatic rollback. Read-only sessions refuse restore. Cancellation retains worker ownership; after errors the safety-backup reference remains available for explicit undo/reopen, while success rereads editor state. Stopped RGB is not resumed automatically. A cooperative per-device guard is independent of the GUI singleton; external Vial software does not honor that advisory guard. No physical restore or keyboard write was performed.

Backup filesystem operations now have Linux, macOS and Windows implementations. Unix operations are anchored to open directory handles; macOS rejects parent ACLs. Windows uses local-drive handle-relative operations, rejects reparse points/hard links, applies a private user/SYSTEM ACL, syncs files and publishes without replacement. Windows does not promise Unix parent-directory fsync semantics, and UNC/device paths are unsupported. Isolated Windows GNU backup/ownership test typechecks and the macOS arm64 anchored-module typecheck passed. They are not full-application builds or platform runtime checks.

The new Firmware files window validates three explicitly selected local files: an exact 51,136-byte profile-catalog payload, a 65,536-byte supported OEM/project snapshot, and a personal 65,536-byte OEM recovery ROM whose protected regions match the snapshot. It keeps copies of the selected bytes while open and reports hashes, expected full-ROM hash, loader checksum and block count. A bounded pure OEM loader wire codec is present. Neither component flashes a device. Live loader transition and reconnect binding, release/revocation authority, durable installation safety and full-ROM write/readback transaction, and separate settings-outcome handling remain unimplemented. The positive released full-ROM fixture is still unavailable because private OEM erase-helper bytes are unavailable; no private OEM bytes were added.

The separate unsigned candidate workflow is DRAFT-only; its verifier has nine new tests. No publication, tag or push was performed. The six-target workflow now includes Windows/macOS tests. The test run evidence is:

| Check | Result and scope |
| --- | --- |
| Full sequential offline suite (`test-current-full.log`) | **1707 passed, 1 failed, 14 ignored, 0 filtered**, 624.80 s. This executable precedes the final read-only guard regression, installer/checker modules and UI additions. The only failure was the umask/UID-child test: the unprivileged child could not traverse the workspace path (`PermissionDenied`). This is not a zero-failure full-suite run. The same executable, SHA256 `3474aa9916b2fe0c71f96b090ee99cd1a88b49e1b50fd0fc5577018879e3ff05`, was copied to `/tmp`; the exact test passed there (`test-full-umask-rerun.log`). |
| Final focused AK820 suite (`test-ak820-final.log`) | **201 passed, 0 failed**, 13.77 s, with the umask case filtered. Final test executable SHA256: `e1cce9a88c0785aae2d2eae1dc16d2a24db945f03937e17de672f532277a3376`. |
| Final umask and ownership reruns | Same final test executable: umask **1 passed**, 0.02 s; ownership **2 passed**, 0.00 s. |
| Python release/packaging tests (`test-python-final.log`) | **25 tests: 24 passed, 1 skipped**, including nine new candidate-verifier tests and two packager tests. The first invocation lacked `PYTHONPATH`; the corrected invocation passed without code changes. The skipped case requires a native DMG. |
| Platform adapter source checks | Windows GNU backup/ownership test typechecks and macOS arm64 anchored filesystem typecheck passed. No full-app cross-target build or runtime is claimed. |
| Localization and whitespace | `scripts/check_i18n.py` and `git diff --check` passed. |
| Native app build | Final `cargo build --locked --offline` passed with Rust 1.97.0 on Ubuntu 24.04 x86_64 in **18.50 s**, development profile with debug information and incremental compilation disabled; **169 warnings**. This is not an optimized release build. Binary SHA256: `f2f0b6750dfbede90756140b24185b23d3b023b96b366b7f07914e2fb90df819`. |
| Native offline UI | The final binary ran with `--demo-ak820` under isolated Xvfb/software rendering at **800 × 500**. Russian Firmware files and Backups windows fit; the file-picker fallback opened, a 17-byte test input produced the wrapped localized size error, and physical capture was disabled in demo. Both languages also passed the window-bound tests. No positive image admission, complete backup publication or physical HID session was exercised. |

During UI cleanup, externally destroying the X11 window with `xdotool windowclose` caused an upstream winit `BadWindow` panic. A separate launch of the identical binary followed by the ordinary `WM_DELETE_WINDOW` message exited successfully with code 0. No application code changed for this check; forced external X-window destruction is not qualified.

Independent review corrections closed in source and regression checks include: restore setters could previously reach the raw transport despite a read-only session; macro intermediate NVM expectations now model the firmware busy byte and retained length until publication, and the published macro getter is checked before later setters; capture now maps the encoder getter's CCW/CW order to the saved CW/CCW order; the macOS anchored call had a C variadic argument mismatch; Windows stage creation now retries only a genuine name collision, obtains the file write access required by FlushFileBuffers, and removes a newly created stage by its handle if metadata validation fails; the release workflow needed explicit `--bin`, immutable commit identity and independent candidate verification; and UI state/localization needed nested-`Option` preservation plus localized actionable errors. These corrections were re-reviewed; the focused tests above include the relevant source checks.

### Remaining software and acceptance boundaries

The local input checker and wire codec are preparation only, not an installer. Required software work still includes binding a live loader transition to the owned HID session and verified reconnect, defining trusted release and revocation authority, and implementing a durable safety/readback transaction with one-attempt behavior, cancellation boundaries, installed-image verification and independent settings outcome. Installer/OEM recovery therefore remain incomplete. A positive released full-ROM fixture cannot be generated from public Intel HEX without unavailable private OEM helper bytes. No physical keyboard test, restore, firmware write, push, tag or candidate publication occurred. Current tests establish source/model behavior only.

Final independent integration acceptance: approved for local commit and an incremental Git bundle from `c207ad1`; reviewed staged patch SHA256 `411a14f26661569817a8377962d25e03db48605fed1db450b5439e3e1ff363b9`. No blocking findings remain. Only this acceptance sentence and its Russian counterpart were replaced after review. Acceptance covers source delivery; it does not establish physical qualification, full cross-platform runtime support or a published release.

## Complete backup capture and restore preview — 2026-10-26

This continuation starts at `06425024a28a4e32c4b41300e4b4199c6b6b9b0b` on `desktop-dev`. Application version remains `0.1.0-beta.2`, targeting the existing exact AK820 `v0.6.0-beta.2` catalog. Firmware, released image identities and the archive formats are unchanged.

The Backups window now connects read-only capture to the existing sole HID owner. A loaded beta2 session reads the complete 64 KiB ROM, runs the production protected-region/CODE/manifest validator, reconciles supported getters against saved settings, and repeats all 10 KiB of NVM, the manifest and storage status. Inactive durable bytes remain in the complete snapshot. Live RGB is validated separately and never substituted for saved RGB. The read log contains fixed metadata, not packet contents, macros or device paths. Cancellation is checked before each request; transport or validation failures stop without retry and latch session failure.

An accepted snapshot remains in memory until explicit save. Linux publication uses the existing private staged directory, reread validation, no-replace rename and parent-directory sync, followed by a pinned reopen. Auto-detection admits only the ten current exact sources or the three historical schema02 source identities. History is bounded to 128 immediate directories and 1024 root entries; invalid candidates are counted, and selected entries are validated again against their listed manifest digest. The restore view invokes the existing immutable planner and reports proposed changes against the captured target snapshot. No restore setter, unlock, reset, loader transition or installer is connected.

### Independent findings and closure

A separate GPT-6.1 Sol reviewer inspected capture, filesystem I/O, UI ownership, cancellation, and matching EN/RU documentation without authoring the changes or operating a keyboard. Corrected and re-reviewed findings include:

- History originally enumerated through an open directory but reopened candidates by pathname. It now reads each child relative to the retained root handle. A structural rename/replacement test distinguishes original and replacement bytes without admitting synthetic firmware.
- A borrowed digest in a new test had the wrong type; the compile error was corrected.
- Cancellation arriving beside a real read failure could suppress the failure latch. Only a typed cancellation outcome now bypasses the latch; a transport/cancel race regression checks that genuine failure remains visible.
- Retiring a connection originally discarded the worker receiver without cancelling capture. Retirement now signals cancellation and retains ownership until the worker finishes. Late completions cannot restore the old handle or snapshot; a queued connection resumes afterward.
- New native-only helpers initially lacked two Wasm cfg guards. Both are guarded; this is source review, not a Wasm build claim.
- The initial zero-ROM tests stopped before getter reconciliation. A private helper now has synthetic matching and mismatch tests for keys, both encoder directions, multiple macro chunks, runtime counts/QSIDs, typed entries, Combos and RGB. Production still validates real ROM before calling it; no production trust substitution was added.

### Verification

| Check | Result and scope |
| --- | --- |
| Full default offline suite, sequential | **1697 passed, 1 failed, 14 ignored, 1 filtered out**, 668.93 s. The existing umask test was run separately as described below. The single failure was the existing `/proc`-dependent window-parser test; the same executable passed that exact test outside the mismatched PID namespace. This is not a zero-failure full-suite run. This executable precedes only the final native cfg guards and UI/help text changes. |
| Final focused AK820 suite | **142 passed, 0 failed**, 12.17 s, after the final source changes; the umask case was filtered from this restricted-sandbox run. Coverage includes fixed read-only requests, cancellation before/between packets, transport/cancel failure latching, untrusted full-ROM rejection, synthetic getter reconciliation, history bounds/anchored reads, stale completions, connection retirement and RU/EN window bounds. |
| Private-directory umask test | The initial targeted run could not create its UID-changing subprocess in the restricted sandbox. The identical full-suite executable was copied to a traversable temporary directory and verified by SHA256; the exact test then **passed, 0 failures**, 0.02 s, outside that sandbox with its unprivileged child. Assertions and source-level ignores were unchanged. |
| Window-parser environment check | The sandbox process saw its inner PID while mounted `/proc` exposed the outer namespace; numeric `/proc/<own-pid>` lookups failed. `app_discovery.rs` is unchanged. The exact `app_discovery::tests::parses_wmctrl_process_and_title` test **passed, 0 failures**, 0.00 s, from the same full-suite executable outside that namespace. |
| Localization and whitespace | `python3 scripts/check_i18n.py` and `git diff --check` passed. |
| Native build | Final `cargo build --locked --offline` passed with Rust 1.97.0 on Linux x86_64 in **19.52 s**; development profile with debug information and incremental compilation both disabled. The build reports **176 warnings**. This is not a new optimized release build. |
| Native offline UI | Final binary launched with `--demo-ak820` in isolated Xvfb/software rendering. The Russian Backups window shows the selected folder, counts one intentionally incomplete directory, refreshes to zero after its removal, and fits at **800 × 500**. Folder selection, close/reopen and the disabled device-capture control in demo were inspected. The existing system-dialog fallback worked without an XDG portal. No physical HID session or successful complete-backup save was exercised. |

The full-suite executable SHA256 is `29ed603be7463c9a1a9978e37dae8a3bd49863c3842102c53f562821ec338483`; both environment-dependent exact reruns used its verified copy. The final focused-test executable is `f5b2115b5abede869ec2a1ced4db8dd954539228d65f1926fde248b74640c9a7`; the final native binary is `5ebe7583ab328264b168cf193f3bc74e82b2c9c9cf6b3a3d655eed4c6b9c93d3`. These distinguish the broad run from the final text/cfg corrections. No test assertion was weakened and no new source-level ignore was added.

### Remaining boundaries

No positive released full-ROM fixture is available because protected OEM bytes are unavailable. Synthetic getter tests establish parsing and ordering, not successful admission of a released keyboard ROM. No physical device was connected or operated. Archive file I/O remains Linux-only. Windows/macOS/arm64 runtime, current-source six-target CI, fresh physical restore confirmation, pre-restore safety backups, execution/undo, installer/OEM ownership and release qualification remain open. No release tag or GitHub publication is made.

Final independent integration acceptance: approved for local commit and an incremental Git bundle from `c207ad1`; reviewed staged patch SHA256 `55ec4a7ec17b832466fce70121a8c286fcf38558e4d0cb0c128e2fac8b00f2cd`. No blocking findings remain. This acceptance does not authorize hardware operations, GitHub publication or release qualification.

## Native RGB scenes — 2026-10-26

This continuation starts at `588aeed963884de8ba8b673c037d1cd27639cfcb` on `desktop-dev`. Application version remains `0.1.0-beta.2`; firmware target remains `v0.6.0-beta.2`. The firmware tag is annotated: its object is `2e0b28b384872c482e0e358a4b353f00ddd976a7`, and its peeled commit is `2990ef877c5c6c54013b8821450c5a91878b8c3a`. Provenance, packaging and current documentation now distinguish those identities. Published catalog bytes are unchanged. No firmware code is changed.

Scene format v2 adds up to 16 ordered/enabled layers over the existing 82-color base palette, LED masks, Solid/Gradient/Wave/Breathing/Rain, horizontal/vertical axes, opacity, 250–60000 ms periods and a local-time quiet-hours brightness cap. Static palette v1 remains compatible. The editor provides bounded 32-state undo/redo, explicit scene save/load and animated offline preview. Selection remains click/Ctrl/Cmd/Shift based; drag selection, audio, OpenRGB and game/screen inputs are future work.

Playback uses the existing HID owner, one pending frame and at least 100 ms between completion and the next dispatch. The 10 frames/s ceiling is an application bound, not a measured device rate. Settings, backup, connection and modal work take priority. Cancellation is checked between packets. Generation/profile changes, disconnect, failure, monotonic or wall-clock gaps over two seconds, and backward wall-clock jumps stop playback without automatic restart. A pending Off request expires across the same gaps. Mode/speed/HSV changes relinquish ownership. Explicit Off sends a black frame only on an intact session. There is no automatic effect restoration or per-frame NVM save.

### Independent findings and closure

Separate GPT-6.1 Sol reviewers inspected the compositor/editor, transport/lifecycle, and EN/RU documentation. They did not author the changes or operate a keyboard. Corrected and re-reviewed findings include:

- Import or undo could leave the active layer outside the current layer list. Selection is normalized, and reorder actions are bounds-checked.
- Offline scenes had no working playback control. Demo now uses the same renderer without opening HID; new effect, axis and validation labels are localized.
- A short priority pause could erase the post-frame dispatch deadline. Resume now preserves the deadline, with a deterministic regression.
- A queued Off request could survive a long suspension. Monotonic and wall-clock expiry are checked before creating playback, with independent regressions for each clock.
- Modal-pause documentation initially exceeded the implementation. The runtime now checks the shared modal/popup state; a key-picker regression checks no requests while open and a fresh frame after closing.
- The Russian release-boundary section was empty. It now matches the English boundary and clock/Off semantics.
- Visual inspection found the layer settings clipped in a narrow window. Controls and selection guidance now wrap; the minimum supported width is checked in the offline UI.

The Off completion test was corrected to advance its injected clock beyond the retained cooldown. No production deadline was relaxed to satisfy the test.

### Host and Linux offline verification

| Check | Result and scope |
| --- | --- |
| Full default offline suite, sequential | **1683 passed, 0 failed, 14 ignored**, 621.79 s. This executable precedes only the final control-wrapping changes in the scene editor. No device/manual checks were enabled. |
| Latest focused RGB tests | **35 passed, 0 failed**, 0.29 s, after the final wrapping changes. Compositor validation and boundaries, time-dependent effects, quiet hours, queue replacement, cooldown, pause/resume, modal priority, cancellation, stale sessions, Off and ownership are covered. |
| Transport fault models | Included in the full suite: all ten Direct packet fault boundaries stop without retry; read-only sessions reject writes; ownership changes and cancellation before/between packets are covered. Echo ACK is not palette readback. |
| Private-directory umask test | Passed in the full suite. The identical test executable was copied to a traversable temporary directory with its digest checked, then run outside the restricted execution sandbox so its UID-changing child could execute. Assertions and ignores were unchanged. |
| Firmware geometry | All **82** compositor LED coordinates match `ak820_led_points[]` at the peeled firmware commit. The encoder press remains excluded. |
| Python candidate packager | **2 passed**, including separate annotated-tag and peeled-commit provenance. Synthetic binary headers establish packaging rules, not target execution. |
| Existing workflow policy tests | **45 passed**. |
| Localization and whitespace | `python scripts/check_i18n.py` and `git diff --check` passed. |
| Native build | `cargo build --locked --offline` passed with Rust 1.97.0 on Linux x86_64 in 16.92 s; development profile with debug information and incremental compilation disabled. This is not a new optimized release build. The build reports 194 warnings. |
| Native offline UI | Launched with `--demo-ak820` in isolated Xvfb/software rendering. Before the final layout-only correction, animated preview, two-layer scene import, explicit save, and exact parsed-JSON round trip were checked: **82 colors, two layers, 1364-byte saved file**. The final binary was relaunched; all layer controls and the complete selection guidance were inspected at **800-pixel** width. No physical HID session was opened. |

The full-suite executable SHA256 is `d5634df8f7589ff10272e01a3c8560e6c98a627199d8fd9195c6101cae3eb2ae`. The latest focused-test executable is `3870b6cf71353ac33a6d5d97f08bdf858b6b2cad8795ba64dc4c313888acd9bd`; the final native binary is `4643e3e3743f18a73776dbd6aae4b760c54546bde266455de597023564ad6d49`. These distinguish the broad run from the final visual correction.

### Remaining qualification

Source/model tests do not establish physical HID timing, visible smoothness, tearing, partial-frame behavior, RAM state after disconnect, or NVM behavior under physical hotkeys. The firmware has no Direct palette getter, atomic frame commit, lease/watchdog or persistence. Stop can leave already-sent colors or a partial frame in RAM. No physical keyboard commands, settings writes, reset, loader transition or flashing were performed.

Current-source Linux offline launch is the only platform launch in this continuation. CPU/memory and complete HID-cycle measurements, Windows/macOS/arm64 runtime checks, current-tree six-target CI, installers, signing and the remaining backup/install gates stay open. No release tag, GitHub publication or fully qualified first release is claimed. Final independent integration review approved this local commit and the incremental bundle from `c207ad1`; reviewed staged patch SHA256: `c383e11b9c629f0bb0681af5574611a7de16946fbb0ed3eabca9c686a5dad975`. Replacing the pending-review sentence in EN/RU was the only permitted closure edit after that review.

## Beta2 desktop candidate — 2026-10-26

The reviewed work starts at `c207ad13f3529bebccdbb8df15a7c38cfd96c1eb` on `desktop-dev` and prepares application `0.1.0-beta.2` for firmware [v0.6.0-beta.2](https://github.com/dvino/ak820/releases/tag/v0.6.0-beta.2), peeled commit `2990ef877c5c6c54013b8821450c5a91878b8c3a` (annotated tag object `2e0b28b384872c482e0e358a4b353f00ddd976a7`; identity terminology corrected in the RGB continuation), published at 04:52:40 UTC. The five bundled JSON catalogs are byte-identical to the published release assets. Their sizes, hashes and URLs are recorded in `assets/ak820/beta2-provenance.json`; the prequalification catalogs in the firmware tag are not substituted for those assets.

The packet adds ten exact schema03 profile contracts, complete read-only connection/reconciliation, profile-aware typed writes and keycode choices, project v2, exact historical backup sources and RGB-mode migration, the static 82-LED palette editor, bilingual help/research/requirements, and versioned six-platform candidate packaging. The exceptional architecture handoff requested in the prior checkpoint was completed before this packet; it is no longer a prerequisite for the next agent. Earlier handoff instructions below describe historical checkpoints.

### Independent findings and closure

A separate GPT-6.1 Sol reviewer inspected code, firmware contracts and matching EN/RU documentation without authoring the changes or operating a device. Findings corrected and re-reviewed:

- The first historical schema02 source (`f1c434…` IHX / `6ff7b1…` packed CODE) has no Combo module. Its explicit registry flag now drives decoding and restore policy. Nonzero reserved Combo bytes are not silently reinterpreted during raw migration.
- Candidate ZIPs now use a separate workflow artifact, preserving the raw binary artifact's extraction layout and existing install commands.
- Requirements, help and status now describe ten beta2 profiles, the actual active QSIDs, source-only historical identities, and 82 Direct colors in ten packets.
- The inherited Key Override all-layer mask is normalized to six bits. Intentional trigger clearing produces a completely zero record. Incomplete drafts remain local; prevalidation of all eight rows prevents frame-driven flushes from reading NVM or sending siblings while a draft is incomplete. Regression checks exercise eight rows and repeated pending flushes.
- Tap Dance output choices and tapping-term limits follow the typed firmware contract. Invalid selections leave the entry and undo history unchanged. The palette is shown only for the four Direct profiles, with an independent transport gate.

### Host verification

| Check | Result and scope |
| --- | --- |
| Full default offline suite, sequential | **1650 passed, 0 failed, 14 ignored**, 793.91 s. This test executable precedes only the final eight-row Key Override preflight correction; the latest AK820 run below covers that correction. No ignored device/manual checks were enabled. |
| Latest AK820 tests | **136 passed, 0 failed**, 15.83 s, after the final batch-preflight correction. Covers exact profiles, modeled connection failures, typed full-NVM preservation, historical restore, Direct packets/fault latching, presets, offline projects and picker regressions. |
| Private-directory umask test | Passed in the full suite. The test executable was copied to a traversable temporary directory, with its digest verified, and the privilege-changing child ran outside the restricted execution sandbox. No test assertion was weakened. |
| Python candidate packager | **2 passed**. Tests header/architecture mismatch rejection, package contents and retained licenses, provenance/checksums, executable mode and refusal to overwrite output. Synthetic headers do not establish runnable cross-platform binaries. |
| Existing workflow policy tests | **45 passed**. |
| Native optimized build | `cargo build --release --locked --offline` succeeded with Rust 1.97.0 on Ubuntu 24.04 x86_64 in 4m 37s. Warnings remain; this is not a warning-free build. |
| Native offline UI | The final release binary launched with `--demo-ak820` on Xvfb/software rendering. Keyboard geometry, group selection, color editing, explicit demo apply, preset save and reload were inspected. Main shows the palette; switching to Typing hides it and retains built-in RGB controls. The saved preset contains exactly 82 colors and only the four WASD slots changed. `docs/images/ak820-beta2-rgb-demo.png` records the final binary's local preview. |
| Real Linux candidate archive | Built from the final native binary, with `--allow-dirty` during review. All 13 archive members and the outer ZIP checksum were checked; manifest records the dirty local trial explicitly. Final clean-source packaging is performed after commit. |
| Whitespace | `git diff --check` passed. |

The full-suite executable SHA256 was `f3d10584c5e789d1c3f73b414051c0dc8df58923a5f5b532e02b82cb70108d9b`; the latest AK820 test executable is `91b346315602e3dab8aeaa4ba8d41191f7984046a3558159b59e1cc750f6a898`. The native release binary SHA256 is `4ec909b4a3b39df2e02ba9a570517ff55cd54561557c39f33bd0704c8259e1d6`. These distinguish the broad run from the bounded final correction rather than presenting both as one identical binary.

The inherited helper tests can time out under parallel suite contention; the workflow now runs them sequentially. The initial restricted-sandbox umask failure was an execution-environment limitation. Neither issue was suppressed by changing assertions or adding test ignores.

### Remaining release evidence

There is no positive released packed-image/full-ROM fixture: public Intel HEX lacks the OEM erase-helper bytes required by the packer. Private OEM data is excluded. Synthetic verified sessions and digest-admission tests establish host rules, not acceptance of a physical keyboard. No hardware writes, reset, loader transition or flashing were performed. HID timing, retention, physical input, real backup/restore and installer execution remain open. Direct palettes have no getter or persistence and are not included in firmware backup data.

The workflow and package script cover six architecture targets; this session built and launched Linux x86_64 only. Windows, macOS, arm64 runtime checks, current-tree six-target CI, installers and signing remain open. The Linux binary uses the Ubuntu 24.04 runtime; older Debian compatibility is not implied. Candidate packaging is not a fully qualified first release.

Final independent review approved this local candidate commit and incremental bundle from `c207ad1`, with the stated release limits. The reviewed staged patch SHA256 was `3486ef281eaecf15d2cfff1e1d35adabf11e127ba047289d38e4320cd8924749`; replacing this approval sentence was the only permitted closure edit after that review.

## Historical evidence before this packet

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

## Offline project packet — 2026-10-25

The packet adds per-profile settings contracts, source-aware offline projects,
schema03 typed codecs and panels, local RGB controls, and backup envelope and
NVM/settings models. It does not enable beta device writes or flashing. Eight
packed identities and the transition list remain incomplete; schema01 settings
derivation remains blocked pending an explicit migration. At that checkpoint, `src/ak820_restore_plan.rs` was unregistered and excluded
from those results. Its subsequent integration is recorded below.

Independent reviews found and closed backup manifest/identity/schema admission
issues and UI state, picker, TD assignment, window reopening and layer-name
synchronization defects. The latest GPT-6.1 Sol Low review found no blocking
issue, including in the subtitle handling. These are source and UI findings;
they do not qualify a physical keyboard.

The earlier full host run passed **1590, failed 0, ignored 14** in 233.69 s;
it preceded the final demo lock fix. The latest focused run passed **60, failed
0** in 5.67 s. The final full suite passed **1593, failed 0, ignored 14**
in 250.55 s (`cargo test --locked --offline`, Rust 1.97.0). The ignored tests
remain skipped. Source fingerprint over the 24 changed/new source paths
(excluding the unregistered next packet) is
`d9794cbe25171f5bf2a840762b638cf1572a3168fe5fc078f83568569c47d39a`.
Final independent integration and EN/RU review found no blocking finding.
The native development build passed in 21.35 s with 139 compiler warnings;
it was started again successfully in the isolated demo display.

The application was launched on Debian 12 x86_64 under Xvfb with software X11.
That run exposed a missing `libxkbcommon-x11-0` dependency and horizontal
catalog card overflow; the dependency was added and catalog cards now scroll
vertically with bounded windows. Live GUI use also exposed a footer window
intercepting clicks; moving the footer to the background fixed the regression.
Profile selection, expanded and scrolled typed panels, panel reopening, macro
creation and typing `OfflineTest` were exercised. The macro demo lock/menu and
scheduler early-return defects were fixed. No unlock was requested or sent.

The minimal container initially had neither an XDG desktop portal nor `zenity`,
so the first dialog request produced no visible dialog. After installing `zenity`,
the native Save dialog wrote a 13,332-byte `.ak820project.json` file and the
Open dialog imported it successfully and enabled Undo. No USB or hardware was
used.

At that stage, complete backup capture/history, migration, guarded restore
execution, installer/OEM execution and runtime qualification across target
platforms remained open.

## Restore, image and backup-directory models — 2026-10-25

The follow-up registers `ak820_restore_plan`, `ak820_image_pack`,
`ak820_install_plan`, `ak820_backup_files` and a read-only backup inspection
window opened from the status bar. It enables no beta device write or flash.

The restore plan is catalog-bound, with exact schema02-to-schema03 migration
and declarative per-setter preconditions. Schema01 is rejected. The image packer
matches pinned Python permanent/trial output hashes on public synthetic inputs;
its Intel HEX record separators match Python. The install simulation preserves
all nine unmet hardware gates, requires a fresh three-second hold for project
firmware, permits button confirmation only for OEM, rejects cancellation after
erase without changing state, allows one flash attempt and separates settings
and firmware outcomes. These are models, not device executors.

Backup-directory I/O is Linux-only: anchored no-follow traversal, nonblocking
regular-file reads, bounded sizes, private 0700/0600 staging, reread validation
and atomic no-replace publication. Restrictive owner-read umasks fail closed
with cleanup. macOS/Windows directory I/O is explicitly disabled pending private
ACL/handle qualification; local example projects are unaffected. The inspection
window has localized path-free errors, scrolling and stale-result guards.

Independent reviews found and closed parser newline parity, confirmation bypass,
post-erase cancellation, file/link/permission/reread and UI bounds/localization
issues. Fresh reviews accepted the bounded models and integration. No positive
catalog-valid full-ROM export, physical-unit ownership or device operation was
proved. Full capture/history, restore/installer UI and execution remain open.

`cargo test --locked --offline` passed **1625 tests, 0 failed, 14 ignored** in
240.28 s; its native build passed in 22.51 s. After wording-only UI labels and
source comments changed, all **92 AK820 tests passed** in 5.96 s and the native
build passed in 21.63 s. Both native builds emitted 183 warnings. Filesystem umask tests
ran in non-root subprocesses. Native Debian 12/Xvfb checks opened the backup
window and Zenity picker, rejected a non-backup directory with a Russian message,
and closed/reopened the window without retaining that error. Positive production
archive acceptance remains unverified. No USB or hardware operation was sent.

CI run [40173262047](https://github.com/dvino/ak-configurator/actions/runs/40173262047)
passed all six targets for `709cde6`. Downloaded artifact ZIP hashes matched
GitHub metadata, included SHA256SUMS passed and all six binary headers matched
the expected architecture. This is evidence for that earlier source packet,
not the models above or execution on every OS.

The current work stops after a tested offline trial build and handoff. Further
backup/restore and installer work is deferred for manual transfer. The next
agent must first obtain a detailed plan from a GPT-6.1 Sol high architect;
no such architect has been invoked here. The exceptional architecture and complex
planning route was added to both projects after independent EN/RU review;
firmware main contains the documentation-only commit `dc0e7cb`.

A separate runtime check of the `709cde6` Linux x86_64 CI binary on Debian 12
failed before startup: GLIBC_2.38 and GLIBC_2.39 were missing. Its symbol table
requires glibc 2.39. The local Debian 12 build remains the tested fallback;
CI success alone does not establish compatibility with that older runtime.

## Offline trial delivery checkpoint

All six jobs in CI [40174635838](https://github.com/dvino/ak-configurator/actions/runs/40174635838)
passed for source `1afd0f17b91dbac6cc75ea859b979abaa01c5624`. Downloaded artifact
ZIP digests, included SHA256SUMS and binary architectures were checked. The
separate Debian 12 x86_64 development binary comes from the same source and
retains its tested bytes (SHA256
`47abf519c08859db808b0ebc5d6bcf44cdbb0793fdaff2f87606da2224d5dc4e`).
Both final CI Linux files reference glibc 2.39; the separate Debian build remains
the locally tested alternative. Windows/macOS/arm64 execution remains unverified.

The trial supplies offline launch scripts, matching EN/RU instructions, license
notices and checksums. It keeps version 0.1.0-alpha.1 and does not claim a full
beta release. Source, platform builds, Linux GUI checks and physical keyboard
qualification remain separate. Work now stops for manual handoff; the next
agent must first obtain the requested detailed Sol High architecture plan.

## Windows audio CI path normalization — 2026-10-26

The follow-up run [40356707732](https://github.com/dvino/ak-configurator/actions/runs/40356707732)
failed in the offline audio setup regression suite on all Windows and macOS
runners, before application compilation. The mocked MSI extraction asserted raw
path strings. Hosted runners resolve temporary directories differently: macOS
uses `/private/var` for `/var`, and current Windows runners return long user
paths where the fixture expected an 8.3 short name. The builder itself was not
reported failing.

The regression checks now resolve both actual and expected paths before
comparison for MSI extraction, returned executable paths, and build output.
The test still checks the exact executable location and MSI target directory;
it no longer assumes how the operating system spells the same path.

| Check | Result and scope |
| --- | --- |
| Audio setup regression tests | 12 passed locally on Linux and on all six hosted targets. Independent GPT-6 Sol Low review approved both CI fixes with no blocking findings. |
| Application/build behavior | Not changed by this test-only correction. |
| Candidate install smoke | The first full workflow exposed an additional Windows shortcut path failure, corrected and passed in the follow-up run below. CI smoke does not establish broader platform qualification. |

### Windows installed-candidate smoke follow-up

Run 40372901470 passed the corrected audio setup tests on all six targets, both
macOS jobs, and both Linux jobs. Windows x64 and arm64 then failed during the
candidate upgrade smoke test: PowerShell returned the Start Menu shortcut target
using a long path while the test installation root used an equivalent 8.3 short
path. The installer correctly refused a literal mismatch, but did not recognize
these spellings as the same Windows path.

The Windows install and uninstall scripts now canonicalize both shortcut and
expected executable paths with `GetLongPathName` before the case-insensitive
comparison. Other targets still fail closed; unresolvable or different paths
are not accepted as owned shortcuts. Independent GPT-6 Sol Low review approved
the change. Commit `14ccf77` contains the Windows correction and is on
`desktop-dev` after the first repair commit `0c28f55`.

The follow-up [run 40377698479](https://github.com/dvino/ak-configurator/actions/runs/40377698479)
passed all six jobs. The audio setup regression passed on every target, and the
candidate install/execute/uninstall smoke passed on Windows x64 and arm64. Linux
x64/arm64 and macOS x64/arm64 jobs also passed. The local release workflow suite
passed 15 tests. These results establish the stated CI checks only; broader OS
qualification and physical keyboard behavior remain separate.

## Windows alpha follow-up — 2026-10-27

An independent automated reviewer checked the changes to FD02 response matching, failure handling, tray close priority, the Windows icon and the documentation. No actionable code findings remained. The review did not include device access.

The Windows x86_64 release build and offline self-check passed. The package install, upgrade, uninstall and data-preservation checks passed in isolation; the same executable was installed and self-checked. Its SHA256 is 79a9e6a2736ef4351363a2cd637c891e827ba61a4074e8287aabf6ae7f80f678. The unsigned archive SHA256 is 92581cecb2dfa1dee053002e495756dabf5b1d2756c6861b7f71a56e7f9c1828. This local package was built from a modified worktree based on b332696, not from a clean release commit.

The first full Windows suite had 1691 passes, 11 failures and 5 ignored. All 11 failed tests passed individually after test-only locale and deadline corrections. At this review, the full rerun had not finished. Targeted FD02, helper fault and tray-close tests passed. The physical indicator, real tray restore and other platforms remain unverified.

The full Windows rerun was stopped after tests wrote to the real application configuration. Unit-test storage, configuration and cache paths now use a temporary root, and unit tests cannot change autostart. Five storage tests and one startup/cache path test passed. Other filesystem paths, system APIs, network access and spawned production binaries still require review. This does not establish that the full suite is safe on a user account.

An independent automated reviewer checked the test-only isolation changes and the EN/RU incident record. No blocking findings remained. The shared fallback does not isolate unscoped tests from each other; worker tasks that need a scoped directory must retain its resolved path and finish before the scope ends.

Follow-up unit-test safeguards reject the reviewed live HID open and discovery paths, require an explicit test HID for connection and skip native tray creation. Focused Windows tests for these safeguards, the indicator and tray-close policy passed; 42 connection and 15 scan tests also passed. They do not cover every operating-system or subprocess path, and the test suite is not qualified as fully isolated.

An independent automated reviewer checked these safeguards and their documentation; no blocking findings remained. Private lower-level HID calls, macOS initialization and spawned production binaries remain outside this review's isolation claim.
