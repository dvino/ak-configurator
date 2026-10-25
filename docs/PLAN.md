Русский перевод: [План](ru/PLAN.md).

# AK Configurator implementation plan

This plan turns the agreed [requirements](REQUIREMENTS.md) into reviewable work for the separate `dvino/ak-configurator` repository. AK820 firmware remains in `dvino/ak820`. See [REVIEW.md](REVIEW.md) for the source reuse findings. It does not authorize a live keyboard operation. Stage completion means the evidence exists; it does not prove installation or device behavior.

## Baseline and delivery order

Updated 2026-10-25 for requirements version 0.5. Firmware contracts use `dvino/ak820` tag `v0.6.0-beta.1`, commit `bef20f89d3da01d770767919bc7c8fc3c3a1a808`, especially `docs/BUILD-PROFILES.md`, `docs/MODULE-MATRIX.md`, `docs/MODULE-STORAGE.md` and `web/docs/FIRMWARE-HANDOFF.md`. The existing alpha.3 editor remains an earlier bounded milestone. The beta.1 catalog preview is implemented; nine-profile offline editing, complete backups and companion installation are separate unfinished milestones. See [architecture review](ARCHITECTURE-REVIEW.md) for source and test gaps.

Complete the native shared contracts, nine-profile offline editor, backup/restore and modeled installer before native package qualification. Hardware evidence gates live acceptance, not work that can be completed with fixtures. Begin browser feasibility after the native editor and packages are reasonably usable. The OS-layout helper has its own later acceptance; firmware module support does not prove that a helper exists.

## 1. Confirm Entropy reuse and native feasibility

**Work.** Reuse Entropy at pinned revision `5b222b5d6349fb351f53b36c66169095b3039fc1`. Start native Rust adaptation first. Use recorded AK820 device replies to check the editor/model with a fake transport. Do not reset, enter the loader, or write to the keyboard in this stage. Browser feasibility and implementation are deferred until after the native desktop release. An alternative foundation requires a separate future decision if Entropy cannot meet a required function; do not switch silently.

Before implementation, define safe OEM source handling: required per-device bytes, protected regions, source validation, and how the source is tied to the connected keyboard. A saved recovery archive is not mandatory for installation, but safe source acquisition and protected-image validation are mandatory. If device identity is ambiguous, block writes and rollback. Do not issue reset, loader-transition, or write commands without explicit confirmation for that specific operation.

**Results.** Native fixture-backed editor tests, pinned source revision and license notes, and a recorded reuse decision. Record a separate project-image policy for custom builds. Preserve the installer reference’s exact approved-image and device-specific OEM checks. The historical alpha.3 pins and beta.1 `work` approval remain separate; do not broaden either policy to accept custom images.

**Completion / stop.** Continue when Entropy reuse is justified, the native path is viable, and safe source/identity rules are concrete. Browser feasibility is assessed in a later stage. Stop any future flashing work if the loader protocol, source bytes, protected regions, or device identity cannot be established safely.

## 2. Establish shared client and project data

Use `main` as the shared baseline, `desktop-dev` for native work, `desktop` for native release preparation, and `web-dev` for browser work. All branches belong to the separate configurator repository. Merge shared changes between development branches and check both targets. AK820 firmware remains in `dvino/ak820`.

