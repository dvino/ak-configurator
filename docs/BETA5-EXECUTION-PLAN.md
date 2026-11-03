# AK Configurator: beta.5 execution plan

Status: execution-ready planning; implementation has not started.
Snapshot: 2026-11-01. Branch: `desktop-dev`.
Baseline: `936a989a8fd7d8e24cbd70ddba0e768e3107494b` plus the recovered uncommitted artifacts.

Recovery provenance and the issue-to-code audit are recorded in [RECOVERY-REVIEW-2026-11-01.md](RECOVERY-REVIEW-2026-11-01.md).

Execute the stages in order. Do not redesign settled decisions, broaden scope or claim physical validation from offline tests. Every commit requires independent review of code, documentation and evidence. Without review, leave changes uncommitted.

## Fixed rules and architecture

- Use `desktop-dev`; merge shared work normally. Isolate worktrees and `CARGO_TARGET_DIR`; do not share build output.
- Do not copy the firmware tree or OEM ROM/recovery data. No device write, reset, loader transition or flash without separate operation-specific authorization.
- After timeout, storage fault, readback mismatch or failed write: stop, preserve uncertain state, never blindly retry/reset/restore.
- Packed `code_sha256` is not linked CODE fingerprint. Catalog membership, profile names and fixtures never establish image admission.
- `settings_writes_authorized=false` remains authoritative. Transition pairs do not authorize settings restore.
- Keep EN/RU behavior and limitations synchronized. Record skipped hardware/platform checks.
- Cargo declares a binary, not a library. Run unit/headless tests with `cargo test --bin ak-configurator <filter>`; existing egui harnesses live in `src/ui/*`. Add new frame regressions there, not a nonexistent integration target. `cargo test --bin ak-configurator ui::` covers those UI modules. Before accepting a filtered run, inspect its test list/count and reject zero matching tests. The only optional features are `mcp` and `mcp-mock`; native tests need neither.

Use a minimal extension, not a generic registry:

1. Add `ImageContract::Beta5(ProfileId)`, a pinned `assets/ak820/beta5-profiles.json` and `beta5_firmware_matrix()` adapter. Recognize only after full installed-CODE SHA256; ambiguous/unknown identity denies access.
2. Replace `InstallerRuntime::beta4_1: bool` with `enum InstallerRelease { Beta3, Beta4_1, Beta5 }`. Store the selected release in `ReleaseAdmission` and `ReleaseObservation`; generalize shared catalog checks rather than cloning `beta5()` methods. Beta.5 is the default UI target with explicit prerelease opt-in.
3. Add `RecordVersion::V3`, `ImageVersion::V3`, QSID 23 and a tenth field: header `51 53 03 13 9f`, offset 20, little-endian u16, range 1–1000. Replace implicit v3 defaults with `defaults(version, leader_timeout_default)`; pass the exact image default (normally 300). V3 accepts v1/v2/v3; v1/v2→v3 appends the default and recalculates CRC; downgrade is rejected. This is offline conversion only.
4. Read QSID 23 through `get_qmk_setting_u16`, never the u8 list `1 | 3 | 22`.
5. Reuse `VialHidOperation`/`HidHelper`; no second transport. Page availability is `Ready | OperationBusy | LeaseBusy | Disconnected | Failed(reason)`. Keep generation/request IDs on every result.
6. Beta.5 initially supports exact read-only reconciliation and backup. Do not expose live writes for new Tap Dance, NKRO or Dynamic Macros. New keycodes remain offline-editor only under the exact keycode source. QS23 editing is a stated later limitation.

Execution is partitioned by [SESSION-EXECUTION-PLAN.md](SESSION-EXECUTION-PLAN.md): B00–B12 are bounded implementation sessions, C cards share their domain work, Q/H separate offline platform and admitted physical evidence. Do not implement an entire broad stage in one session when it has multiple cards.

## Stage 0 — freeze final beta.5 inputs

Dependencies: firmware CI and republished assets. Files: `assets/ak820/beta5-*`, `.github/scripts/test_beta5_catalog.py`, verification report.

