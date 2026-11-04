# AK Configurator: beta.5 execution plan

Current state (2026-11-04): B01 stage 0 and the limited offline scopes through B11 have been accepted as recorded in [SESSION-EXECUTION-PLAN.md](SESSION-EXECUTION-PLAN.md). B12 integration and delivery remain in progress; the alpha is not accepted. The implementation stages below record the earlier execution plan and are not current assignments.
Planning snapshot: 2026-11-03. Original baseline: `936a989a8fd7d8e24cbd70ddba0e768e3107494b` plus the recovered uncommitted artifacts; this is not the current checkout state.

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
3. Add `RecordVersion::V3`, `ImageVersion::V3`, QSID 23 and a tenth field: header `51 53 03 11 ff`, record length 24, offset 20, little-endian u16, range 1–1000. CRC-16/CCITT covers bytes 0–21 and is stored little-endian at 22–23. Firmware C blank defaults set QSID 22 at byte 17 to 1; the official Python v3 default vector uses 0. V1 upgrade preserves its byte-17 value (zero in v1); v2 migration preserves the existing value. The image marker at bytes 20–21 is 300 for all ten profiles. Keep the differing defaults explicit in independent vectors; do not silently equate them. **Implementation update:** V1/V2 model defaults retain the fallible `defaults(version)` API; the legacy wrapper delegates to the common marker API. V3 requires the explicit image-format marker through `decode_for_formats`; missing V3 marker/default returns `Err`. Do not route the V3 marker through a generic timeout-default parameter. Preserve V3 acceptance of v1/v2/v3, reject V2 parsing of V3, append the selected image marker during v1/v2→v3 normalization, recalculate CRC, and reject downgrade. This is offline conversion only. See the [B02 contract audit](session-evidence/B02-CONTRACT-AUDIT.json).
4. Read QSID 23 through `get_qmk_setting_u16`, never the u8 list `1 | 3 | 22`.
5. Reuse `VialHidOperation`/`HidHelper`; no second transport. Page availability is `Ready | OperationBusy | LeaseBusy | Disconnected | Failed(reason)`. Keep generation/request IDs on every result.
6. Beta.5 initially supports exact read-only reconciliation and backup. Do not expose live writes for new Tap Dance, NKRO or Dynamic Macros. New keycodes remain offline-editor only under the exact keycode source. QS23 editing is a stated later limitation.

Execution is partitioned by [SESSION-EXECUTION-PLAN.md](SESSION-EXECUTION-PLAN.md): B00–B12 are bounded implementation sessions, C cards share their domain work, Q/H separate offline platform and admitted physical evidence. Do not implement an entire broad stage in one session when it has multiple cards.

## Stage 0 — freeze final beta.5 inputs