**Work.** Reuse Entropy’s Rust source tree under `src/`. Keep the model, protocol, validation, firmware packaging, backup format, UI, and platform adapters in clear Rust modules that fit the upstream layout. Use verified pinned upstream code at [5b222b5d6349fb351f53b36c66169095b3039fc1](https://github.com/ergohaven/entropy/tree/5b222b5d6349fb351f53b36c66169095b3039fc1) for the configurator. Firmware behavior and tooling come from the separate [AK820 source](https://github.com/dvino/ak820/tree/bef20f89d3da01d770767919bc7c8fc3c3a1a808). Reuse its `tools/installer/settings_backup.py`, `tools/installer/settings_workflow.py`, `qmk-8051-ak820/util/pack_ak820.py` and `qmk-8051-ak820/util/ak820_tool.py` as behavioral references. These paths belong to the firmware repository, not this tree.

Define bundle versioning and revocation data, prerelease policy, image-source policy, model identity, and capability metadata before screens depend on them. The catalog must distinguish released, development, and installed firmware capabilities. Add modules only where implementation, resource use, and ABI impact are supported by evidence; do not infer a need for MCU GETFEATURES without protocol evidence.

**Results.** Shared data contracts, custom-image project format and verifier, generated release/capability catalog, source map, license inventory, and fixtures. Demonstrate identical acceptance/rejection behavior for the packer and Python reference implementation, including negative cases and exact output bytes. Validate schema and bundled references offline.

**Completion / stop.** No UI may enable a write from unverified metadata or a partial/ambiguous image. Stop if packers disagree, a negative fixture is accepted, or project-image policy cannot be checked independently of a warning.

## 3. Complete the native desktop editor

**Work.** Complete the native AK820 editor for the verified full permanent alpha3 image. Add exclusive device ownership, explicit selection when identity is not unique, firmware and capability checks, full settings read, supported layer/key/encoder mapping controls, macros, Combo, RGB, help, and RU/EN selection. Match AK820 behavior instead of copying unrelated Vial screens. Preserve fail-stop settings writes with readback. Provide an offline UI preview and bundled help; demo data is an example layout, not a device backup. Browser work is deferred until after native packaging and release.

**Results.** Native development binary, offline help and UI preview, RU/EN help coverage, recorded-response/model fixtures, and evidence that settings readback and failure stops are represented. These are host-side results and do not prove physical device behavior.

**Completion / stop.** The existing desktop editor is reasonably working for the declared alpha3 settings scope; the beta.1 desktop milestone also requires 3a and 3b below. A failed read or persistence operation must never appear successful. No physical writes are part of this stage.

### 3a. Complete beta.1 profile contracts and offline editing

**Work.** Extend `src/ak820_beta1_profiles.rs` and the shared model with exact settings identities, supported keycodes by source, table counts, active QSIDs, limits and explicit transition pairs. Keep linked-window and packed CODE hashes distinct. Extend `src/ak820.rs`, `src/hid_ak820.rs`, connection loading, key picker and supported editor modules to use a verified per-image contract instead of the global alpha.3 capabilities. Preserve the alpha.3 path and safe rejection of unqualified identities. Catalog preview alone does not enable device editing.

Add offline selection and editable example projects for all nine profiles, with no HID discovery and no change to the installed profile. Cover all 14 optional modules from the matrix, including controls that are key assignments rather than tables. Add typed Tap Dance, Key Overrides and QMK Settings codecs and editors for the three schema03 profiles. Use the firmware's exact ranges, CRC, defaults and source restrictions; do not expose inactive reserved storage. Add RU/EN explanations and links for every supported control, volatile state and unavailable combination.

**Results.** Nine-profile UI/model fixtures, complete successful image-verification/settings-load fixtures, contextual keycode tests and typed codec parity with the firmware references. Cover absent modules, malformed counts/IDs, invalid ranges, ACK/getter/NVM mismatch, unchanged blank defaults and a failure at each transaction boundary. These checks use a fake transport and no physical interaction.

**Completion / stop.** All profile controls and offline edits match their contracts. Settings mutation stays disabled for a missing or unqualified packed identity. Any dropped stored field, unknown capability, wrong-source keycode or untested validation path leaves the milestone incomplete.

### 3b. Complete backups, migration and guarded restore

**Work.** Implement the firmware-compatible `ak820-settings-backup-1` and `-2` contracts in reusable Rust modules and integrate local import/export/history into the UI. Preserve complete durable settings and inactive typed storage. Add dry-run change summaries, backup before import/reset, undo import, exact same-image checks and explicit hash-pair transitions. Implement explicit schema01/02-to-schema03 migration and reject schema03 downgrade. Keep `.entlayout` identified as a partial editor snapshot until it meets the complete contract.

**Results.** Golden files and negative cases agree with `settings_backup.py`, `settings_workflow.py` and typed storage rules. Test all nine profiles, cross-profile denial, inaccessible nonempty tables, duplicate/unknown fields, insufficient backup, cancellation and stopped restore. Validate the complete input before the first setter and read back every applied field.

**Completion / stop.** Complete settings round trips and failure stops pass offline. No runtime Python dependency, silent truncation or automatic restore after failure. Physical unlock and retention remain separate qualification.

## 4. Package and release the native configurator

**Work.** Prepare native development binaries and release packages for Windows and Linux x86_64/arm64 and macOS arm64/x86_64, with matching checksums and RU/EN installation instructions. Support offline configuration, all curated AK820 profiles, exact image validation, and a guarded in-app firmware install/readback flow. Do not enable live firmware installation until its target-image and hardware qualification evidence is complete. OEM recovery and the operating-system LED helper remain separate work. Treat helper autostart, tray behavior, and active-layout detection as separate future work. Prove layout mappings and supported desktop environments per platform; confirm firmware changes, resource use and stack margin before any helper implementation. Measure application memory and startup on target systems; do not infer them from installer size.

**Results.** Per-platform build/package records, checksums, installation instructions, offline-use evidence, and a qualification record listing passed, skipped and unproven checks. The `desktop-dev` workflow defines raw development binaries for Linux and Windows x86_64/arm64 and macOS arm64/x86_64. Workflow definitions are not evidence that a CI run passed. The firmware's standalone installer is transitional and will be retired after the companion provides the safe profile installation and verification flow.

**Completion / stop.** Release the native configurator only after applicable per-platform package, offline-use, HID and function checks pass. Unsupported OS environments must be named plainly. Native release does not require browser completion.

### 4a. Prepare the companion installer and OEM recovery

**Work.** Before live support, implement reusable Rust image validation/packing and an operation state machine with fake transports. Reproduce the firmware reference's exact bytes and failures, OEM ownership and protected-region rules. Include profile and version selection, prerelease opt-in, revocation checks, bundled/local inputs, fresh backup, immutable confirmation contents, fresh unlock where available, helper ownership, cancellation before erase, one flash attempt, reconnect, full installed-image verification and optional settings restore. Keep firmware outcome and settings-restore outcome separate. Add validated local OEM rollback without project settings. Prepare every input before erase; no downloads after erase begins.

**Results.** Offline success and fault traces for every state and all supported profiles, packer parity fixtures, explicit image/backup identity checks and a review of permitted writes. When required OEM input is unavailable, record the specific missing input; do not replace ownership or hardware evidence with synthetic proof. Live flash, reset, loader transition and device setters remain disabled until separately qualified and authorized.

**Completion / stop.** Native installation replaces the standalone installer only after each supported profile has a qualified install and verification path. Release approval, inclusion in the beta.1 standalone `work` installer and companion support remain separate. Keep the existing installer while these conditions are open. OEM hardware rollback acceptance and the optional helper are recorded separately.

### 4b. Qualify each desktop target

Record build, package, checksum, offline launch, USB access, function, memory and startup evidence for Linux x86_64/arm64, Windows x86_64/arm64 and macOS x86_64/arm64. Define minimum OS versions, Linux runtime dependencies and supported X11/Wayland environments from actual qualification. A raw CI binary is not an installer or a successful runtime check. Provide matching RU/EN download, checksum, extraction and launch instructions. Signing/notarization status must be explicit. Missing target evidence blocks support claims for that target.

## 5. Add browser update, restore, and OEM rollback

**Work.** Begin browser feasibility after the native desktop editor and packaging are reasonably working. Demonstrate the WebAssembly build, WebHID permission flow, and read-only Feature Report request before promising browser flashing. Then implement release selection, stable/prerelease and revocation rules, custom image selection, compatibility preview, fresh settings backup, confirmed update flow, cancellation before erase, device reconnect, installed-image verification, and optional settings restore with readback. The browser release must perform actual flash, restore, and OEM rollback entirely in the browser. An installer fallback does not satisfy this requirement. Download and validate every input before erase; after erase begins, any timeout, storage fault, disconnect, or bad readback stops the sequence. Never retry, reset, blindly restore, or continue automatically. OEM restore uses only a validated image with proven ownership and never applies project settings over it.

**Results.** Offline host-model coverage for the operation state machine, negative fixtures for hashes, addresses, protected regions, truncation and mismatched identity, and a reviewed browser permission/loader design. Hardware checks are separate and require explicit owner confirmation for each transition or write.

**Completion / stop.** Host checks prove ordering and fail-stop behavior, not hardware. The browser feature is not release-ready until the required loader, flash, installed image, settings restore, and OEM rollback have been separately qualified on hardware with operation-specific authorization. Any unresolved identity or image-preparation ambiguity blocks writes.

## 6. Qualify the browser configurator and first web release

**Work.** Cover every supported behavior and remapping in the reusable suite, including both encoder directions. Regression checks use fixtures and require no physical key presses or knob movement. Include protocol parsing, backups, packaging parity, release/revocation, offline cache, local storage, and UI/help consistency. Help modules and catalog fields must match actual firmware support. CI builds and checks browser assets and fixtures offline; publish to GitHub Pages and firmware/catalog/help data to GitHub Releases. Report device behavior and durable storage separately from static checks and host models. Keep the firmware's standalone installers only as a transitional path until the companion can safely install and verify every supported profile.

**Results.** Acceptance matrix, CI artifacts, release manifest and hashes, deployment instructions, and qualification record listing passed, skipped, and unproven conditions. Coordinate browser and firmware releases by exact firmware, configurator and help identifiers.

**Completion / stop.** All acceptance rows applicable to the browser release have evidence and no unresolved release-blocking failure. Do not claim a physical feature from source presence or a host-model pass.

## Acceptance coverage

| Requirements | Evidence required before release |
| --- | --- |
| AKW-001–005, AKW-082: scope and foundation | Shared editor/transport prototype, Entropy reuse evidence, browser feasibility record, and RU/EN coverage |
| AKW-010–015: device and capabilities | Unique identity or blocked write; installed-image capabilities, not release assumptions |
| AKW-020–024, AKW-040–042, AKW-045: settings and help | Layers, keys, both wheel directions, macros, Combo, RGB, backup/restore and documented limits |
| AKW-030–034: releases and custom builds | Stable/prerelease/revocation behavior; schema, model, address, hash and protected-region checks |
| AKW-043–044, AKW-050–054: firmware and OEM recovery | Validated source/identity, browser loader, cancellation boundary, flash/readback/restore/rollback results |
| AKW-023, AKW-070–072: offline and privacy | Cached UI/help, verified local backups, no required account or upload, private diagnostic report |
| AKW-060–064: desktop and helper | Linux x86_64/arm64, Windows x86_64/arm64 and macOS x86_64/arm64 packages, platform-specific HID/layout checks and measured resource use |
| AKW-080–085: checks and release | Cross-repository firmware/configurator identifiers, both-target CI, browser deployment, and a separate hardware qualification record |

This table summarizes coverage; it does not replace the detailed requirements.
