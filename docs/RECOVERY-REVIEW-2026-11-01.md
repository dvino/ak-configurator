# Recovery and issue review — 2026-11-01

## Provenance and completeness

Source session: `LOCAL_SOURCE/.pi/agent/sessions/SOURCE_SESSION/2026-10-29T07-22-15-030Z_01a18689-b876-70bc-a0e3-d30e4994a65f.jsonl` on the original host (identity redacted). Source worktree: `LOCAL_SOURCE/ChatGPT/ak-configurator-issue6-20261028`, detached at `936a989a8fd7d8e24cbd70ddba0e768e3107494b`. The remote worktree was not modified.

Recovered every reported dirty artifact into `LOCAL_WORK/ak-configurator-beta5-review`, created from `origin/desktop-dev` at the same commit:

- modified `docs/STATUS.md` and `docs/ru/STATUS.md`;
- new `.github/scripts/test_beta5_catalog.py`;
- new `assets/ak820/beta5-release-catalog.json`, `beta5-capability-catalog.json`, `beta5-firmware-matrix.json`, `beta5-module-descriptions.json`.

No other source files were reported or copied. Original JSONL and prior handoffs are retained in `LOCAL_TMP/ak-configurator-review-mvxnftok/`; the worktree and temporary evidence remain present. The recovered Python test passes (`Ran 1 test ... OK`). It validates pinned asset hashes, ten profiles, policy flags and cross-document identities, not packed-byte reconstruction or transition pairs.

## Recovered-work findings

- The staged beta.5 files describe the superseded published release at `78dc69e6`, not replacement source `c9e0220c25f664c4a470faa6700059f6aedf520c`. They remain review inputs, not production pins.
- Moving the test into `.github/scripts` closed the earlier CI-discovery issue. Its checks are still self-consistency checks until stage 0 verifies final bundle/IHX/CODE bytes.
- Prior evidence reported 1969 tests before the final fix plus eight targeted tests, no physical-device or complete-ROM proof and no platform GUI proof. Those historical counts were not rerun here.
- Exact packed CODE differs from linked CODE. Release transitions do not authorize settings restore. `settings_writes_authorized=false` remains binding.

## Issue-to-code audit

Rechecked the full GitHub issue inventory on 2026-11-01: six issues, #2–#7, all open; no closed issues. Exact bodies, URLs and comments were fetched with `gh issue view --json number,title,body,comments,url` and retained in `LOCAL_TMP/ak-configurator-review-mvxnftok/issues-final-review/issue-{2..7}.json`. Issue #2 has the owner comment separating read failure from blocked/untested restore; the other issues have no comments. The matrix and execution stages use those original acceptance criteria, not inferred meanings of their titles.

| Issue | Current evidence | Required work and closure |
|---|---|---|
| #2 disconnected/lease admission | `src/ui/ak820_backup_inspection.rs` reduces capture errors to `Problem::Capture`; `src/ui/ak820_status.rs` includes native-operation busy; `src/ui/ak820_connection.rs` owns reconciliation. | Preserve domain causes and distinguish `OperationBusy`, `LeaseBusy`, `Disconnected`; beta.5 stage 2 tests all three plus stale generation. |
| #3 Windows RGB hang | Four setters in `src/ui/rgb_settings.rs` call synchronous `hid.set_vialrgb_mode`; `src/ui/vial_hid_task.rs` is the existing worker. | One submitted command plus one unsent coalesced draft, bounded result/readback and no retry. Offline closure does not close Windows hardware acceptance. |
| #4 unlock never terminates | `src/vial/unlock.rs` `fail_vial_unlock_poll` logs retrying and continues polling. | Add 15-second virtual-clock timeout, terminal error/cancel states and no automatic restart. |
| #5 layout flicker | Actual seam is `Job::Sample`/`Job::Write`, completion polling and cached HID-loan display facts in `src/ui/ak820_os_indicator.rs`. Existing unchanged-sample, competing-work and cached-page tests provide partial protection; the reported physical cause is not established. | Stage 5 preserves these protections and adds 100 steady/100 alternating EN/RU multi-frame tests for version/page/dialog/capture/connection stability. Windows read-only timing evidence is required before issue closure. |
| #6 native navigation | Native service pages and AppSettings service-navigation entry exist in `src/ui/ak820_status.rs`; `src/ui/layout_settings_dropdown.rs` AppSettings entry and sizing label are Wasm-only, so native duplicate removal is already implemented. Byte-identical beta.4/4.1 cannot be distinguished by CODE; restore needs a first-step explanation. `src/ui/app_settings.rs` uses localized label keys. | Stage 6 preserves navigation/Wasm behavior, tests one intended native settings route, separates observed source identity/alias/provenance from target, explains inspection/blocked restore and RU editor/import/export scope. No replacement architecture. |
| #7 OS detection versus LED mapping | The actual issue is EN detection shown alongside EN=Off/RU=On mappings, not keyboard geometry. `src/ui/ak820_os_indicator.rs` already has partially clarified mapping labels and separate paused/detected status. Fixing history was not established. | Stage 7 explicitly labels detection versus saved LED behavior, explains every mode in EN/RU and preserves choices on pause. Test EN=Off/RU=On, detection changes and busy/unavailable states; exact-firmware read-only mapping/LED observation remains required. Geometry/encoder tests are unrelated. |

