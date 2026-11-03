# Execution by independent sessions

Status: future execution contracts, not implementation or device authorization. Date: 2026-11-01.
Russian: [SESSION-EXECUTION-PLAN.md](ru/SESSION-EXECUTION-PLAN.md).

This is the ordering and handoff layer for [BETA5-EXECUTION-PLAN.md](BETA5-EXECUTION-PLAN.md) (B) and [MCP-COMPLETION-PLAN.md](MCP-COMPLETION-PLAN.md) (M). Those documents supply the detailed algorithms, policies and issue acceptance. Session cards partition their work; they do not replace or relax it. [RECOVERY-REVIEW-2026-11-01.md](RECOVERY-REVIEW-2026-11-01.md) records the baseline and review corrections. Earlier broad stages/commit suggestions are work groups, **not** instructions to implement an entire group in one session.

## Rules for a fresh session

One session executes exactly one card. Start only when the operator authorizes that implementation/qualification session. This planning task does not start any card. Read this common contract and the chosen card as the ready-to-use starting assignment; no extra design decision is delegated to the executor.

1. Read project AGENTS, requirements, PLAN, STATUS, recovery report, the card's B/M sections and every prerequisite handoff. Record the current code revision **and** reviewed dirty patch digest. The original `936a989a8fd7d8e24cbd70ddba0e768e3107494b` is provenance, not a command to discard subsequent work.
2. Use one owned native worktree based on reviewed `desktop-dev` work. Verify branch/ownership under AGENT-ROUTING before editing; preserve concurrent and uncommitted work. All writer cards run sequentially in that worktree. Only read-only investigation/review may overlap. Never copy the firmware tree or OEM data into the companion repository.
3. Set a unique absolute `CARGO_TARGET_DIR` for the session, outside other active build directories, and record it. Retain useful outputs for following sessions; never perform broad cleanup. Commands below run from this worktree. Prefix noisy safe local test/build/lint commands with RTK if available, preserving real exit status. Do not wrap remote, mutating or secret-handling operations.
4. Implement only the card's named delta. Use B/M's already selected architecture. Add tests in existing binary inline modules and `scripts/test_mcp_stdio.py`; do not invent a library or integration target. Filtered tests must list/run nonzero intended tests. New test names mentioned below are explicit deliverables, not claims that they already exist.
5. Maintain matching EN/RU behavior/limits and the card's metadata-only evidence. Default tests are offline and never start inherited UI against a device. Any unexpected device access/write, timeout, corrupt storage, bad readback or ownership loss stops the operation; no retry/reset/restore.
6. Before commit/push obtain independent review. This plan does not itself authorize commit/push. Fix substantive findings, rerun affected checks and obtain fresh review. An uncommitted reviewed patch with its digest is a valid handoff; never silently commit to satisfy a stage label.
7. End at the card boundary. Do not begin the next card to use remaining context. If a prerequisite is missing, produce a blocked handoff. If a card reveals an unplanned architectural/policy choice, stop and ask; do not grow the card or redesign in flight. Do not claim that every platform run or external wait fits a fixed duration.

### Fixed check recipes

`N(filter)` means `cargo test --locked --bin ak-configurator <filter>`. `N(all)` means omit the filter. `C(filter)` means `cargo test --locked --features mcp-mock --bin ak-configurator <filter> -- --test-threads=1`. `C(all)` omits its filter. These are notation for the exact commands, not new scripts.

`STDIO`: rebuild with `cargo build --locked --features mcp-mock`, then `python3 scripts/test_mcp_stdio.py --binary "$CARGO_TARGET_DIR/debug/ak-configurator"` (Windows `.exe`). For production-feature mock denial, build feature `mcp` without `mcp-mock` in a second isolated target directory and pass that binary as `--production-binary`; never discover real devices. Preserve both supported protocol generations in the existing script.