1. Wait for successful CI and publication at an explicitly identified commit. Runs 41294887078 and 41294883546 at `c9e0220c25f664c4a470faa6700059f6aedf520c` have failed. Latest observed replacement candidate is `2ab372dab7f084b76b57aa98a9543c249a6d0c30`, runs 41297661542 and 41297648433 still in progress on 2026-11-01; `gh release view beta.5 -R dvino/ak820` returned `release not found`. Recheck these facts before pinning; neither candidate nor running CI is success evidence.
2. Download the newly published catalog, capabilities, matrix, module descriptions and bundle over HTTPS. Record tag, commit, run URL, sizes and SHA256.
3. Require `settings_writes_authorized=false`, ten expected profiles, exact IHX/CODE bindings and exactly 200 directed pairs: beta.4/4.1→beta.5 and beta.5→beta.5. Deny beta.3 and unknown sources.
4. Reconstruct every profile with the official pinned firmware packer into private temporary output. Its current CLI has no `--check`: `python3 "$FW/qmk-8051-ak820/util/pack_ak820.py" --ihx "$IHX" --map "$MAP" --stock-rom "$STOCK_ROM" --native-reference "$NATIVE_REFERENCE" --native-rom "$NATIVE_ROM" --out "$PRIVATE_OUT"`. Resolve IHX/map from each final profile; require separately validated local OEM/native inputs and a fresh nonexisting output directory per profile. These arguments are file inputs only, never device operations. Verify IHX/map digests separately, then compare reconstructed permanent packed CODE bytes/hash with the published profile identity; IHX text is not packed bytes. Require 10/10. The packer also emits private recovery/full-ROM files: never copy outputs, OEM inputs or raw reports to the repository. Retain metadata/digests only and clean only owned disposable outputs after checking they are unused. Missing local inputs, changed packer CLI or mismatched output stops this gate; do not substitute synthetic OEM or linked fingerprints.
5. Replace the old staged JSON with final bytes. Extend the catalog test to verify external pinned digests and all 200 pairs, not just internal links. Do not embed the bundle.
6. Run `python3 .github/scripts/test_beta5_catalog.py`. No Rust beta5 catalog test exists at this stage; add Rust admission tests only with stage 1 integration.

Gate: missing final hashes or less than 10/10 pack checks stops stage 1. Report the blocker; never substitute old values.

## Stage 1 — offline QS v3 and beta.5 profiles

Files: `src/ak820_qs_versioned_offline.rs`, `src/ak820_settings_model.rs`, `src/ak820_typed_storage.rs`, `src/ak820_restore_plan.rs`, `src/ak820_settings_backup.rs`, `src/ak820.rs`, `src/ak820_beta1_profiles.rs`, offline tests.

1. Implement the QS and profile decisions above without enabling a writer.
2. Add independent firmware-Python golden v3 defaults/maxima; test Leader 1/300/1000 and reject 0/1001/65535.
3. Test v1/v2/v3 roundtrip, erased 00/FF, v1/v2→v3, forbidden downgrade, CRC/header/reserved corruption and property/fuzz decoding. Test synthetic v2→v3 restore preparation without production restore.
4. Verify ten profiles and final capability/matrix limits; do not inherit beta.4.1 values silently.
5. Run `cargo fmt --check`, `cargo test --bin ak-configurator ak820_qs_versioned_offline`, `cargo test --bin ak-configurator ak820_settings_model`, `cargo test --bin ak-configurator ak820_restore_plan`, `cargo test --bin ak-configurator beta5` and `cargo clippy --all-targets -- -D warnings`. The beta5 filter must match tests added in this stage.

Acceptance: all tests pass; no production write is enabled; EN/RU docs call migration offline-only.

## Stage 2 — recognition, connection and full snapshot (#2)

Files: `src/ui/ak820_connection.rs`, `src/ak820_settings_backup.rs`, `src/ak820_backup_capture.rs`, `src/ui/ak820_backup_inspection.rs`, `src/ui/vial_hid_task.rs`, `src/ui/ak820_status.rs`, EN/RU catalogs.

