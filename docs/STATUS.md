Русский перевод: [Состояние проекта](ru/STATUS.md).

# Project status

## beta.4.1 exact-image installation integration — prepared, not device-tested

The published beta.4.1 release catalog is pinned to SHA256 `5ec2dbddfb2d66d2afa13a408f0365d2474d8f94866f700d65d124eeb13f05c1`; its firmware matrix is pinned to `7b38153e68d168718d04c5c8c8502daad875ca63298bf0e79c20a39c4648c570`. It authorizes flashing and binds each of ten profiles to exact IHX and packed-CODE SHA256 values; `hardware_validated=false` and `settings_writes_authorized=false`. The installer selects beta.4.1 by default, keeps beta.3 as an alternate, and requires explicit prerelease opt-in. It uses beta.4.1 defaults, and retains fresh backup, confirmation, one-attempt loader and full readback gates. Beta.4 settings writes and restore remain blocked. No physical-device operation was run.

A targeted Linux `cargo check --locked --bin ak-configurator` passed with Rust 1.97.0 in the Debian-based build container; 171 compiler warnings remain across the project. The beta.4.1-filtered test build was stopped before producing a test result when temporary compilation approached the host's low free-disk limit. No Linux release package was rebuilt; Wayland and physical-device tests remain deferred.

The optimized Linux x86_64 binary was built with Rust 1.97.0 on Debian 12 (glibc); SHA256 is `70b3e46acdd249e53efc8dfc2b71dc4b9b016d3d51e9bff89d11f3f40f72c03e`. Its offline `--self-check` passed and reports `hardware_access=false` and `network_access=false`. The report identifies application version `0.1.0-alpha.1` and firmware release `v0.6.0-beta.3`, so it validates the built-in beta.3 public profiles and does not verify beta.4 packaging. The full offline command `cargo test --offline --locked --bin ak-configurator -- --test-threads=1` passed 1916 tests; 14 were ignored. This includes exact beta.4 image recognition, blocked HID writes and localized QSID 22 display tests. The build emitted 169 compiler warnings, not individually triaged. Neither the self-check nor unit tests verify GUI operation, Wayland, or physical hardware. The glibc binary is not qualified for Alpine/musl.

### Superseded pre-release candidate audit

The earlier firmware PR candidate `eb6d01870411133c08bef4def2748000c3bcb785` had a development capability catalog (SHA256 `f33698e6329a66a3e4ade069d1eda50dfc45882109112860e76cebe41cf165c2`) and matrix (SHA256 `8bf90526a51e9879870a61d7de1c59b1df38e3489b77a67e67496503f0307658`). Its catalog IHX identities matched published beta.3 profiles rather than the candidate beta.4 matrix. That earlier candidate audit does not describe the official release integrated above.

The Linux x86_64 application candidate was built from base commit `771ac4fb4a4dcbfd15969f73e9a2a67a53decd87` and passed `--self-check` in Ubuntu 24.04.5 for all ten published beta.3 profiles. The report records `hardware_access=false` and `network_access=false`; SHA256 is `4e83c322245efdeac5b5b2173ac9bdc3e78183dcc79f7cda49be066357c73d69`. It remains a beta.3-targeting candidate, not beta.4 support; GUI operation was not checked. This glibc binary is not qualified for Alpine/musl. The builder reported 168 compiler warnings; build output was not retained and the warnings were not individually triaged.


Catalog navigation, window and approval labels now derive their release from the pinned catalog version. This changes presentation only and does not change device admission.

A separate offline NVM preparation helper also preserves all non-QS bytes; it neither validates the complete backup nor authorizes a transition.

An offline metadata projection checks provisional compact-controls and storage version consistency; it is not a complete image or catalog validator.

A separate offline full-settings decoder now combines explicit image versions with structural, macro and non-QS extension checks; current production admission and v1 QS behavior remain unchanged. Provisional media Combo outputs are parsed only by the separate offline decoder with an explicit source-image capability; production paths still reject them.

A separate offline archive-envelope bridge reuses five-file validation and requires an injected exact-ROM validator; synthetic callbacks establish no image trust and its result cannot start a restore.

Beta.4 connection reads QSID 22 as a u8, reconciles it with the exact v2 NVM record, and displays the English/Russian Caps Word ruleset in About Device. QSID 22 remains excluded from the inherited permissive-hold reader and from all writers. Beta.3 behavior remains unchanged.

The explicit QS v1/v2 model is used for read-only settings reconciliation after exact beta.4 image identification. It does not authorize saved-settings writes or restore; exact beta.4.1 firmware installation is separately admitted by the pinned release catalog described above.

Offline project import undo now restores the replaced profile without discarding subsequent edits to other profiles. Project import rejects nonregular files; Unix opens are nonblocking and rechecked after opening. The file dialog worker also handles reads and atomic writes, keeping slow file access off the UI thread; parsing and application remain on the UI thread.