`LINT-N`: `cargo fmt --check` and `cargo clippy --locked --all-targets -- -D warnings`. `LINT-C`: same fmt plus `cargo clippy --locked --all-targets --features mcp-mock -- -D warnings`. `CAT`: `python3 .github/scripts/test_beta5_catalog.py`. New profile/prepare/execute filters must be added by their cards before use.

### Mandatory handoff

Create `docs/session-evidence/<ID>.md` and matching `docs/ru/session-evidence/<ID>.md` when future implementation begins (these are new planned reports, not existing test targets). Include:

- card ID, date, worktree/branch, base revision, final revision or dirty-patch digest, file inventory;
- prerequisite report IDs and exact input/catalog/firmware hashes; observed identity versus package provenance;
- changed behavior and explicitly unchanged safety boundaries;
- exact commands, exit codes, test names/counts, tool/platform versions, independent findings and their closure;
- PASS/FAIL/PENDING separately for source/offline/protocol/platform/physical evidence;
- temporary resources, owner, purpose, in-use status, retained outputs and private-input locations **without contents or secrets**;
- one terminal result: `accepted`, `blocked` or `needs-correction`; next card ID and the precise unblock condition.

Do not put OEM bytes, settings/macros, private paths containing secrets, raw device logs or full-ROM artifacts in reports. A missing prerequisite blocks its dependents, not unrelated cards. Reports may exist with pending external acceptance but must not label their card accepted if its own required checks are absent.

## Order and dependencies

Default native sequence: **B00 → B01 → B02 → B03 → B04 → B05 → B06 → B07 → B08 → B09 → B10 → B11 → B12**.

MCP sequence: **C00 → C01 → C02 → C03 → C04**; dependencies on B cards below are mandatory. C00 can be scheduled immediately after B00 while final firmware is unavailable, but not concurrently with another writer. C02 can follow C00 before C01 if no final firmware exists; C03 still needs both. Do not duplicate QS/profile/backup engines in C cards. **C05 is separately gated optional mock work**, not a prerequisite to accepted read-only MCP.

Offline qualification: **Q01 → Q02 → Q03 → Q04** after B12/C04; platform unavailability produces pending evidence. Live cards **H01/H02/H03** are separate optional qualifications with their own admission; no Q card silently performs them. Q04 records pending live issue closure rather than inventing a pass. This is a dependency graph, not permission for parallel writers.

## Native implementation cards

### B00 — establish the execution baseline

**Start assignment:** Preserve and verify the recovered review work; do not implement a feature. Inputs: accepted planning documents, recovery report and original issue snapshots. Files: PLAN/STATUS EN/RU and B00 reports only. Read `docs/AGENT-ROUTING.md`, inspect branch/status and enumerate recovered/catalog changes. Bind every next report to the reviewed baseline/dirty-patch digest; retain original JSONL and unique material outside the repository. Inspect `cargo metadata --offline --no-deps --format-version 1` for binary/features and resolve the owned worktree/output directory. Record local final-firmware availability without installing anything. Check local links and `git diff --check`. **Exit:** complete ownership/provenance inventory and exact baseline; next B01 or C00. **Stop:** unowned work, missing unique source or uncertain baseline.

### B01 — pin and reconstruct final beta.5 inputs

**Start assignment:** Execute B stage 0 only. Needs B00 plus final published firmware, successful release CI and separately validated private local OEM/native files. Files: `assets/ak820/beta5-*`, `.github/scripts/test_beta5_catalog.py`, B01 reports. Download exact final public inputs; record tag/commit/run, sizes and digests, ten profiles and 200 allowed directed pairs. Run the **actual six-input packer command in B stage 0**, fresh private output per profile; compare IHX/map digests separately from reconstructed permanent packed CODE. Keep all full-ROM/recovery outputs private; require 10/10. Extend the catalog test with external pins and pair checks; run CAT. **Exit:** immutable final-input inventory, 10/10 metadata proof and passing CAT; next B02. **Stop:** unpublished/failed candidate, missing private input, changed CLI, wrong count/hash or reconstruction mismatch. Do not fall back to staged files.