**Accepted 2026-11-03.** B01 stage 0 is accepted following `b01_pins_review` and `b01_reconstruction_review`. The official release is [v0.6.0-beta.5](https://github.com/dvino/ak820/releases/tag/v0.6.0-beta.5), tag object `1d960eabb5fd9abf078e6815b0c5c01bfb13a803` resolving to commit `dfd0770f9ef64bf5c7ab6d40e9f9e7b4354cd2cd`. Successful publication/firmware CI runs: [41351933676](https://github.com/dvino/ak820/actions/runs/41351933676) and [41349097405](https://github.com/dvino/ak820/actions/runs/41349097405).

The four catalog assets and bundle were pinned to the release: [capability-catalog.json](https://github.com/dvino/ak820/releases/download/v0.6.0-beta.5/capability-catalog.json), [firmware-matrix.json](https://github.com/dvino/ak820/releases/download/v0.6.0-beta.5/firmware-matrix.json), [module-descriptions.json](https://github.com/dvino/ak820/releases/download/v0.6.0-beta.5/module-descriptions.json), [release-catalog.json](https://github.com/dvino/ak820/releases/download/v0.6.0-beta.5/release-catalog.json), and [firmware-bundle.zip](https://github.com/dvino/ak820/releases/download/v0.6.0-beta.5/firmware-bundle.zip). The catalog test passed in 0.099 seconds and confirmed all 200 allowed directed pairs; the published write-authority flag remains false. The Windows reconstruction report records actual 10/10 profile pack results and per-profile IHX/map/CODE/permanent hashes: [JSON evidence](session-evidence/B01-WINDOWS-RECONSTRUCTION.json). B01's accepted handoff is [B01](session-evidence/B01.md).

The reconstruction review rehashed the three private input metadata records and ten public IHX/map pairs, checked the official packer blob and CODE interval, and inspected the actual-output-hash script. Raw packer outputs were removed after metadata capture; the review did not rerun the packer, and historical private-input acquisition provenance was not independently established. The pins review independently reran CAT and checked GitHub API asset sizes/hashes and the tag/commit mapping. A separate release-bundle download attempt ended with `IncompleteRead`; the bundle identity is recorded in the reconstruction report. This acceptance covers B01 stage 0 only; it proves no physical behavior, installed image identity, or runtime compatibility.

**Next:** B03 beta.5 profile integration. B02 offline implementation acceptance and its limits are recorded in [B02 evidence](session-evidence/B02.md). The authoritative wire contract remains [B02-CONTRACT-AUDIT.json](session-evidence/B02-CONTRACT-AUDIT.json).

## Stage 1 — offline QS v3 and beta.5 profiles

**Current state (2026-11-03):** B02 offline implementation accepted by independent reviewer `b02_code_review`, based on static review and reported test/lint results without reruns. The focused Windows offline groups passed: codec 8, settings model 16, NVM migration 7, versioned backup 6 and restore plan 13; final settings-model regression 16/16 and invalid-QSID-23 regression 8/8 also passed. Strict all-target Clippy and `cargo fmt --check` passed. This acceptance does not qualify firmware runtime support, image admission, an installed image or device behavior. See [B02 evidence](session-evidence/B02.md); the [contract audit](session-evidence/B02-CONTRACT-AUDIT.json) records authoritative wire details.

Files: `src/ak820_qs_versioned_offline.rs`, `src/ak820_settings_model.rs`, `src/ak820_typed_storage.rs`, `src/ak820_restore_plan.rs`, `src/ak820_settings_backup.rs`, `src/ak820.rs`, `src/ak820_beta1_profiles.rs`, offline tests.

1. Implement the QS and profile decisions above without enabling a writer.
2. Add independent firmware-C and official Python golden v3 vectors. Preserve the documented difference in blank QSID22 default (C=1, Python=0), verify v1 migration keeps byte 17 at zero, verify v2 preserves it, and use image marker 300 for all ten profiles. Test Leader 1/300/1000 and reject 0/1001/65535.
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

Files: `src/ui/rgb_settings.rs`, `src/ui/vial_hid_task.rs`, `src/ui/app_state.rs`; the narrowly scoped cancellation hook in existing `src/hid.rs` / `src/hid_ak820.rs` transport and guard path; transaction and RGB tests. Reuse the existing guard and readback path; do not create a second engine.

1. Remove synchronous `hid.set_vialrgb_mode` calls from all four RGB setters. Add one `VialHidOperation::Ak820RgbWrite` carrying a full effect/speed/H/S/V snapshot, request ID, generation and old value.
2. Validate exact beta.5 identity, writable authorization and ranges before submission. Keep exactly one submitted command and at most one unsent coalesced draft: a new UI change replaces only that unsent draft; after it is submitted, a newer change may become the single new draft. Never grow an unbounded queue or replay/retry a sent command.
3. Reuse the existing FD01 guard before and after the operation and existing `ak820_verify_write` SET/GET, SAVE/GET and NVM readback behavior. Do not add a duplicate GET or second engine. Keep one SET and at most one SAVE per logical submission; preserve existing required verification/status packets.
4. The UI worker may cancel before method dispatch, but worker exit cannot stop internal packets after dispatch. Add a narrow cancellation hook at boundaries inside the existing transport/guard. If cancel/exit occurs before a packet boundary, send no later packet and return a fail-stop uncertain result. Do not weaken the authorization guard or beta.5 write denial.
5. Disable RGB controls while submitted but keep window repaint running. Stop the scene before queueing. Exit remains nonblocking with bounded wait for in-flight work and issues no later packet or queued draft. Generic non-AK820 and Wasm behavior remains unchanged.
6. Fake delayed-HID tests: five-second delay does not stop frames; rapid changes yield at most one submitted command plus one unsent draft; cancellation before and between internal packet boundaries prevents later packets; timeout has no retry; stale generation is ignored.
7. Run `cargo test --bin ak-configurator rgb`, `cargo test --bin ak-configurator vial_hid_task` and `cargo test --bin ak-configurator ui::`.

8. **Acceptance:** offline tests prove no sync wait/retry, fail-stop cancellation at existing packet boundaries and safe Quit/window close during delayed/failed worker completion, with no later packet or draft on exit. For #3 record chosen effect, exact image/candidate, active owner, request/result or timeout and close behavior without private payloads. Keep #3 open until Windows physical evidence exists; every live RGB action needs fresh write authorization. No implementation or test results are claimed by this plan update.

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

**B09 state (2026-11-03):** The limited offline scope was accepted by independent reviewer `b09_matrix_review` after source/EN-RU evidence review, 96-file inventory verification and independent aggregate result checking. Reported tests/lint were not rerun. See [B09 evidence](session-evidence/B09.md). The checks below record offline UI/model and source evidence. Native OS window behavior, hardware behavior, a Wasm build and physical issue closure are not established. Beta5 default/prerelease admission remains a B11 dependency; saved-settings restore remains blocked.

| Issue requirement | Current implementation / gap | Exact remaining change | Verification |
|---|---|---|---|
| Persistent firmware/install, backup/restore and app settings pages | Native service routes persist in the main shell. | No navigation redesign or top-level-window replacement was made. | `service_buttons_open_persistent_pages_in_both_languages`; full-shell reopen tests for installer with selected target, backup inspection state and AppSettings in the root viewport. Headless UI/model evidence only; no native window or keyboard-focus proof. |
| Selected device and verified source/target | With no or invalid candidate, Device prompts for selection and CODE is unknown. If the selected candidate differs from the editor candidate, Device shows the selected candidate name and CODE remains unknown. If path and token match, Device shows the selected candidate name; CODE is shown only when a verified observation is available. Beta4 alias remains an observation; target changes do not rewrite it. Receipt provenance was unavailable and is not inferred. | Dated B09 checkpoint (2026-11-03): the rendering fixtures reflected the beta.3/beta.4.1 target state at that checkpoint. Current installer defaults to beta.5 with explicit prerelease opt-in; beta.3 remains catalog-denied. See [current B11 evidence](session-evidence/B11.md). | `candidate_observation_requires_matching_path_and_instance`, `candidate_selection_renders_only_matching_observation`, `beta4_alias_render_stays_observed_when_target_changes`, `installer_distinguishes_unverified_source_from_selected_target`. No full-ROM, receipt or preflight SHA proof from rendering fixtures. |
| Restore first step and safety | Inspection remains available; settings restore remains blocked. The new first-step instructions are bounded to the inspection entry. | Keep inspection and the explicit blocked-restore state; do not add a writer or loader route. | Backup extended-viewport render and strict manifest checks, with existing corrupt/stale cases; no restore execution or write path. |
| Prerequisites, consequences, Back/Cancel and specific blocked reasons | Existing prerequisite and stop models remain in place. Cancellation is available only before erase; submitted/executing operations do not imply rollback. | Preserve current safety behavior and precise stop states. | `executing_cancel_is_available_only_before_erase` plus executor safety-failure/stop-model tests. No physical operation. |
| Duplicate Application settings | `ak820_status.rs` provides the persistent native service route. The dropdown entry and sizing label remain Wasm-only. | Preserve the single native route and existing Wasm-only behavior. No Wasm build was run. | Native EN/RU full-shell navigation tests open and reopen the same AppSettings page; source inspection confirms the Wasm configuration guard. |
| Unclear Russian labels | AppSettings labels, explanations and editor behavior are checked through the actual rendered page and hover text in both languages. | Keep app-preference import/export distinct from firmware backup/restore and explain `KC_TRNS` as editor assignment. Formats are unchanged. | AppSettings render/hover tests, bounded backup instruction/action render, and existing middle-click `KC_TRNS` plus import persistence tests. |

1. The limited offline B09 scope was accepted by `b09_matrix_review`; its review did not rerun reported tests/lint. Physical issue closure remains pending H01.
2. Reported Windows checks: `cargo test --locked --bin ak-configurator ui:: -- --test-threads=1` (219 passed, 2 ignored, 1754 filtered); installer tests (49 passed) and backup tests (39 passed). The final source state was compiled and focused executable checks passed, but these are not native-window or hardware tests.
3. Strict LINT-N0 and whole-tree formatting passed after final test-only cleanup; the 96-file source inventory is recorded in B09 evidence. Keep physical issue #6 open pending H01; issue #5 is not closed by this UI evidence. See the evidence report for row-by-row limits and hashes.

## Stage 7 — separate OS detection from LED mapping (#7)

**B10 status (2026-11-03):** Limited offline scope was accepted by `b10_semantic_review` after source/EN-RU review, verification of all 97 listed hashes and the aggregate inventory result; reported tests/lint were not rerun. The common resolver returns a choice only for enabled, recognized, mapped layouts; `None` pauses scheduling and prevents a new transition. Explicit `Normal` and disabled `Some(Normal)` retain prior behavior, and imported unknown mappings are retained. Rendering leaves unset mappings unset; selection saves an application preference without an immediate HID command. Final `N(ak820_os_indicator)` passed 26 tests (1954 filtered, 9.12 s); `ui::` passed 219 with 2 ignored on unchanged production code before the test-only helper API update. Strict LINT-N0 and whole-tree `cargo fmt --check` passed. See [B10 evidence](session-evidence/B10.md) and [B10 source inventory](session-evidence/B10-SOURCE.json). B10 acceptance does not close issue #7; exact beta.5 physical LED observation remains an H01 condition. Later working-tree changes are outside the accepted checkpoint.

Files: `src/ui/ak820_os_indicator.rs` (settings rows, `status_label`, `mode_label` and tests), `src/ui/app_settings.rs`, EN/RU UI text. This issue is not keyboard geometry, encoder ordering or split layout. No fixing commit has been established; do not attribute it to `936a989`.

1. Preserve the current partial improvements: rows already say `LED for English/Russian layout`, `status_label` distinguishes detected/paused states, and English modes say `LED on/off`. Complete the explanation rather than rewriting the helper.
2. Label the status `Detected OS layout` / `Обнаруженная раскладка ОС`. Label choices `LED behavior when English is detected` / `Поведение LED при английской раскладке` and corresponding Russian-layout text. Add always-visible text: `Detection reports the current OS layout. The choices below define LED behavior, not whether a layout is active.` Mirror in Russian. Define Normal = firmware's ordinary behavior, On = LED lit, Off = LED unlit, Breathing = slow breathing; use `LED включён/выключен` in RU, not bare `Вкл./Выкл.`.
3. Busy/unverified status must say detection is paused and current layout is not refreshed. Keep last observed value explicitly labelled as last observed if shown; never present it as a new detection. Do not change saved EN/RU choices, generation or selected page on pause/resume. Disabled/unavailable/error remain separate states; no automatic reset/retry/write.
4. Add deterministic tests: detected EN with EN=Off and RU=On shows EN detection plus both unchanged mappings without contradiction; detected RU reverses only detection; busy/unverified pause and resume preserve mappings; unavailable/disabled/error do not fabricate detection. Test all four mode explanations in EN/RU, persistent widget IDs and no configuration save or HID write caused by rendering/status updates.
5. Run `cargo test --bin ak-configurator ak820_os_indicator` and `cargo test --bin ak-configurator ui::`. Inspect the exact connected firmware/profile/CODE read-only on Windows: record detected layout, saved mappings and physical LED observation without changing indicator options or issuing writes. If helper activity would write, leave it disabled; mark physical mapping confirmation pending separate authorization rather than claim a pass. Historical beta.4 observation is not beta.5 proof.
6. Close #7 only with wording/state tests plus exact-firmware read-only mapping/LED evidence. Without physical observation report `UI clarification complete; firmware/LED observation pending` and keep the issue open. No 65/75/100 or encoder test substitutes for these criteria.

## Stage 8 — beta.5 installer admission

Dependencies: stages 0–2. Files: `src/ak820_release.rs`, `src/ui/ak820_install.rs`, `src/ak820_install_executor.rs`, installer tests, install/help EN/RU.

**Current state (2026-11-03):** The limited offline B11 scope was accepted by independent reviewer `b11_independent_review`; see [B11 evidence](session-evidence/B11.md). Beta5 defaults to explicit prerelease opt-in; the exact 200 canonical pair set and pin/denial/preservation behavior are recorded there. No device operation or physical flash occurred. This acceptance does not qualify an installed image, source-to-binary linkage, release readiness or physical behavior.

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