Updated 2026-10-28. Application source version is `0.1.0-alpha.1`. The previous `0.1.0-beta.2` was an unpublished development version; GitHub has no application release or tag for it. The firmware target is AK820 `v0.6.0-beta.3`, tag object `4cabae9af9bb68492a8cf079c59f8b388c5cf788`, peeled commit `88d190abb156960915a0f1f8ac7f602bb401eab3`. The [public firmware release](https://github.com/dvino/ak820/releases/tag/v0.6.0-beta.3) was published on 2026-10-27 at 06:50:35 UTC. CI run [40461867751](https://github.com/dvino/ak820/actions/runs/40461867751) passed the shared-host job and “Revalidate and assemble all ten profiles”; its profile qualification matrix and publishing jobs were skipped. A later independent verification confirmed GitHub API SHA256 digests for the release catalog, firmware bundle, five JSON assets and `SHA256SUMS`; all ten published IHX/settings identities match reviewed candidates, and the ten published map hashes match the release matrix. All ten profiles are marked release-approved/installable in `approved-release.json`, which pins the exact hashes from `installer-release.json`. The published `independent-review.md` records reproduced CI/source-review evidence, not a new human review. This establishes release-input identity, not physical device behavior. See [REVIEW.md](REVIEW.md) for application source and test evidence.

CI run `40534916601` failed both Windows native GUI smoke checks without retaining their JSON reports. The cause is not established. The workflow now retains a bounded failure report separately; a later run is required to diagnose the failure.

## Handoff follow-up and partial MCP preview

The handoff fixes release the tray relay mutex before native window calls and separate 100 ms layout sampling from one-second active override renewal. Sampling no longer borrows HID. Necessary writes retain the verified device's cached editor and exact RGB capabilities, without enabling concurrent input or showing a disconnected page. This addresses recurring page flicker and RGB subpage resets. The previous five-second hidden-window heartbeat could outlast the firmware's roughly three-second override lease. The new cadence continues while hidden; it is a scheduling target, not a measured physical LED latency guarantee. Unchanged modes do not send on every sample.

Auto Shift now follows advertised settings. Exact beta.3 Main/Typing expose timeout QSID 4 but do not implement enable/options QSID 3. The misleading switches are hidden and timeout remains editable; the page explains the assigned AS_ON/AS_OFF/AS_TOGGLE actions used by this firmware. This does not add a persistent enable setting to beta.3. Generic firmware that advertises QSID 3 retains flag editing with fresh read, masked change and checked readback.

Visible startup shows the supplied application logo before full editor/font initialization. GUI construction no longer performs synchronous HID enumeration; the existing background scanner starts discovery. Startup phase timing is retained by the native smoke report. Pre-window preference reads, graphics initialization and OS loader time remain separate; Linux offline measurements do not establish Windows startup latency. README in both languages uses the same supplied logo.

Indicator controls belong to application settings. The redundant “Close interface” action is removed. Normal window close follows the tray setting automatically; explicit Quit in the application menu, Ctrl+Q (also Cmd+Q), or the tray requests full exit while respecting unlock/installation and pending-write guards.

Linux now exports a StatusNotifierItem and a Show/Quit D-Bus menu instead of merely minimizing the window. A missing tray host keeps the window visible; watcher/host loss requests restoration. Windows/macOS keep their native tray paths, with a macOS creation-failure guard. The cross-platform smoke uses native visibility checks; Linux CI uses a synthetic protocol host, not a real desktop panel. CI packaged runtime passed on Windows/macOS and Linux; real desktop-panel behavior, full Wayland tray hiding/restoration and physical helper latency remain open.

An optional, default-off MCP stdio preview implements part of P0 using pinned `rmcp = 3.8.0`: diagnostic owner lifecycle, exact FD00-first identity, typed FD01, six closed tools, synthetic error checks, bounded metadata traces and in-memory reports. It exposes no setters or firmware operations. Full settings/backup/control workflows and complete positive packed-image fixtures remain unimplemented. See [MCP.md](MCP.md).

`--demo-ak820` and `--runtime-smoke` now create private temporary settings before application startup and block reviewed live hardware/system integration paths. This improves isolation; it does not certify every inherited/ignored test or provide an OS sandbox. The release verifier requires native GUI reports for all six targets bound to the packaged executable SHA256. No publication is performed, and a configured gate is not a completed platform pass. Current test results and limits are in [REVIEW.md](REVIEW.md).

## Current source scope

The current source supports ten exact beta.3 profile images: Main, Game, Mmo, Typing, Code, Pointer, Onehand, Creative, Shortcuts and Multios. Exact packed image/settings identity and required settings remain prerequisites for editing. The beta.2 identities are kept on a distinct legacy path; beta.2 backups migrate only through an explicit reviewed source-to-target migration, never by automatic promotion. Alpha3 compatibility remains exact and separate. The v2 offline project format pins its target release, catalog and Intel HEX image; beta1 v1 projects require deliberate manual migration.

The editor exposes beta.3 profile capabilities, including Combo with two distinct ordinary-key triggers and CapsWord as a result, plus the supported `LT(layer0..5, MC0..15)` macro action. This combined layer-tap/macro action is available only on the physical matrix; other actions retain their own source restrictions. AutoShift QSID 4 is available in Main and Typing. Beta.2 Main does not gain it. CAVA 1.0.0 and OpenRGB SDK 0/1 integrations remain intact.

The beta.3 Direct palette path reads the current palette, commits a complete frame atomically in RAM, and can explicitly restore the previously read Direct palette. `direct-ram.json` provides a separate explicit save/restore sidecar. It is not durable keyboard settings and does not write NVM. The source control owner and RGB scheduler cap sends at ten frames/s. Playback starts explicitly; faults stop without blind retry, automatic abort or restore. Successful firmware commit and logical readback do not prove physical LED scan output or visual qualification. See [RGB-PLAN.md](RGB-PLAN.md).

## Linux candidate checks

The optimized Linux x86_64 binary passed the offline ten-profile `--self-check`. A local pre-commit trial package passed installation, replacement, installed self-check and removal while preserving separate user data. Native demo inspection covers RU/EN at 800×500, key-picker scrolling and LT assignment, Combo output selection and timing, and separation of footer status from key hints. These checks use no physical keyboard or system capture. Exact test and package evidence is recorded in [REVIEW.md](REVIEW.md).

## Backup capture and installation

Once a settings capture is attempted, any failure stops preparation, including when defaults or rollback were selected. The failed owner remains blocked from prepare/execute until a fresh reread. A fresh install-only/OEM owner without loaded project settings may skip unsupported settings capture, but still requires exact full-ROM and safety checks. The OEM no-serial warning is limited to the first OEM source path.

The ten beta.3 release profile sources and verified assets are available for offline preparation and explicit prerelease installation. Package scripts now include localized Desktop, Changelog, Status and Review guides alongside the existing install/RGB guides, licenses, and offline beta.3 assets. Six native platform candidates are configured and unsigned. All six unsigned candidates passed final packaged build/runtime/install CI; signing and publication remain open. No physical flash, retention, OEM rollback, or LED behavior is claimed.

## Verification boundary and remaining work

Firmware CI passed the shared-host job and revalidated/assembled all ten profiles; it skipped the profile qualification matrix and publishing. The later public release and source-to-release identity checks are complete as described above. Application package configuration is not a completed release. Backup history limits, guarded restore/reset/deletion steps, and filesystem/platform constraints are detailed in [DESKTOP.md](DESKTOP.md). The final six-platform packaged CI runtime/install pass completed; signing/notarization, real desktop-panel checks and device qualification remain open. The verified Linux trial is recorded separately above. Verified release inputs do not establish physical device behavior.

Historical beta.2 documentation and changelog entries describe the earlier development baseline; claims that Direct palette readback or atomic commit are absent apply only to that historical firmware. Beta.2 backup migration is supported only through an explicit reviewed source-to-target path. Historical CI runs, isolated adapter typechecks and earlier platform checks do not establish current-source qualification. Browser support remains a later stage. Source implementation, host tests, platform execution and physical behavior are tracked separately.

Windows local fix: HID now routes FD02 replies to OS indicator validation. The local build uses the supplied AK Configurator icon for the Windows executable, window and tray. The matching test passes; physical indicator and tray restore remain unverified.

On a Russian-locale Windows host, the full suite reported 1691 passes, 11 failures and 5 ignored. All 11 passed individually after test fixtures explicitly selected English where expected; the held-HID test also has a longer test-only deadline. At this review, a full rerun had not finished.

The OS helper cannot enter the background after a session or helper error. The error stays visible while the device is unverified. Targeted offline tests passed, and the rebuilt Windows x86_64 package was installed and self-checked. Physical behavior remains unverified.

The tray setting takes priority over the automatic helper handoff. The Windows close-event test passed; restoring a real window from the tray remains unverified.

The local Windows full-suite rerun was stopped after test code wrote to the real application configuration. Unit-test storage, configuration and cache paths now use a temporary root; unit tests cannot change autostart. Five storage tests and one startup/cache path test passed. Other filesystem paths, system APIs, network access and spawned production binaries still need isolation review; this does not qualify the full suite as safe to run on a user account.

Unit-test device discovery and the reviewed HID open paths now fail closed; the connection worker requires an explicit test device. Unit tests skip native tray creation. Focused Windows tests passed for these paths and the existing indicator and tray decisions; 42 connection and 15 scan tests also passed. A real tray restore and physical keyboard behavior remain unverified.

On Linux, hiding to the tray is currently supported only for X11 window backends. On Wayland or an unknown backend the application keeps the window open and reports the limitation; `--minimized` also leaves it visible because winit cannot restore a minimized Wayland window. The exact CI Linux executable rendered in isolated headless Weston 13; full Wayland tray/real desktop behavior and physical keyboard behavior remain unqualified.