### B02 — QS v3 codec and offline migration

**Start assignment:** Implement only B stage 1 QS/model part. Needs B01. Files: `src/ak820_qs_versioned_offline.rs`, `src/ak820_settings_model.rs`, `src/ak820_typed_storage.rs`, `src/ak820_restore_plan.rs`, inline tests. Add V3, the exact header, QSID23 u16 offset20/range1–1000 and image-supplied default, with CRC/forward-only offline migration. Add firmware-derived golden vectors and edge/corruption/property cases specified in B; no HID writer. Run N(ak820_qs_versioned_offline), N(ak820_settings_model), N(ak820_restore_plan), LINT-N. **Exit:** codec/default/migration evidence and unchanged write denial; next B03. **Stop:** golden bytes or default contract disagree with B01.

### B03 — exact beta.5 profile/domain integration

**Start assignment:** Finish B stage 1 profile part. Needs B01/B02. Files: `assets/ak820/beta5-profiles.json` (new), `src/ak820.rs`, `src/ak820_beta1_profiles.rs`, `src/ak820_settings_backup.rs`, affected exhaustive matches/tests. Add `ImageContract::Beta5(ProfileId)` and `beta5_firmware_matrix()` without a generic registry; populate all ten profiles only from B01. Preserve canonical Beta4 identity plus beta.4/4.1 alias, distinct package provenance and target. Add `beta5`-named positive/unknown/ambiguous/capability regressions; run N(beta5), N(ak820_settings_backup), CAT, LINT-N. **Exit:** exact read-only domain admission, ten-profile limits and no new writes; next B04/C01. **Stop:** unpinned field or unsupported identity inference.

### B04 — full read-only reconciliation and backup metadata

**Start assignment:** Implement B stage 2 domain/worker read path, not page redesign. Needs B03. Files: `src/ui/ak820_connection.rs`, `src/ak820_backup_capture.rs`, `src/ak820_settings_backup.rs`, `src/ui/vial_hid_task.rs`. Read QS23 via u16 API, reconcile full NVM/macros/QS, carry source observation/alias, selected target, catalog digests and QS versions; preserve generation, lease and fail-stop. Add v1/v2/v3, mismatched macro/QS, busy/disconnect/stale-generation fixtures. Run N(ak820_backup), N(ak820_connection), N(beta5), LINT-N. **Exit:** verified reusable read snapshot and precise typed failures, no atomic-snapshot or restore claim; next B05. **Stop:** any ambiguous read or implicit writer.

### B05 — snapshot failure and backup inspection UI (#2)

**Start assignment:** Complete B stage 2 UI/issue acceptance offline. Needs B04. Files: `src/ui/ak820_backup_inspection.rs`, `src/ui/ak820_status.rs`, EN/RU catalogs. Preserve original capture errors and fully verified preview; distinguish LeaseBusy/OperationBusy/Disconnected/Failed. Add synthetic `ak820-settings-backup-2` inspection fixtures: compatible preview or exact image/version/policy denial, not generic disconnect. Keep restore/reset blocked. Run N(ak820_backup), N(ui::), LINT-N. **Exit:** offline #2 acceptance, explicit Windows pending flag; next B06. **Stop:** older backup structure cannot be safely interpreted; never reinterpret it as compatible.

### B06 — bounded RGB worker (#3)

**Start assignment:** Execute B stage 3 only. Needs B05. Files: `src/ui/rgb_settings.rs`, `src/ui/vial_hid_task.rs`, affected result handling/tests. Move synchronous effect/brightness/color/speed HID and autosave off egui; exactly one submitted command plus one replaceable unsent draft, no replay/cancel of submitted work. Validate exact image/ranges/authorization before submit; production beta.5 write denial remains. Inject delay/fault/disconnect and prove responsive close/Quit plus bounded queue. Run N(rgb), N(vial_hid_task), N(ui::), LINT-N. **Exit:** offline responsiveness/fail-stop proof; H02 remains separately gated; next B07. **Stop:** proposed fix needs broader write permission.

