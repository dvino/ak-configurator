Русский перевод: [План](ru/PLAN.md).

# AK Configurator implementation plan

This plan turns the agreed [requirements](REQUIREMENTS.md) into reviewable work for the separate `dvino/ak-configurator` repository. AK820 firmware remains in `dvino/ak820`. See [REVIEW.md](REVIEW.md) for the source reuse findings. It does not authorize a live keyboard operation. Stage completion means the evidence exists; it does not prove installation or device behavior.

## 1. Confirm Entropy reuse and browser feasibility

**Work.** Reuse Entropy at pinned revision `5b222b5d6349fb351f53b36c66169095b3039fc1`. Start native Rust adaptation first and build the WebAssembly/browser target in parallel. Use recorded AK820 device replies to check the editor/model with a fake transport. Separately check a browser/Wasm build, WebHID permission flow, and a read-only Feature Report request; fixtures cannot test browser permissions or loader access. Do not reset, enter the loader, or write to the keyboard in this stage. Keep the browser target’s feasibility risk explicit. An alternative foundation requires a separate future decision if Entropy cannot meet a required function; do not switch silently.

Before implementation, define safe OEM source handling: required per-device bytes, protected regions, source validation, and how the source is tied to the connected keyboard. A saved recovery archive is not mandatory for installation, but safe source acquisition and protected-image validation are mandatory. If device identity is ambiguous, block writes and rollback. Do not issue reset, loader-transition, or write commands without explicit confirmation for that specific operation.

**Results.** Fixture-backed editor test, separate browser/Wasm and read-only browser request results, pinned source revision and license notes, loader feasibility report, and a recorded reuse decision. Record a separate project-image policy for custom builds. Do not weaken the existing AK820 native installer’s exact alpha3 and personal-reference checks to accept custom images.

**Completion / stop.** Continue when Entropy reuse is justified, the native path is viable, browser feasibility has evidence and remains explicitly tracked, and safe source/identity rules are concrete. Stop browser flashing work if the loader protocol, source bytes, protected regions, or device identity cannot be established safely. Native work can proceed independently.

## 2. Establish shared client and project data

Use `main` as the shared baseline, `desktop-dev` for native work, `desktop` for native release preparation, and `web-dev` for browser work. All branches belong to the separate configurator repository. Merge shared changes between development branches and check both targets. AK820 firmware remains in `dvino/ak820`.