1. Extend every `ImageContract` match and `contract_for_hash`; require CODE digest, never VID/PID/name/schema alone.
2. Use image v3 and explicit defaults. Reconcile full NVM, published macro buffer, QSID list and all values including u16 QSID23. Mismatch leaves read-only failure with a specific reason.
3. Store observed source identity, selected target release, catalog digest, CODE digest, source/target QS versions and `settings_writes_authorized=false` in backup metadata. Never call a snapshot atomic. Beta.4 and beta.4.1 share IHX/map/packed CODE identities (`src/ak820_beta1_profiles.rs`): use canonical observed `ImageContract::Beta4` plus an explicit `beta.4 / beta.4.1 (byte-identical)` alias label. Device bytes do not identify the installation package. A trusted local installation receipt may add separately labelled package provenance, never replace observed identity; target selection never supplies source provenance.
4. Distinguish lease contention, operation busy and disconnected. Preserve fully verified capture results across later navigation where possible; retain the original capture error instead of generic disconnected text.
5. Keep beta.4/4.1→beta.5 restore and factory reset blocked; 200 installer pairs are not restore authority.
6. Add synthetic beta.4.1/beta.5, unsupported CODE, v1/v2/v3, macro/QS23 mismatch, lease, busy, disconnect and stale-generation tests.
7. Run `cargo test --bin ak-configurator ak820_backup`, `cargo test --bin ak-configurator ak820_connection`, `cargo test --bin ak-configurator beta5` and `cargo test --bin ak-configurator ui::`.

Acceptance: all three availability states and readable EN/RU causes are tested. Add inspection fixtures for the earlier `ak820-settings-backup-2` manifest/settings/NVM/full-ROM structure without copying OEM data: compatible preview when policy permits, otherwise the precise image/version/policy incompatibility, never generic disconnected. The issue #2 comment explicitly separates snapshot failure from untested blocked restore; restoring settings remains denied. Record offline repair separately; close the live Windows report only after read-only snapshot admission and specific preview/denial evidence on exact firmware. USB behavior is not proved by fixtures.

## Stage 3 — bounded asynchronous RGB (#3)

Files: `src/ui/rgb_settings.rs`, `src/ui/vial_hid_task.rs`, `src/hid_ak820.rs` only for diagnosis, RGB tests.

1. Remove synchronous `hid.set_vialrgb_mode` calls from all four RGB setters. Add one `VialHidOperation::Ak820RgbWrite` carrying a full effect/speed/H/S/V snapshot, request ID, generation and old value.
2. Validate exact beta.5 identity, writable authorization and ranges before submission. Keep exactly one submitted command and at most one unsent coalesced draft: a new UI change replaces only that unsent draft; after it is submitted, a newer change may become the single new draft. Never grow an unbounded queue or cancel/replay a sent command.
3. Worker sends exactly one command and performs the existing required readback. Timeout/error/mismatch terminates without autosave retry; UI retains old/uncertain value.
4. Disable RGB controls while submitted but keep window repaint running. Stop the scene before queueing.
5. Fake delayed-HID tests: five-second delay does not stop frames; rapid changes yield at most one submitted command plus one unsent draft; timeout has no retry; stale generation is ignored.
6. Run `cargo test --bin ak-configurator rgb`, `cargo test --bin ak-configurator vial_hid_task` and `cargo test --bin ak-configurator ui::`.

Acceptance: offline tests prove no sync wait/retry and safe Quit/window close during delayed/failed worker completion, with no further command on exit. For #3 record chosen effect, exact image/candidate, active owner, request/result or timeout and close behavior without private payloads. Keep #3 open until Windows physical evidence exists; every live RGB action needs fresh write authorization.

## Stage 4 — finite unlock (#4)

Files: `src/vial/unlock.rs`, `src/ui/vial_hid_task.rs`, unlock tests, EN/RU messages.

1. Add a 15-second deadline after successful `UNLOCK_START` and a `TimedOut` state. Keep 200 ms polling; timeout must not depend on repaint.
2. On first poll transport error stop the session with the real cause; do not log “retrying” forever or resend start automatically.
3. Timeout tells the user to release keys, check keyboard response/lighting, close and retry explicitly. No automatic reset/reconnect.
4. Allow cancel before start. After start, “Stop waiting” ends only UI waiting and does not claim firmware cancellation. Exit issues no new commands.
5. Test already-unlocked, correct/wrong hold, regressing counter, poll error, virtual-clock timeout, close before/after start and generation change; Russian message mentions checking the keyboard.
6. Run `cargo test --bin ak-configurator vial_unlock` and `cargo test --bin ak-configurator unlock`.