### B07 — finite unlock operation (#4)

**Start assignment:** Execute B stage 4 only. Needs B06. Files: `src/vial/unlock.rs`, `src/ui/vial_hid_task.rs`, tests. Preserve exact ownership/identity checks; add already-unlocked success and bounded states/deadline from B, terminal errors and cancellation without retry loop. Test busy helper, timeout, denial and disconnect; run N(vial_unlock), N(unlock), N(ui::), LINT-N. **Exit:** no indefinite polling and offline transition evidence; H03 pending; next B08. **Stop:** unknown device or a physical unlock requirement in an automatic test.

### B08 — multi-frame indicator/HID-loan stability (#5)

**Start assignment:** Execute B stage 5 only. Needs B07. Files: `src/ui/ak820_os_indicator.rs`, `src/ui/device_connection.rs`, existing inline egui tests. Preserve Job::Sample/Write generation/freshness and cached loan facts. Add 100 steady plus 100 alternating EN/RU multi-frame regressions asserting version, Backups page/dialog/capture state, connection availability and generation; inject stale completions/competing work. Do not guess a physical root cause or change saved mappings. Run N(ak820_os_indicator), N(ui::), LINT-N. **Exit:** frame assertions and partial-versus-live limits recorded; next B09. **Stop:** a fix would enable indicator writes or reset page ownership.

### B09 — remaining navigation/backup entry gaps (#6)

**Start assignment:** Execute B stage 6 matrix only. Needs B08. Files: `src/ui/ak820_status.rs`, `src/ui/ak820_backup_inspection.rs`, `src/ui/app_settings.rs`, `src/ui/layout_settings_dropdown.rs` tests and EN/RU catalogs. Preserve persistent native pages, one native AppSettings service route and Wasm-only menu. Add source observation/alias versus selected-target rows, inspection/blocked-restore first steps, safety/back/cancel behavior and exact RU editor/import/export explanation. Do not remove an already-absent native duplicate or alter Wasm behavior. Run N(ui::), LINT-N; attach evidence for all six matrix rows. **Exit:** complete offline matrix, no navigation redesign; next B10. **Stop:** provenance would be inferred from byte-identical firmware or target selection.

### B10 — detected layout versus LED mappings (#7)

**Start assignment:** Execute B stage 7 only. Needs B09. Files: `src/ui/ak820_os_indicator.rs`, its tests and EN/RU catalogs. Preserve partial existing changes; display detected EN/RU separately from saved EN/RU LED choices and paused/mode state. Explain unchanged mappings during pause; test EN detected with EN=Off/RU=On, unknown layout, disabled/paused helpers and target changes without sending writes. Run N(ak820_os_indicator), N(ui::), LINT-N. **Exit:** explicit semantic regressions; exact beta.5 physical LED observation pending H01; next B11. **Stop:** inherited helper would write during observation.

### B11 — installer release selection and admission

**Start assignment:** Execute B stage 8 only. Needs B01/B03/B10. Files: `src/ak820_release.rs`, `src/ui/ak820_install.rs`, `src/ak820_install_plan.rs`, `src/ak820_install_executor.rs`, admission tests. Replace beta4_1 boolean with the selected InstallerRelease enum; generalize common admission, preserve Beta3/Beta4_1 semantics and observed source aliases. Beta5 default requires prerelease opt-in and final pins. Test all 200 allowed pairs plus beta3/unknown/downgrade/digest/catalog/opt-in denials; settings restore stays denied. Run N(ak820_release), N(ak820_install), N(beta5), CAT, LINT-N. **Exit:** immutable installer preflight with no physical flash; next B12/C04 prerequisites. **Stop:** missing pin or confirmation content; no physical loader/erase.