**Work.** Reuse Entropy’s Rust source tree under `src/`. Keep the model, protocol, validation, firmware packaging, backup format, UI, and platform adapters in clear Rust modules that fit the upstream layout. Use verified pinned upstream code at [5b222b5d6349fb351f53b36c66169095b3039fc1](https://github.com/ergohaven/entropy/tree/5b222b5d6349fb351f53b36c66169095b3039fc1) for the configurator. Firmware behavior and tooling come from the separate [AK820 source](https://github.com/dvino/ak820/tree/5cbc66fbaf22bf5372e76e892207f58d453d9183). Reuse its `tools/installer/settings_backup.py`, `tools/installer/settings_workflow.py`, `qmk-8051-ak820/util/pack_ak820.py` and `qmk-8051-ak820/util/ak820_tool.py` as behavioral references. These paths belong to the firmware repository, not this tree.

Define bundle versioning and revocation data, prerelease policy, image-source policy, model identity, and capability metadata before screens depend on them. The catalog must distinguish released, development, and installed firmware capabilities. Add modules only where implementation, resource use, and ABI impact are supported by evidence; do not infer a need for MCU GETFEATURES without protocol evidence.

**Results.** Shared data contracts, custom-image project format and verifier, generated release/capability catalog, source map, license inventory, and fixtures. Demonstrate identical acceptance/rejection behavior for the packer and Python reference implementation, including negative cases and exact output bytes. Validate schema and bundled references offline.

**Completion / stop.** No UI may enable a write from unverified metadata or a partial/ambiguous image. Stop if packers disagree, a negative fixture is accepted, or project-image policy cannot be checked independently of a warning.

## 3. Adapt the native client and browser in parallel

**Work.** Begin with Entropy’s native client and shared Rust model; develop the browser/Wasm target in parallel after stage 2. Keep browser feasibility visible as a release risk. Add exclusive device ownership, explicit selection when identity is not unique, live capability detection, full settings read, editors for supported layers, key and encoder mappings, macros, Combo and RGB, help, and RU/EN selection. Match AK820 behavior instead of copying unrelated Vial screens. Add backup import/export and local backup history. Cache public app files and help for offline use; keep personal backups out of the public cache and verify local persistence. Pin the visible UI/bundle version so a background update cannot change an active operation. Any cloud feature stays optional, isolated, and opt-in. OEM and recovery data always stay local. Remove analytics; do not collect telemetry.

**Results.** Offline browser build, shared backup fixtures, schema/help coverage, recorded-response transport tests, and checks for storage denial, disconnect, malformed response, timeout, and occupied device. These checks use a fake transport; they do not prove WebHID access. Saved settings use readback and stop on error; fixture tests are host evidence only.

**Completion / stop.** All supported controls and backup operations pass fixtures and fail-stop checks; offline UI and help open after initial caching. A failed read or persistence operation must never appear successful. No physical writes are part of this stage.

## 4. Add browser update, restore, and OEM rollback

**Work.** Implement release selection, stable/prerelease and revocation rules, custom image selection, compatibility preview, fresh settings backup, confirmed update flow, cancellation before erase, device reconnect, installed-image verification, and optional settings restore with readback. First release must perform actual flash, restore, and OEM rollback entirely in the browser. An installer fallback does not satisfy this requirement. Download and validate every input before erase; after erase begins, any timeout, storage fault, disconnect, or bad readback stops the sequence. Never retry, reset, blindly restore, or continue automatically. OEM restore uses only a validated image with proven ownership and never applies project settings over it.

**Results.** Offline host-model coverage for the operation state machine, negative fixtures for hashes, addresses, protected regions, truncation and mismatched identity, and a reviewed browser permission/loader design. Hardware checks are separate and require explicit owner confirmation for each transition or write.

**Completion / stop.** Host checks prove ordering and fail-stop behavior, not hardware. The browser feature is not release-ready until the required loader, flash, installed image, settings restore, and OEM rollback have been separately qualified on hardware with operation-specific authorization. Any unresolved identity or image-preparation ambiguity blocks writes.

## 5. Qualification, help, and first browser release

**Work.** Cover every supported behavior and remapping in the reusable suite, including both encoder directions. Regression checks use fixtures and require no physical key presses or knob movement. Include protocol parsing, backups, packaging parity, release/revocation, offline cache, local storage, and UI/help consistency. Help modules and catalog fields must match actual firmware support. CI builds and checks browser assets and fixtures offline; publish to GitHub Pages and firmware/catalog/help data to GitHub Releases. Report device behavior and durable storage separately from static checks and host models.

**Results.** Acceptance matrix, CI artifacts, release manifest and hashes, deployment instructions, and qualification record listing passed, skipped, and unproven conditions. The paired firmware release retains the existing standalone Windows/Linux x86_64 firmware installers, SHA256 files and RU/EN instructions. The shared desktop configurator has its own packages and release gate; neither product must wait for the other to finish.

**Completion / stop.** All acceptance rows applicable to the browser release have evidence and no unresolved release-blocking failure. Desktop and helper requirements have their own release gates and do not block the website. Do not claim a physical feature from source presence or a host-model pass.

## 6. Native release and optional helper

This stage runs alongside stages 3–5; native configuration and packaging can be completed and released before the website. Reuse the shared editor and model in a Windows, Linux, and macOS application. Reuse Entropy’s native egui client and the shared Rust model. Add offline installation, bundled firmware/catalog/help, and per-OS installers and instructions. Treat helper autostart, tray behavior, and active-layout detection as separate features. Prove three OS-layout mappings and supported desktop environments per platform. Treat the helper as conditional on firmware support. Define the three layout states and any firmware layout/lock module before implementation; require confirmed firmware changes, resource measurements, and stack-margin results. Coordinate the helper with browser flashing and suspend it during device operations. Measure application memory and startup on target systems; do not infer them from installer size.

**Completion / stop.** Release the native configurator after its applicable per-platform package, offline-use, HID and function checks pass. Add the helper in an application release once layout detection, background lifecycle and firmware-resource checks also pass. It may follow in a later version. Native release does not require website completion. Unsupported OS environments must be named plainly.

## Acceptance coverage

| Requirements | Evidence required before release |
| --- | --- |
| AKW-001–005, AKW-082: scope and foundation | Shared editor/transport prototype, Entropy reuse evidence, browser feasibility record, and RU/EN coverage |
| AKW-010–014: device and capabilities | Unique identity or blocked write; installed-image capabilities, not release assumptions |
| AKW-020–023, AKW-040–042: settings and help | Layers, keys, both wheel directions, macros, Combo, RGB, backup/restore and documented limits |
| AKW-030–034: releases and custom builds | Stable/prerelease/revocation behavior; schema, model, address, hash and protected-region checks |
| AKW-043–044, AKW-050–054: firmware and OEM recovery | Validated source/identity, browser loader, cancellation boundary, flash/readback/restore/rollback results |
| AKW-023, AKW-070–072: offline and privacy | Cached UI/help, verified local backups, no required account or upload, private diagnostic report |
| AKW-060–064: desktop and helper | Three OS packages, platform-specific HID/layout checks and measured resource use |
| AKW-080–084: checks and release | Cross-repository firmware/configurator identifiers, both-target CI, browser deployment, and a separate hardware qualification record |

This table summarizes coverage; it does not replace the detailed requirements.