Acceptance: every path is terminal with no automatic retry. Include simultaneous background layout/RGB ownership contention and an already-unlocked response; UI must explain the required physical hold and started/completed/rejected/timed-out states. Record exact candidate/image, UI step and protocol outcome. Offline boundedness does not prove the reported unlock failure fixed: keep #4 open until an exact-image Windows unlock check is separately authorized and observed.

## Stage 5 — preserve UI during OS-indicator refresh (#5)

Files: `src/ui/ak820_os_indicator.rs`, `src/ui/device_connection.rs`, `src/ui/ak820_status.rs`, `src/ui/ak820_backup_inspection.rs`, indicator/native layout tests. Current code uses `Job::Sample`, `Job::Write`, `poll_ak820_os_indicator_task` and `tick_ak820_os_indicator`; there is no `LayoutPoll` or `poll_layout()` API. Do not create a second polling path.

1. Preserve existing sample generation/freshness checks and HID loan cache (`ak820_os_indicator_display_connected`), plus tests `unchanged_layout_samples_keep_hid_and_ui_availability`, `competing_work_discards_sample_before_borrowing_hid` and `indicator_hid_loan_preserves_cached_page_shapes_without_writes`. These are partial prior fixes, not full #5 closure evidence.
2. Add test-only scheduling/result injection for Sample and Write completion without real HID I/O. Keep OS sampling independent from HID ownership. While a verified same-generation loan is in flight, render connected metadata from the cached verified facts; do not treat temporary `hid_device.take()` as physical disconnect. A real disconnect or generation change invalidates the cache immediately.
3. Apply that existing connected-display predicate consistently to the app-version/footer, service-page routing and Backups availability. Keep page/dialog IDs and capture state independent of sample ticks. Never open/close/rebuild a view merely because the helper borrowed the owner; busy pauses controls, not navigation state. Change only predicates/state transitions that fail the new regression; do not invent a separate identity cache or alter write authorization.
4. Add metadata-only debug traces for sample/completion, loan begin/end, generation, selected page and availability reason, without settings or payloads. Inject stale-generation results and competing work; assert no saved mapping/draft changes, no new write and no fabricated disconnect.
5. Add an egui multi-frame regression harness with Main and Backups open separately: sample a constant EN layout for 100 ticks, then alternate EN/RU for 100 ticks. Record app-version text presence every frame, selected page ID, backup inspection/capture state, modal ID/open state, connection generation, availability and owner changes. Assert the version never disappears, pages/dialogs are not recreated or closed, capture state persists, and a poll alone never produces `Disconnected` or a generation change. Only detection text/LED status may change. Repeat with operation busy and lease busy: show paused, not disconnected.
6. Run `cargo test --bin ak-configurator ak820_os_indicator` and `cargo test --bin ak-configurator ui::`, including the new multi-frame regressions. For live acceptance use the installed Windows candidate from run 40632342950 as the reported baseline, then the fixed candidate with exact firmware/CODE identified. Observe Main and Backups during steady layout and repeated EN/RU switches; record frame/time evidence and availability transitions. Do not enable indicator writes or change saved mappings for this check; if the inherited helper writes automatically, keep it disabled and record that live indicator coverage is pending operation-specific authorization.

Acceptance: sample/loan unit tests alone prove only bounded helper behavior, not closure of #5. Offline acceptance requires the full multi-frame stability assertions; keep #5 open until Windows read-only observation confirms no version/Backups flicker, false disconnect or view reinitialization. A screenshot alone cannot establish timing stability.

## Stage 6 — finish native navigation (#6)

Files: `src/ui/ak820_status.rs`, `src/ui/settings_shell.rs`, `src/ui/layout_settings_dropdown.rs`, `src/ui/app_settings.rs`, `src/ui/ak820_backup_inspection.rs`, `src/ui/ak820_offline_extensions.rs`, `src/ui/ak820_install_files.rs`, `src/ui/ak820_install.rs`, EN/RU catalogs/docs. Preserve the existing native pages; the audit below is the execution contract, not an assignment to redesign navigation.

