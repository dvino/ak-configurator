Русский перевод: [Требования](ru/REQUIREMENTS.md).

# AK Configurator Requirements

**Status:** agreed requirements for future work

**Version:** 0.8

**Date:** 2026-10-27

This document sets requirements for AK Configurator, maintained in the separate `dvino/ak-configurator` repository. It reuses Entropy, whose pinned upstream source is [5b222b5d6349fb351f53b36c66169095b3039fc1](https://github.com/ergohaven/entropy/tree/5b222b5d6349fb351f53b36c66169095b3039fc1). AK820 firmware remains in `dvino/ak820`. It does not confirm that these features have been implemented or authorize operations on a physical keyboard. Current source files, built artifacts, and installed firmware are checked separately.

The native desktop configurator is the first milestone. Browser requirements marked **Web** apply to a later stage that starts after the desktop editor is reasonably working. Standalone application requirements are marked **Desktop**. Each acceptance criterion is checked against the exact target image and stated keyboard model.

## 1. Goal and scope

**AKW-001 · Web.** Users can configure a supported keyboard and install firmware directly from the website. A separate installer does not replace browser-based flashing. The first version includes all user-facing firmware features already implemented: layers, key and encoder mappings, macros, Combo, lighting, and other supported settings.

**Acceptance:** all user settings and mappings for supported features are available in the editor. Actions that cannot be configured and relevant limits are described in the help. A build or release is blocked if a user-facing feature implemented in the firmware is missing from the editor and help.

**AKW-002 · Web/Desktop.** The current target is the wired AK820 keyboard with the VS11K28A controller in explicitly confirmed revisions. Other project-owned models may be added later after separate checks. A general Vial/QMK editor, bootloader replacement, and writes to protected areas outside the permitted image are out of scope.

**Acceptance:** flashing is available only for the listed model and a confirmed revision. An unknown device or protected area blocks the write and explains why.

**AKW-003 · Web/Desktop.** The configurator must support exact AK820 profile image contracts and must not infer write or installation authority from catalog membership. Beta.3 remains the default installer target. The beta.4.1 release catalog is pinned separately; beta.4.1 flashing is available only with explicit prerelease opt-in and exact catalog/matrix/per-image IHX and packed-CODE bindings. Hardware validation remains false, while saved-settings writes and restore remain unavailable. The beta.4 read-only catalog does not authorize flashing; per-profile flags never override release-level policy. Profiles are selected as complete images; there is no all-module image, arbitrary module switching, or compilation on demand.

**Acceptance:** profile membership, limits and measured resources agree with the firmware matrix and module descriptions. Catalog availability, release approval, standalone-installer inclusion, settings support and companion installation support are shown separately. Legacy beta.2 standalone-installer coverage does not establish beta.3 admission, companion installation or physical qualification. A profile name or a successful offline test does not enable writes.

**AKW-004 · Foundation.** Reuse Entropy as the foundation in the separate configurator repository. Develop and stabilize its native Rust application first. Begin browser/WebAssembly feasibility and implementation only after the desktop editor is reasonably working. Demonstrate browser feasibility before promising browser flashing. Keep any alternative as a separate future decision if Entropy cannot meet a required need; do not silently switch foundations.

**Acceptance:** record the pinned upstream revision, local changes, license, feature coverage, and evidence for native and browser feasibility.

**AKW-005 · Interface.** Keep the selected foundation's interface and prioritize feature completeness. Redesign the interface separately. All features and help are available in Russian and English.

**Acceptance:** switching RU/EN changes all user-facing labels and help, and the selection is remembered. On first launch, the browser or operating-system language selects the language for its target.

## 2. Connection and firmware capabilities

**AKW-010 · Web.** The website uses a desktop Chromium browser with WebHID for direct connection. If the API is unavailable, the site explains the requirement; help remains available. A desktop application is also offered after release.

**Acceptance:** the supported browser, missing WebHID, and access denial are checked. None of these failures starts a write.

**AKW-011 · Web/Desktop.** The user selects a device. Reconnection is automatic only when permission is saved and the match is unambiguous; otherwise, the configurator asks the user to select a device. Selecting again after entering the bootloader is allowed. An ambiguous device receives no write commands.

**Acceptance:** identity and permission are checked before each write operation. If there is any doubt, the operation is blocked.

**AKW-012 · Web/Desktop.** Capabilities are determined from the exact installed image. Features for missing modules are hidden. A new feature in an already enabled module is marked “Available after update” and has an update button. A release's capabilities are not attributed to an older or custom build.

**Acceptance:** the interface, help, and available actions match the catalog entry for the current image. Unknown firmware blocks setting changes; unknown capabilities in a custom build also block the editor.

**AKW-013 · Web/Desktop.** Only one configurator instance, browser tab or helper can own an operation on the device at a time. Other instances say that it is busy. If Vial or another application blocks the connection, the configurator explains why and suggests closing it; the user retries the connection with a button.

**Acceptance:** a busy device receives no parallel commands, and retries do not happen automatically.

**AKW-014 · Web/Desktop.** A disconnect during an operation stops it. After reconnection, the state is checked again. Erase, write, and flash operations are never retried automatically.

**Acceptance:** after a disconnect, the interface does not show success or continue without a new check and confirmation.

**AKW-015 · Image contract.** Join the beta.3 capability catalog, firmware matrix and RU/EN module descriptions by stable IDs and exact IHX identity. Preserve configuration, schema, enabled modules, table counts, active QMK Settings IDs, keycode source restrictions, settings identity and allowed transitions. Distinguish the linked CODE-window fingerprint from permanent packed CODE SHA256 over `[0x2800, 0xC700)`. Only a separately trusted exact packed identity can authorize settings matching; missing identity blocks mutation while leaving catalog preview available.

**Acceptance:** fixtures reject mismatched catalogs, duplicate or unknown contract fields, missing/changed image identity, unsupported modules and invalid transitions. Runtime data is not inferred from a profile title. All ten profiles can be selected in the offline editor without selecting a device or changing its active image.

## 3. Settings and help

**AKW-020 · Web/Desktop.** A setting change is applied immediately, then read back and compared with the expected value. An error or mismatch stops further changes. “Read settings again” shows the actual state; editing is available after a successful read. Writes are not retried automatically.

**Acceptance:** successful readback, an error, timeout, readback mismatch, and a second manual read are checked.

**AKW-021 · Web/Desktop.** Users can undo the last key or encoder mapping (both directions and the press action), restore the previous lighting, reset the current firmware's settings with confirmation and a prior backup, and test keypresses received by the page without writing to firmware.

**Acceptance:** an undone mapping is read back. The test clearly explains that the browser intercepts some key combinations and that the test does not prove all physical keys or both encoder directions work.

**AKW-022 · Web/Desktop.** A setting's ⓘ icon shows a short tooltip on hover; clicking it opens the related help section. Tooltips are keyboard-accessible. Help explains actions, examples, and module limits.

**Acceptance:** every visible feature has current help. Missing QMK Settings is not described as a connection error; support for NKRO reports is not described as NKRO control; temporary RGB state in RAM is not described as a saved setting.

**AKW-023 · Web.** After the first load, the site can use settings, local backups, and saved help offline. New releases are downloaded from the network; firmware can be selected from a local file.

**Acceptance:** after caching, the editor and help open offline. Availability of a new release offline is not promised.

**AKW-024 · Profile editors.** Expose the common layer, matrix, encoder, macro, Combo and RGB controls and all actions supported by the selected exact image. Preserve source restrictions: physical matrix, encoder, Combo and macro tap/down/up are distinct contexts. Beta.3 Combo supports two distinct ordinary-key triggers and CapsWord as a result. The `LT(layer0..5, MC0..15)` selector appears in the Macro UI group but the action is valid only on the physical matrix, not inside a macro stream or on an encoder; beta.2 remains separate. Optional modules are controlled by the image contract, including NKRO control, One Shot, Space Cadet, Leader, Dynamic Macros, Mouse Keys, Auto Shift, GUI lock, modifier swaps and OS indicators. Tap Dance and Key Overrides each have eight typed entries only in their matching schema03 profiles. QMK Settings exposes active advertised IDs in each exact profile contract. For beta.3, AutoShift QSID 4 is active in Main and Typing; beta.2 Main remains unchanged. Reserved storage never enables a control.

**Acceptance:** match the exact firmware source and image contract selected by the catalog. Beta.2 compatibility remains a separate legacy path; beta.3 uses its own profile capabilities and explicit transitions. Validate ranges, reserved bytes, CRC, defaults and per-entry terms before writing. Typed setters verify ACK, getter and complete NVM readback including adjacent bytes. Unchanged effective defaults preserve blank storage. Temporary Dynamic Macros, session modes and VialRGB Direct colors are identified as volatile. Unsupported controls remain unavailable in both the editor and key picker.

**AKW-025 · Desktop RGB.** The companion provides a physical keyboard lighting editor with individual-key and group selection, color/brightness controls and local presets. Add native effects, music visualization, useful host indicators, game events and optional OpenRGB SDK interoperability in the phases defined in [RGB-PLAN.md](RGB-PLAN.md). Prefer complete existing open-source components and compatible plugins over new audio analysis or general effects engines. Manage bundled components from the configurator so ordinary keyboard lighting does not require launching several applications. Use the existing VialRGB protocol and one HID owner; SDK networking does not replace image verification. Direct mode is available only on profiles that advertise it.

**Acceptance:** validate all 82 LED mappings and exclude the unlit encoder press. Keep 83 key positions distinct from LED count. Bound HSV V to 160, reads to seven LEDs and write chunks to eight LEDs. Local presets, acknowledged RAM commit, readback, volatile `direct-ram.json` save/restore and persistent settings are separate states. Beta.3 provides a Direct getter and atomic full-frame RAM commit; neither implies NVM persistence or physical LED scan qualification. No per-frame NVM writes. Audio/screen capture is local and opt-in; actual game events require a supported event source. Unknown external input and a device error cannot bypass fail-stop or operation ownership.

## 4. Releases and image compatibility

**AKW-030 · Web/Desktop.** Release, feature, help, and change description data come from published project materials. By default, the latest stable, non-revoked release is offered. Pre-releases appear behind a separate checkbox; if there is no stable release yet, installing a pre-release requires that checkbox to be enabled. Checking for an update does not start installation.

**Acceptance:** user-visible changes and their effect on settings are shown separately before installation. Installation starts only after a user action.

**AKW-031 · Web/Desktop.** Earlier non-revoked versions are available, with settings compatibility checked. A known revoked file is blocked even when selected manually. When online, the site reports if the installed version has been revoked. Offline, it uses the latest saved information, and a later revocation may be unknown.

**Acceptance:** a revoked file cannot be installed. Offline status is not presented as a current revocation check.

**AKW-032 · Web/Desktop.** A custom build is accepted only in the project format. The format, model, addresses, checksums, and protected areas are checked. A damaged, incompatible, or unverifiable file is blocked without a way to bypass the block. A warning that the project has not checked a build does not replace these checks or make the build official.

**Acceptance:** the exact selected file is checked; information about its features and settings applies to that file. If settings are incompatible, the user is offered a backup and default values.

**AKW-033 · Web/Desktop.** All files needed for flashing are fully downloaded and checked before erase. No internet connection is needed to finish afterward. IHX, CODE, full ROM, and package hashes remain separate values; IHX is not presented as a ready ROM.

**Acceptance:** before erase begins, the actual bytes, target image, and required files are checked. Losing the network after that point does not prevent completion.

**AKW-034 · Catalog.** The editor, help, backup checks, compatibility checks, and release process use one catalog. It contains a registry of features and modules, plus a catalog of exact images with model, confirmed revision, settings schema, features, compatible transitions, release status, and file hashes. Stability, project verification, and revocation are separate properties.

**Acceptance:** CI checks the catalog against release files. The latest version is selected with pre-releases taken into account, not only from the moving `releases/latest` link. A manifest asserts identity but does not replace checking the actual CODE.

## 5. Settings backups and OEM firmware

**AKW-040 · Web/Desktop.** The website and application use one backup format containing all supported, saveable settings: key and encoder mappings, macros, Combo, lighting, and required extensions. RAM-only state is not described as saveable. The file records the format version, source image, and data needed to check compatibility.

**Acceptance:** before applying, the file is checked for format, ranges, mappings, and compatibility. Incompatibility is explained along with the required version.

**AKW-041 · Web/Desktop.** A backup can be downloaded, imported from a local file, viewed, restored, or deleted. Before import, the changes are summarized and the current state is backed up. “Undo settings import” restores the previous configuration and checks the result. Automatic backups made before flashing, reset, or import are shown with their date and reason.

**Acceptance:** backup creation failure is clearly shown as an error. Settings import and reset stop if a safety backup cannot be created or read. Only for a firmware update, the user may explicitly choose to continue with default settings after a warning if the full backup could not be read. Incompatible data is never silently deleted or truncated.

**AKW-042 · Web/Desktop.** Settings are backed up automatically before an update, then restored and checked afterward. The “Start with default settings” checkbox disables restoration but not backup creation. When returning from OEM firmware, the user is offered a compatible saved backup or default values.

**Acceptance:** a restoration failure is shown separately from the firmware result; the original backup remains available.

**AKW-043 · Web.** Before the first installation from OEM firmware, the site attempts to read, check, and save the factory image locally in the browser and as a separate file. Saving this backup is optional for installation. If it is missing, a warning that return to stock firmware will not be possible is shown before selection and again at confirmation. Updates do not overwrite this backup, and deleting it requires separate confirmation and an offer to download the file.

**Acceptance:** an unverified OEM file cannot be used for rollback. If the browser copy is missing, the selected file is opened and checked before use. The user is told that local browser data may be lost.

**AKW-044 · Web/Desktop.** Even without a saved factory backup, the packer must read and check required device-specific data and prepare a safe image. The absence of an archived backup alone does not block installation; inability to prepare the image safely blocks it with a specific reason. ROM and recovery session data stay local.

**Acceptance:** all required device-specific data and protected areas are checked before writing; an archived file does not replace these checks.

**AKW-045 · Backup compatibility.** Port `ak820-settings-backup-1` for schema02 and `ak820-settings-backup-2` for schema03 from the firmware reference into shared Rust code; Python is not a runtime dependency. Preserve complete typed extension storage, including inactive values. Same-image restore requires exact reviewed identity. A different image requires an explicit source/target IHX pair in `settings_transitions` and all schema/module/value checks; a shared schema or profile name is insufficient.

**Acceptance:** golden fixtures agree with the Python reference on bytes, defaults, errors and compatibility. Reject duplicate, unknown or missing fields; schema03-to-schema02 restore; nonempty inaccessible tables; and unsupported stored settings before the first write. Schema01/02-to-schema03 migration requires an explicit target and preserves supported settings, including Combos. A partial `.entlayout` snapshot does not satisfy complete backup or undo-import requirements.

## 6. Firmware installation and OEM rollback

**AKW-050 · Web.** The complete first-version workflow runs on the website. A separate installer does not satisfy this requirement. Browser flashing is not considered ready unless a safe path through the factory bootloader has been confirmed.

**AKW-051 · Web/Desktop.** Before writing, select an image and check the device, compatibility, revocation status, and files; prepare backups and the final image; show the operation contents and effect on settings. After files are downloaded and checked, warn the user to keep power stable and not disconnect the keyboard. Recheck the device and pause the helper before erase. After writing, reconnect, check installed contents, restore selected settings, and read them back.

**Acceptance:** success is shown only after checking the image and selected settings. A bootloader checksum alone does not prove successful installation.

**AKW-052 · Confirmation.** If the current image has a lock, a fresh physical hold of Esc and left Ctrl for three seconds confirms the specific operation. The site shows unlock progress; an earlier unlocked state does not count as confirmation. If the lock module is disabled or this is the first installation from OEM, a second explicit confirmation with the “Start flashing” button is required. Typed text, a separate Vial app, and a manual bootloader transition are not required. Settings restoration is part of the confirmed plan.

**Acceptance:** confirmation applies to the displayed image and action. Changing the image requires new confirmation.

**AKW-053 · Web/Desktop.** The operation can be cancelled before erase. Cancellation is unavailable after erase starts. On timeout, disconnect, storage error, or readback mismatch, the operation stops: no blind restore, automatic retry, reset, or continuation. The web page warns against closing or reloading; the application blocks ordinary closing until the operation ends.

**Acceptance:** each failure leaves the state clearly unknown or failed and never shows success. A new action is possible only after checking the state and obtaining any required confirmation.

**AKW-054 · Web/Desktop.** OEM rollback uses a factory dump saved in the browser or selected manually. Its ownership by the connected keyboard must be confirmed; the filename and VID/PID are not enough. Only permitted areas are written; the bootloader is not overwritten and project firmware settings are not applied. Project settings backups are kept.

**Acceptance:** rollback is blocked unless dump ownership is proven; success is checked after reconnection.

## 7. Standalone application and system helper

**AKW-060 · Desktop.** Release a native application for Windows and Linux x86_64 and arm64, and macOS arm64 and x86_64. Stabilize the native editor first, then begin the WebAssembly/browser stage; desktop release does not depend on website readiness. Use Entropy’s native application and shared Rust model. The application includes compatible firmware, all curated AK820 profiles, the compatibility catalog, and help. Configuration, installation of bundled or local firmware, backups, and rollback using a suitable dump work offline.

**Acceptance:** record minimum supported OS versions, Linux package/runtime dependencies and X11/Wayland limits before release. Build, USB connection, memory use, startup speed, and required features are confirmed separately for each OS/CPU pair; a build on one target does not confirm another. A CI runner label alone does not define OS compatibility.

**AKW-061 · Desktop.** Application updates are offered separately and started only by the user; they do not start firmware flashing and preserve helper settings. If new firmware needs a newer configurator, offer the application update first. A compatible bundled firmware remains available offline.

**Acceptance:** an application update sends no commands to the keyboard.

**AKW-062 · Desktop.** A small helper runs in the background. The full interface is created when opened and released when closed. The layout helper shows the active window's system keyboard layout, including when different applications use different layouts. The helper and its autostart can be enabled or disabled.

**Acceptance:** helper settings (layout mappings, enabled state, and autostart) are stored separately from the keyboard and survive its firmware reset or update.

**AKW-063 · Desktop.** Users map system keyboard layouts to three OS LED states: off, on, and slow breathing. One state may be assigned to several layouts. The helper sends the state; firmware performs the breathing effect. If the helper stops, the connection is lost, or the layout cannot be determined, firmware returns the LED to its normal behavior; the menu shows the reason.

**Acceptance:** all three states, connection loss, and an unknown layout are checked. The helper periodically confirms that it is connected.

**AKW-064 · Desktop.** The helper pauses device access while firmware is flashed or rolled back from the application or website. After the operation is checked, it resumes indication. After an ordinary USB reconnection, it resumes without opening the configurator.

**Acceptance:** while an operation is in progress, the helper does not compete for the device or send commands.

## 8. Privacy and diagnostics

**AKW-070 · Web/Desktop.** There is no analytics, telemetry, hidden collection, or automatic error reporting. Settings, macros, and backups stay local by default; core features do not require an account or cloud service. Optional cloud features are off by default and enabled only by an explicit choice after explaining their purpose, recipient, and data. OEM ROM and recovery session data are never sent to the cloud.

**Acceptance:** no data is sent unless a feature is enabled. Each connected provider is checked separately for its terms and data handling. Encryption and provider security are not promised before verification.

**AKW-071 · Web/Desktop.** Without cloud features, network access is used for releases, the catalog, help, and application updates. An explicitly enabled OpenRGB SDK source may listen on loopback only; it accepts bounded lighting input through the existing verified device owner. The application contains no GitHub tokens. Downloads are checked; arbitrary HTML is not executed, and external links are not treated as firmware sources without project rules. A hash from the same source detects corruption but not compromise of that source.

**Acceptance:** incoming files and the absence of client-side secrets are checked; published firmware is tied to a verified catalog.

**AKW-072 · Web/Desktop.** An error gives a short reason and lets the user download a technical report with versions, operation stage, checks, and details. The report contains no macros, keyboard layout, factory dump, tokens, or personal paths. An issue link opens for manual submission; the site does not publish anything.

**Acceptance:** the report is checked for the listed data; submission requires a separate user action.

## 9. Checks, CI, and release

**AKW-080 · CI.** A feature ships with configurator support and help. CI checks consistency among the catalog, files, documentation, commands, final image memory use, backups and compatibility, revocations, transport errors, and packaging. A packer port requires bit-for-bit comparisons on identical inputs and matching failures on invalid data.

**Acceptance:** inconsistencies block release. A feature is not ready until the configurator and help are ready. Memory checks use complete final images, not estimates of feature costs in the interface.

**AKW-081 · Checks.** Static checks, builds, CPU/host models, protocol checks, and device tests are tracked separately. Host tests do not prove bootloader behavior, physical input, durable storage, or both encoder directions. Default test commands are offline or read-only; changing settings requires an explicit run option and separate authorization. A timeout, storage error, or bad readback is a failure that stops the operation, with no blind restore, retry, or reset.

**Acceptance:** reports name skipped checks and physical behavior the tests did not confirm. No CI result is presented as a device test. Readback errors are reported as a mismatch between expected and actual values.

**AKW-082 · Foundation check.** Confirm reuse of Entropy at pinned revision `5b222b5d6349fb351f53b36c66169095b3039fc1`. Test the AK820 editor and readback through a fake transport, and separately demonstrate a browser/Wasm build. Check WebHID, loader, and OEM paths separately; entering the loader, reset, erase, and writes on a physical device require separate authorization.

**Acceptance:** record the source revision, licenses, feature gaps, native and browser feasibility evidence, and untested conditions. A launching interface alone does not prove an operation works.

**AKW-083 · Release.** Before a commit, obtain an independent review of code, documentation, and verification evidence. Close findings and recheck substantive fixes. The standalone firmware installer is a temporary path while the companion is being implemented; retire it after the companion can safely select, validate, install and verify all supported profiles. Companion builds target Windows and Linux x86_64 and arm64, and macOS arm64 and x86_64, with SHA256 files and RU/EN instructions. Instructions explain package selection and download, checksum verification, extraction and launch.

**Acceptance:** instructions match the actual packages and commands. The site offers a button to reload an updated version. It does not reload the page during a write, firmware installation, or rollback.

**AKW-084 · Hosting and branches.** AK Configurator source lives in the separate `dvino/ak-configurator` repository, with Rust sources under `src/` and browser platform adapters alongside the native platform code. `main` is the shared baseline, `desktop-dev` carries native development, `desktop` prepares native releases, and `web-dev` carries browser work in the configurator repository. The `dvino/ak820` repository remains the firmware source.

**Acceptance:** CI checks shared model, protocol, format, help, and exact firmware compatibility across both targets. A coordinated release uses common metadata and compatibility identifiers and tests both targets; hardware qualification remains separate. Native releases are not blocked by website readiness.

**AKW-085 · Offline completion.** Complete every development and acceptance step that does not need a keyboard: ten-profile offline editors, typed codecs, backups/migration, release and revocation rules, packer parity, operation state machines, localized help, negative fixtures, and six-target builds/packages. Use fake transports and approved non-sensitive fixtures; unavailable OEM inputs must not be replaced with invented acceptance evidence.

**Acceptance:** the report maps every applicable requirement to code and tests with passed, failed, missing, skipped or hardware-gated status. Cover complete successful image verification through full settings loading, both encoder mappings, every capability/source context and failure at every mutation stage. Hardware-only gates name the exact remaining operation: identity/USB behavior, fresh unlock, flash/readback, physical input or persistence. Offline preparation is not blocked merely because hardware qualification remains open.

Readiness for a specific release is determined by the requirements above and the work plan in [PLAN.md](PLAN.md). Acceptance covers successful scenarios and safe stops for a damaged file, wrong device, incompatible backup, revocation, timeout, and readback mismatch. Firmware update checks separately cover continuing with default settings after a full-backup read failure and explicit warning, installation without a factory backup with a warning, and blocking rollback with an unsuitable dump. The site and application show only capabilities confirmed for the exact model and image.