### B12 — native integration regression and documentation

**Start assignment:** Execute B stage 9 offline integration portion. Needs B02–B11 accepted. Files: affected tests and matching README/INSTALL/REQUIREMENTS/PLAN/STATUS/release notes EN/RU; no new feature. Run N(all), LINT-N and CAT once on the integrated baseline; inspect local Markdown links and `git diff --check`. Verify issue coverage, snapshot/alias labels, release defaults and preserved write denials. Record exact counts and failures; obtain independent review of implementation and evidence. **Exit:** reviewed native offline candidate and platform/hardware checklist; next Q cards when C04 also ready. **Stop:** regression or a claim needing missing live evidence. Do not close #2–#5/#7 from offline tests.

## MCP implementation cards

### C00 — current CLI/tools and stdio/error coverage

**Start assignment:** Implement M0 protocol/CLI/reporting only, without final-image claims. Needs B00. Files: `src/mcp/{mod,backend,dto,mock,trace}.rs`, `scripts/test_mcp_stdio.py`, explicitly new dev-only `scripts/mcp-interop/{package.json,package-lock.json,check.mjs}`, MCP EN/RU docs. Preserve the six actual tools and CLI defaults; extend script coverage for current protocol generations, malformed/oversized input, invalid parameters, backend/access conflicts, shutdown, stderr/stdout separation and bounded cancellation. Create the official TypeScript SDK client and pinned Inspector wrapper exactly as M0 step 5 specifies, with locked mutually compatible versions, integrity metadata and a prepared cache; this is the sole implementation owner for independent-client coverage. No new production dependency, GUI/device startup or normal-test download. Run C(all), STDIO, LINT-C, then `npm ci --offline --ignore-scripts --prefix scripts/mcp-interop` and `node scripts/mcp-interop/check.mjs --binary "$CARGO_TARGET_DIR/debug/ak-configurator"`. Record versions/lock digest and SDK/Inspector results. **Exit:** reviewed host protocol evidence; exact-image and three-platform P0 gates still pending. Next C01 or C02. **Stop:** a change would replace session/test/trace APIs or weaken admission.

### C01 — exact bootstrap fixture and shared ownership

**Start assignment:** Finish M0 exact-image/ownership portion. Needs B01/B03/B04/C00. Files: `src/mcp/backend.rs`, inline fixtures/tests, `src/hid.rs`, `src/hid_ak820.rs` only for reproduced shared ownership defects. Add positive full packed-bootstrap fixture bound to B01; no OEM/full-ROM material in repository. Test FD00→FD01, unrelated VID/PID avoidance, ambiguity denial, busy/cancel/disconnect and bounded owner release. Run C(all), STDIO, LINT-C. **Exit:** exact fixture proof; platform P0 still awaits Q01–Q03. **Stop:** synthetic/linked fingerprint offered as trust or ownership flaw not reproduced.

### C02 — bounded private artifact store

**Start assignment:** Implement M private-artifact contract only. Needs C00; no final firmware needed for storage tests. Files: new `src/mcp/artifacts.rs`, module registration and inline tests. Implement the specified root, random IDs, payload/manifest schema, permissions/no-follow, byte/count limits, explicit eviction and atomic/durable storage policy. Test traversal/symlink, truncation, limits, collisions, full disk/fault cleanup and platform durability limitations using temporary synthetic data. Add `mcp_artifacts` test names; run C(mcp_artifacts), LINT-C. **Exit:** reusable private store, no new tool or device access; next C03 after C01. **Stop:** OS cannot enforce required isolation; do not silently downgrade privacy or auto-evict.

### C03 — shared verified backup tool