| Issue requirement | Current implementation / gap | Exact remaining change | Verification |
|---|---|---|---|
| Persistent firmware/install, backup/restore and app settings pages | Native service routing exists in `ak820_status.rs`; installer/files/backups were integrated by `936a989`; `settings_shell.rs::open_app_settings_page` opens AppSettings. | Keep these routes and state; remove only residual modal-only duplicate entry points. | Open each from the menu; assert main-window page ID, stable state across Back/reopen, keyboard focus and no replacement top-level window. |
| Selected device and verified source/target | Installer has source ROM SHA and target profile; Beta4 recognition cannot distinguish byte-identical beta.4/4.1 packages. | Add persistent `Connected device / Source firmware` and `Selected install target` rows. Show canonical observed image/profile/CODE, the beta.4/4.1 alias and separately labelled receipt provenance if available; never infer package release from target. Unknown source blocks install. Beta.5 default follows stage 8; beta.3 is an explicit alternate only for supported pairs. | Shared beta.4/4.1 bytes produce the same observed identity and alias; receipt changes provenance only; beta.5 target does not change source; supported beta.3 alternate; unknown denial. |
| Restore first step and safety | Backup capture/inspection exists; restore must remain blocked by release policy. | Put `1. Open an existing backup → 2. Inspect source and compatibility → 3. Review restore prerequisites` above backup actions. For beta.5 show `Restore unavailable: saved-settings writes are not authorized for this release`; inspection remains available. Never add a live restore shortcut. | Existing/corrupt backup, no device, incompatible source and blocked beta.5 restore; no writer/loader call on any inspection path. |
| Prerequisites, consequences, Back/Cancel and specific blocked reasons | Busy/confirmation flows exist; stage 2 repairs domain causes. | Reuse stage 2 reasons and preflight summary; expose Back before submit and precise post-submit stop semantics, never imply rollback. | Ready/busy/lease/disconnected/unverified states, cancel before/after submit, Tab/Shift-Tab and repaint. |
| Duplicate Application settings | Native duplicate removal is already implemented: `src/ui/ak820_status.rs` provides the persistent service-navigation AppSettings button. Settings-dropdown entry and sizing label in `src/ui/layout_settings_dropdown.rs` are Wasm-only; a dormant match arm is not a rendered menu item. | Preserve native service navigation and Wasm-only dropdown behavior; do not remove either or invent an application-menu entry. No native duplicate-removal code is needed. | Render native EN/RU navigation and assert one intended AppSettings route opens the same persistent page; do not count contextual explanatory links as menu duplicates. |
| Unclear Russian labels | `app_settings.rs` uses `ui.middle_click_transparent_label`/tooltip and app import/export keys; no literal legacy wording remains here, so preserve existing localized improvements. | Set RU label `Средняя кнопка мыши: назначать прозрачную клавишу`; tooltip says this changes editor assignment to `KC_TRNS`, not keyboard transparency/LED. Use `Импорт настроек приложения` / `Экспорт настроек приложения`; explain local app preferences, not AK820 firmware backup or restore. Mirror clear EN scope; keep formats unchanged. | Render both languages; assert no legacy `Middle click → Transparent` / `Import/Export Entropy` labels and no firmware-write implication; import/export roundtrip remains unchanged. |

1. Implement only the remaining changes in the matrix, with one regression per row; retain existing close/reopen/stale-capture tests.
2. Run `cargo test --bin ak-configurator ui::`, including the affected menu/settings/backup regressions. Exercise open→close→capture→cancel→reread→exit without disconnect loops.
3. Attach row-by-row commit/test evidence. Close #6 only when all rows pass; keep RGB/unlock fixes in separate commits.

## Stage 7 — separate OS detection from LED mapping (#7)

Files: `src/ui/ak820_os_indicator.rs` (settings rows, `status_label`, `mode_label` and tests), `src/ui/app_settings.rs`, EN/RU UI text. This issue is not keyboard geometry, encoder ordering or split layout. No fixing commit has been established; do not attribute it to `936a989`.