## Independent review and verification

The review below covered an earlier revision and missed substantive issue-acceptance gaps. A subsequent completion audit correctly rejected #7's wrong scope and incomplete #5/#6 coverage. Those sections were rewritten against the original GitHub issue bodies; subsequent review and closure are recorded below. SOL review `bfed151f-5431-4b47-8b09-b6b9d8014c13` returned BLOCK on six concrete planning defects: invalid test commands, incorrect MCP API/CLI, already-absent native duplicate, beta.4/4.1 aliases, inconsistent R/L admission and this report's obsolete #5 attribution. The revised documents correct those defects; the final independent closure is recorded below. See [SESSION-EXECUTION-PLAN.md](SESSION-EXECUTION-PLAN.md) for one-session-per-stage handoff contracts.

Earlier focused independent review: `55d7f33b-27bf-4585-a7e6-ccdae0862f72`, 2026-11-01. Verdict: **PASS — no substantive remaining blockers** in the EN/RU recovery report and beta.5/MCP execution plans. It confirmed issue-to-source mappings, removal of unsupported #7 attribution, real stage 0 test commands, matching artifact-store contracts and safety gates. Review did not run builds, device/remote operations or inspect committed history; commit scope was separately checked by the main agent with `git show 936a989`.

Local verification: `python3 .github/scripts/test_beta5_catalog.py` passed one test; `git diff --check` and the local Markdown-link check passed. No Rust implementation or platform/device acceptance is claimed. No commit or push was performed. Temporary recovery evidence and the review worktree are intentionally retained for handoff.

## Final planning verification

- Broad SOL review `d06f4fc8-009d-4998-8e9e-a19d592a679a` confirmed prior corrections except a remaining MCP action mismatch, and found unassigned SDK/Inspector interoperability, a nonexistent unlock path and a checkout-escaping RU command. All four were corrected in EN/RU.
- Fresh focused SOL review `932c1b8c-78d8-4a68-959a-756830d93042` returned **PASS**, with file/line closure for all four. It reuses the broad review's source/architecture conclusions; it is not a new full code/hardware audit. Reports are retained in the parent session's `subagent-artifacts` directory under those run IDs.
- Final local checks: `git diff --check` passed; project-doc scan resolved 210 links in 40 files; 26 ordered session cards match EN/RU; literal source paths/whitespace/invalid-command exclusions passed. Reproducible checker and evidence remain under `LOCAL_TMP/ak-configurator-review-mvxnftok/`.
- `python3 .github/scripts/test_beta5_catalog.py` passed 1 test: superseded recovered catalog self-consistency, not final firmware admission or packing. Application/firmware builds and physical tests were not run for this planning task.
- GitHub recheck on 2026-11-01 still lists only open #2–#7, with unchanged update timestamps. No issue was closed. Final pins, platform qualification, SDK/Inspector execution and separately authorized physical observations are future session gates, not planning-completion blockers.
- Recovery worktree and temporary evidence directory are retained because they contain required uncommitted results/provenance. No commit/push, application/firmware implementation or device operation was performed. Future execution starts only on a separately authorized card.

## Firmware beta.5 alignment

Candidate `c9e0220c25f664c4a470faa6700059f6aedf520c` failed runs 41294887078/41294883546. Latest observed candidate on 2026-11-01 is `2ab372dab7f084b76b57aa98a9543c249a6d0c30`, runs 41297661542/41297648433 in progress. `gh release view beta.5 -R dvino/ak820` returned `release not found`. Earlier run 41291741328 failed static settings contract assembly. All are historical observations to recheck at stage 0, not final pins; no final CI success or hashes are claimed.

QS record v3 adds QSID23 at offset 20, little-endian u16, 1–1000 ms, image-bound default normally 300. Read v1/v2/v3 and model lazy migration offline; do not enable saved-settings writes. Allowed installer transitions are only beta.4/4.1→beta.5 and beta.5→beta.5. Final release assets and pack verification are mandatory gates in the execution plan.