**Start assignment:** Execute M1 adapter portion using C02 store. Needs B04/B05/C01/C02. Files: `src/mcp/backend.rs`, `src/mcp/dto.rs`, existing shared backup engine, stdio script/tests. Add `ak820_backup` as R, reuse native reconciliation, source alias/provenance and generation; results are metadata/artifact ID only. Add `mcp_backup` tests for equality with native fixture, v3/u16, contention, mismatch/storage fault/cancel/disconnect. Update both stdio and official-SDK/Inspector expectations from six to seven tools. Run C(mcp_backup), N(ak820_backup), STDIO, LINT-C. **Exit:** byte-identical verified snapshot and no writer; next C04. **Stop:** adapter starts a second HID engine or leaks raw settings.

### C04 — immutable read-only preparation

**Start assignment:** Execute M2 only. Needs B11/C03. Files: shared installer/plan modules, `src/mcp/{backend,dto}.rs`, store integration, stdio script/tests. Add `ak820_prepare`, canonical versioned digest, strict fields/duplicate-key denial, expiry and source/backup/catalog binding. R/L remain available under actual device-read-only CLI; S/T/D/production execute remain denied. Add `mcp_prepare` mutation/200-pair/admission tests; update both stdio and official-SDK/Inspector tool inventories to eight. Run C(mcp_prepare), C(all), STDIO, LINT-C. **Exit:** eight-tool read-only candidate, no approval or writer; next Q01. **Stop:** restore authority inferred from an installer pair or new access-mode design required.

### C05 — optional approval-bound mock execute

**Start assignment:** Execute M3 only **after a separate explicit authorization to develop this mock writer path**. Needs C04 and that authorization; not required for read-only MCP acceptance. Files: shared plan/approval state plus MCP adapter/mock tests. Bind one-shot local approval to plan/source/generation and expiry; client cannot mint it. Add `mcp_execute` replay/timeout/cancel/uncertain-result cases, preserve terminal report and no retry. Mock inventory becomes nine; HID remains eight and direct execute denied. Run C(mcp_execute), C(all), STDIO, LINT-C. **Exit:** mock-only architecture evidence, not production execution/hardware qualification. **Stop:** missing authorization or any attempted device write/policy extension.

## Platform and physical qualification cards

### Q01 — Linux offline/process and artifact evidence

**Start assignment:** Qualify the reviewed integrated B12/C04 candidate on Linux; no feature changes. Files: Q01 evidence and matching installation/MCP docs only. Run N(all), C(all), STDIO, LINT-N/C and CAT on isolated outputs. Run C00's cached independent-client recipe (`npm ci --offline --ignore-scripts --prefix scripts/mcp-interop`, then `node scripts/mcp-interop/check.mjs --binary "$CARGO_TARGET_DIR/debug/ak-configurator"`); official-SDK handshake/read/status and Inspector discovery/tools-list must pass on this final candidate, with versions/lock digest/negotiated protocol recorded. Do not substitute the custom harness. Then exercise bounded pipe/Unicode/shutdown/cancel and permission-error fixtures. Verify the Linux x86_64/arm64 package matrix, SHA256/size and extraction/launch instructions against actual artifacts; do not claim an unrun architecture launch. Record UI smoke only on a disconnected/offline-safe setup. **Exit:** Linux process results and exact artifact evidence; next Q02. **Stop:** unavailable runner/artifact, unreviewed source or unexpected HID startup; report pending rather than changing code.

### Q02 — Windows offline/process and artifact evidence

**Start assignment:** Repeat Q01's bounded offline recipes on Windows using `.exe` and isolated targets; no device access. Needs B12/C04; Q01 is the default handoff predecessor, not a portability substitute. Verify x86_64/arm64 packages/checksums/instructions, pipe/Unicode/exit and mocked hidapi lease/error mapping. Record each unavailable architecture separately. **Exit:** Windows stdio and artifact evidence, live issues still open; next Q03. **Stop:** unavailable platform/runner or runtime failure; implementation correction returns to its owning card in a fresh session.

### Q03 — macOS offline/process and artifact evidence