1. Preserve the current partial improvements: rows already say `LED for English/Russian layout`, `status_label` distinguishes detected/paused states, and English modes say `LED on/off`. Complete the explanation rather than rewriting the helper.
2. Label the status `Detected OS layout` / `Обнаруженная раскладка ОС`. Label choices `LED behavior when English is detected` / `Поведение LED при английской раскладке` and corresponding Russian-layout text. Add always-visible text: `Detection reports the current OS layout. The choices below define LED behavior, not whether a layout is active.` Mirror in Russian. Define Normal = firmware's ordinary behavior, On = LED lit, Off = LED unlit, Breathing = slow breathing; use `LED включён/выключен` in RU, not bare `Вкл./Выкл.`.
3. Busy/unverified status must say detection is paused and current layout is not refreshed. Keep last observed value explicitly labelled as last observed if shown; never present it as a new detection. Do not change saved EN/RU choices, generation or selected page on pause/resume. Disabled/unavailable/error remain separate states; no automatic reset/retry/write.
4. Add deterministic tests: detected EN with EN=Off and RU=On shows EN detection plus both unchanged mappings without contradiction; detected RU reverses only detection; busy/unverified pause and resume preserve mappings; unavailable/disabled/error do not fabricate detection. Test all four mode explanations in EN/RU, persistent widget IDs and no configuration save or HID write caused by rendering/status updates.
5. Run `cargo test --bin ak-configurator ak820_os_indicator` and `cargo test --bin ak-configurator ui::`. Inspect the exact connected firmware/profile/CODE read-only on Windows: record detected layout, saved mappings and physical LED observation without changing indicator options or issuing writes. If helper activity would write, leave it disabled; mark physical mapping confirmation pending separate authorization rather than claim a pass. Historical beta.4 observation is not beta.5 proof.
6. Close #7 only with wording/state tests plus exact-firmware read-only mapping/LED evidence. Without physical observation report `UI clarification complete; firmware/LED observation pending` and keep the issue open. No 65/75/100 or encoder test substitutes for these criteria.

## Stage 8 — beta.5 installer admission

Dependencies: stages 0–2. Files: `src/ak820_release.rs`, `src/ui/ak820_install.rs`, `src/ak820_install_executor.rs`, installer tests, install/help EN/RU.

1. Introduce `InstallerRelease`; default to beta.5 with explicit prerelease opt-in and existing confirmation token plus fresh three-second Esc+left Control. No substitute confirmation.
2. Generalize `ReleaseAdmission` and verify final catalog/matrix/capability/profile IHX/CODE and directed pair. Unknown identity denies.
3. Require verified source CODE for beta.4/4.1 and beta.5 reinstall; deny beta.3 before erase. Never infer CODE from catalog fingerprint.
4. Prepare payloads and exact confirmation contents before erase; flash once, verify once, terminate bounded; no blind retry/reset/restore.
5. Test all 200 allow pairs and representative denies: beta.3/unknown→beta.5, beta.5→beta.4.1, digest mismatch, prerelease off and catalog substitution. No physical flash in CI.
6. Run `cargo test --bin ak-configurator ak820_release` and `cargo test --bin ak-configurator ak820_install`.

Acceptance: offline admission is complete; physical flashing remains separately authorized.

## Stage 9 — regression, docs and release gate

1. Update README/install/status/requirements/help EN/RU: beta.5 default, previous beta.4/4.1 compatibility, read-only QS23, no live new-module writes and all hardware limitations. Remove claims that staged beta.5 assets are current.
2. Run once after all fixes: `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test --bin ak-configurator` (includes native/headless UI tests); `python3 .github/scripts/test_beta5_catalog.py`. Check all changed Markdown links with the repository documentation checker if one is introduced; currently verify them explicitly in review.
3. Record test counts/environment and skipped Windows/macOS/physical checks.
4. Map #2–#7 to commits/tests. #3 remains open without Windows hardware. Never label skipped checks as pass.
5. No companion release until independent review closes findings. Platform packages/SHA256 are a separate release task.

## Commit order

1. `ak820: pin final beta.5 release inputs`
2. `ak820: add beta.5 QS v3 offline contracts`
3. `ak820: recognize beta.5 and repair backup availability`
4. `ak820: move RGB writes to bounded HID worker`
5. `ak820: make unlock polling finite`
6. `ak820: preserve service pages during indicator refresh`
7. `ui: finish native service-page integration`
8. `ui: clarify OS detection and LED mapping states`
9. `ak820: admit exact beta.5 installer transitions`
10. `docs: record beta.5 verification and limitations`

Each commit needs independent review and a recorded audit. Substantive corrections require retesting and fresh review. Do not push without user authorization.

## Stop conditions

Do not select replacement firmware inputs, enable writes/restore/reset/flash from catalog flags, start browser/Wasm work, copy OEM data, broaden this into a generic HID framework or close hardware issues using offline evidence. If final release assets are unavailable, stop at stage 0 and request them.