**Start assignment:** Repeat bounded offline/process tests on macOS, plus permission-error fixtures; no device access. Needs B12/C04 and default Q02 handoff. Verify x86_64/arm64 artifact identities, extraction/launch instructions, stdout/pipe/Unicode/cancellation/exit; distinguish unavailable launch from build success. **Exit:** macOS stdio/artifact evidence; next Q04. **Stop:** missing runner/artifact or failed required test; do not label Linux/Windows results as macOS proof.

### Q04 — cross-platform security, release docs and final review

**Start assignment:** Complete M4 and B stage 9 evidence assembly without implementation. Needs Q01/Q02/Q03 accepted for full three-platform read-only completion; may assemble an explicitly pending report earlier. Files: final reports, release notes, README/INSTALL/MCP/STATUS EN/RU. Review no path/HID/shell escape, raw-data leak, schema/size/rate bypass, concurrent writers, approval escalation/replay or uncertain retry. Reconcile six application targets and checksums, platform test counts, issue-specific offline/live state and C05 deferred state. Check links/diff and obtain fresh independent review. **Exit:** accepted read-only MCP only if M0–M2/M4, Q01's official-SDK/Inspector interoperability and all three stdio platforms pass; otherwise precise pending milestone. H cards do not gate honest offline completion but do gate their live issue closure. **Stop:** unclosed security finding or missing required platform evidence; never report all P1/P2 complete.

### H01 — separately admitted Windows read-only issue observation

**Start assignment:** Qualify live #2/#5/#7 only after B12/Q02, exact connected beta.5 identity and explicit admission for a proven read-only launch/observation. No key presses, rotation, reconnect, unlock or option writes are required. Inspect inherited auto-writing helpers before launch; leave them disabled if they could write. Collect snapshot/preview-or-specific-denial, version/Backups timing at steady and EN/RU layout changes, and detected/mapped/paused LED observation for exact firmware. Natural OS-layout changes are not simulated device input. If safe observation cannot cover active indicator behavior, record it pending, not passed. **Exit:** per-issue read-only evidence or limitation; no broader closure claim. **Stop:** any setter, manual interaction requirement, identity mismatch, lease/timeout/storage/readback fault. Never retry/reset/restore.

### H02 — gated Windows RGB live qualification (#3)

**Start assignment:** Do not run while beta.5 write policy denies this operation. Needs B06/Q02 plus exact image support, explicit release-policy admission and fresh authorization for the particular RGB operation. Only then use B stage 3's bounded responsiveness/Quit evidence and exact write budget; no automatic replay or autosave beyond approved contents. **Exit:** specific #3 physical/protocol evidence, not general write qualification. **Stop:** policy still false, missing authorization or first uncertain/error result. Until admitted, the card's deliverable is a blocked qualification report, not a device test.

### H03 — separately authorized Windows unlock qualification (#4)

**Start assignment:** Needs B07/Q02 and explicit authorization for the exact unlock operation on an admitted image. Check shared owner and already-unlocked state first. Observe the bounded B stage 4 state machine without retries or unrelated writes; do not ask for an unapproved manual key sequence. **Exit:** exact Windows unlock result and safe termination evidence or pending limitation. **Stop:** missing authorization, physical interaction requirement, ownership conflict, timeout or failure. This does not authorize restore, loader or flash.

## Starting text for the next agent

> Execute only card `<ID>` in docs/SESSION-EXECUTION-PLAN.md. Read its common contract, prerequisite handoffs and linked B/M steps before editing. Verify the baseline and exclusive worktree/output ownership. Perform the card's already-decided actions and exact check recipes, preserve all device/write gates, and finish its EN/RU evidence handoff. Do not start another card, commit/push without authorization/review, or invent a missing architecture/input. If blocked, save the specific unmet gate and ask only the necessary question. This assignment is valid only when the operator separately authorizes execution of this card.

Replace `<ID>` with the single next admitted card from the preceding handoff, not an entire B/M work group. The next agent can read the repository and reports instead of inheriting a huge chat transcript.
