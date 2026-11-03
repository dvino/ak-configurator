# Completing MCP integration

Status: execution plan, not implementation evidence. Date: 2026-11-01.
Base contract: [MCP_IMPLEMENTATION_PLAN.md](MCP_IMPLEMENTATION_PLAN.md). This document adapts it to current P0 and beta.5; stricter safety limits take precedence.

## Outcome and boundaries

MCP is complete when its stdio server passes offline protocol/integration tests on Linux, Windows and macOS, uses the same exact-image and operation engine as native UI, supports read, backup, immutable prepare and explicitly authorized execute, prevents concurrent HID writers and passes security/admission tests.

Beta.5 has `settings_writes_authorized=false`. Completion does **not** enable QS, macro, layout, RGB, AKBT, reversible-suite or protected-update writes. Those scopes remain future work pending separate release authorization. Do not label all P1/P2 complete when only the read-only subset is delivered.

MCP must not bypass UI authorization, device leases, packed identity or physical confirmation. No arbitrary paths, arbitrary HID bytes, shell, telemetry, remote calls, automatic retries or background startup.

## Preserve the existing implementation

Keep `src/mcp/mod.rs`, `backend.rs` and `dto.rs` architecture:

- opt-in `--mcp-stdio`; optional `--mcp-access=read-only` and `--mcp-backend=hid` are current defaults, while `--mcp-backend=mock` requires feature `mcp-mock`. Do not invent `--read-only`, `--hid`, `--mock` or scope flags;
- `rmcp 3.8.0` stdio, 64 KiB JSON-RPC frame limit;
- six tools: `ak820_devices`, `ak820_session`, `ak820_read`, `ak820_tests`, `ak820_trace`, `ak820_operation`; preserve their actions, resource behavior, closed schemas and result envelope. Full layout/settings reconciliation is new domain integration, not an existing read-layout/read-settings tool;
- one bounded `sync_channel(1)`, one owner, cached operation/report IDs;
- closed DTO schemas with no settings/macro/payload trace;
- FD00-first identity then FD01, metadata trace and cancellation;
- irreversible real-device/mock guard;
- no network, writes or firmware payloads.

Do not add a second backend, thread-per-tool, custom JSON parser or copied HID protocol.

## Target architecture

### Shared operation engine

Use one domain flow: `DeviceDiscovery → ExactIdentity → ReadSnapshot`; then `BackupArtifact | ImmutablePlan`; then `ImmutablePlan + Approval + FreshIdentity → ExecuteOperation → TerminalOperationReport`.

MCP DTO projects domain results only. Exact `ImageContract`, QS v1/v2/v3, CODE digest, macro getter, lease, generation and fail-stop behavior remain in the domain. MCP must not know QSID offsets or layout bytes.

### Nine tools

Add `ak820_backup` (verified snapshot into private storage), `ak820_prepare` (immutable offline/read-only plan) and `ak820_execute` (supported plan plus one-shot approval), without renaming/replacing the six tools. `ak820_operation` retains its existing `status/cancel` actions for new operations too; do not add list/get actions. Keep the existing result envelope; operation-bearing responses add ID, generation, terminal state, domain error and safe diagnostics. Nine schemas are the target inventory; production exposes only admitted tools (eight after M2); mock-only execute is not advertised on HID. Normal responses never contain OEM bytes, setting defaults, macro payload or secrets.

### Access scopes

R: discovery, identity, layout/settings read and backup. L: prepare from private local artifacts. S: saved settings, denied for beta.5. T: reversible suites, currently denied. D: destructive/protected update, currently denied.

The current CLI has no scope-selection flag. Preserve `--mcp-access=read-only`: it forbids device mutation, not bounded private local storage. Current P0 exposes R; M1 adds R backup and M2 adds L prepare under that same access policy. Do not require a writer access mode for prepare or add a scope parser in this release.

| Admission | P0 | After M1 | After M2 | HID mutation |
|---|---|---|---|---|
| R discovery/read/test/trace | Existing tools | Preserved | Preserved | Never |
| R backup, local artifact creation | Not yet implemented | Allowed | Allowed | Never |
| L local artifact/prepare | Not yet implemented | Not yet implemented | Allowed | Never |
| S/T/D, production execute | Denied | Denied | Denied | Denied |

L accepts artifact IDs only, never arbitrary paths. Hide execute in HID tool lists and deny direct calls with stable `authorization_denied`. M3 permits only feature-gated mock execution after separate development authorization; it does not alter HID admission.

### Plans and approval

Prepare binds exact source CODE SHA256, target release/profile, catalog/matrix/capability digests, QS source/target versions, backup digest, operation kind, limits and expiry. Changed input invalidates rather than silently recalculates a plan.

Execute accepts only plan ID, approval ID, expected generation and expected source CODE hash. Approval is short-lived, one-shot and bound to plan digest and local user confirmation. No MCP tool may create approval. Beta.5 production execute remains unavailable; mock execution is not hardware qualification.

### Ownership and termination

Share one per-device owner with UI: `Ready`, `LeaseBusy`, `OperationBusy`, `Disconnected`, `Failed`. Execute requires a lease and fresh exact identity. Never wait indefinitely or steal a lock.

Every operation ends `Succeeded|Failed|Cancelled|TimedOut`. Cancel before submit sends nothing. After submit, cancel only waits if supported and never claims rollback. Never retry an uncertain write. Disconnect/shutdown never triggers reset/restore.

### Private artifacts

Implement `src/mcp/artifacts.rs`. Root: `dirs::data_local_dir()/ak-configurator/mcp-artifacts/v1` (fall back to an explicit per-user application data error, never the current directory). Each random 128-bit lowercase-hex ID owns one directory containing `payload.bin` and `manifest.json`. Manifest schema `ak820.mcp.artifact/v1` stores kind, created UTC, payload length/SHA256, source CODE/release, QS version, completeness and plan expiry; no arbitrary user path. Maximum backup payload is 16 MiB, plan payload 1 MiB, manifest 64 KiB, with at most 100 retained artifacts; eviction is explicit by artifact ID, never automatic.

Use 0700 directories, 0600 files, `O_NOFOLLOW`/platform equivalent and server-generated IDs only. Write a same-directory temporary file, flush and `sync_all`, close, atomically rename, then `sync_all` the payload directory and manifest directory on Unix; on Windows flush file buffers and use atomic replace, reporting an unsupported directory flush as a durability limitation rather than success evidence. Any failure removes only the new temporary/incomplete artifact and returns storage error. Backups contain private raw data; responses expose metadata/digests. Trace/report is metadata-only by default. Export is a separate user operation with an explicit path, not an MCP path parameter.

Session boundaries and ready assignments are in [SESSION-EXECUTION-PLAN.md](SESSION-EXECUTION-PLAN.md): C00/C01 split M0, C02/C03 split storage/backup, C04 covers prepare, separately gated C05 covers mock execution, Q01–Q04 cover acceptance. Shared B work is prerequisite, never duplicated.

## M0 — finish P0

Files: existing `src/mcp/{mod,backend,dto,mock,trace}.rs`, `src/hid.rs`, `src/hid_ak820.rs`, `scripts/test_mcp_stdio.py`, MCP docs. Extend existing inline unit tests and the subprocess script; do not invent library/integration targets. Commands run in the owned checkout with an isolated target directory; replace `$CARGO_TARGET_DIR/debug/ak-configurator` by its `.exe` form on Windows.

1. **Completed in C01 for the limited offline scope:** an official-recipe positive beta.5 full packed bootstrap fixture is host-qualified. C01 records exact input/fixture identities. This does not establish an installed image or physical-device behavior; synthetic FD01 fixtures alone do not establish production trust.
2. Test FD00→FD01, no probing unrelated VID/PID, ambiguous denial and exact CODE admission.
3. Fix only reproduced live ownership/helper/cancel/disconnect gaps; prove no concurrent read/write and bounded termination.
4. Add safe stage/timing/error reports; leave payload trace disabled.
5. Protocol tests: initialize, list/call, malformed JSON, 64KiB+1 frame, unknown method/tool, invalid params, notification and shutdown. Stdout is protocol only; logs go to stderr. C00 also creates explicitly new dev-only `scripts/mcp-interop/{package.json,package-lock.json,check.mjs}`: use the official TypeScript MCP SDK client and pinned MCP Inspector, not another handwritten client. Resolve exact mutually compatible versions once during C00, record their versions/integrity in the lockfile/report, and provision a controlled cache. `check.mjs --binary <mock-binary>` performs official-SDK stdio handshake/discovery, tool schema/list and a safe fixture read/operation-status call, then an Inspector CLI discovery/tools-list check against the same mock. Preserve the actual status/cancel schema; no GUI, network service or HID backend. Pin all transitive inputs; no floating npx/download during normal tests. Offline execution uses `npm ci --offline --ignore-scripts --prefix scripts/mcp-interop` followed by `node scripts/mcp-interop/check.mjs --binary "$CARGO_TARGET_DIR/debug/ak-configurator"`. The wrapper is a deliberate new interoperability deliverable, not an existing Rust test target. Missing cache or no compatible official client is a failed gate, not permission to substitute the handwritten harness. Q01 runs it on the final candidate; C03/C04 update expected seven/eight-tool inventories.
6. CLI matrix: `--mcp-stdio` alone defaults to read-only HID but must not discover devices before a request; explicit access/backend gives the same admission. Reject unknown flags/access modes and duplicate/conflicting backend/access arguments. A build with feature `mcp` but without `mcp-mock` rejects mock; feature-enabled development builds test mock without touching HID. Preserve irreversible real→mock guard. Test denied write calls without sending setters. No-flags GUI startup is not an offline acceptance test.
7. Run `cargo test --locked --features mcp-mock --bin ak-configurator mcp:: -- --test-threads=1`, `cargo build --locked --features mcp-mock`, `python3 scripts/test_mcp_stdio.py --binary "$CARGO_TARGET_DIR/debug/ak-configurator"`, `python3 .github/scripts/test_beta5_catalog.py` and `cargo clippy --locked --all-targets --features mcp-mock -- -D warnings`. Use a separately isolated feature-`mcp` binary with the script's `--production-binary` option to verify mock denial. Set `CARGO_TARGET_DIR` explicitly per session; reject zero-match filtered test runs.

Gate: The positive exact host fixture was accepted in C01. P0 as a whole remains partial until required Linux/Windows/macOS STDIO and remaining live/platform checks pass; record unavailable platforms as pending. Offline M1 may continue.

## M1 — shared beta.5 snapshot/backup

**C03 checkpoint (2026-11-03):** `ak820_backup` is implemented as the seventh MCP tool and reuses the native reconciler and C02 store. Focused mock tests, mock STDIO for both protocol generations and the official SDK/Inspector mock check are reported passing; see [C03 evidence](session-evidence/C03.md). The production-feature build and combined production STDIO passed for both protocol generations; metadata tools passed and the mock backend was rejected in production. Independent review `c03_backup_review` accepted the limited offline C03 scope after checking documentation/source and 71 file hashes; reported tests/builds/stdio/SDK results were not rerun. Device behavior and source-to-binary linkage remain unqualified.

**C04 checkpoint (2026-11-03):** `ak820_prepare` is implemented as the eighth tool and reuses the native install-prepare builder and C03 archive for a fixed install preview only. Settings-restore plans are unsupported; `settings_writes_authorized` remains false. The current full Windows mock-feature suite passed 2,015 tests with 18 ignored in 288.93 s; three ignored C04 fixtures passed in separate runs and the remaining ignored cases are unqualified. Focused `mcp_prepare`, strict Clippy/format, mock/production builds, combined STDIO and official SDK/Inspector checks passed. Reviewer `c04_final_review` verified the 103-file inventory and source/patch/report/binary hashes, closed four documentation findings and accepted the limited offline Windows C04 scope. The inventory is sparse; no source-to-binary attestation or three-platform/GUI/device qualification is claimed. See [C04 evidence](session-evidence/C04.md). This is limited Windows acceptance, not full MCP completion.

Dependencies: beta.5 stage 2. Files: shared backup engine, MCP backend/DTO, artifact-store module/tests.

1. Connect MCP to the same full NVM/macro/QS reconciliation as native backup, including v3 and u16 QSID23 through domain APIs.
2. Add R-only `ak820_backup`; return artifact ID, observed source/CODE digest, QS version, completeness and safe errors, not raw settings. Preserve canonical Beta4 plus explicit beta.4/4.1 byte-identical alias; receipt-based package provenance is separate from device observation.
3. Implement private storage with atomic temp→rename, fsync policy, size limits and storage-fault stop. Never automatically clean active/foreign artifacts.
4. Enforce generation across reconnect; distinguish UI/MCP busy from lease/disconnect.
5. Test beta.4.1/beta.5, v1/v2/v3, macro/QS23 mismatch, corrupt storage, symlink/traversal, concurrent backup, cancel/disconnect and stale generation.
6. Add `mcp_backup`-named inline regressions; run `cargo test --locked --features mcp-mock --bin ak-configurator mcp_backup`, `cargo test --bin ak-configurator ak820_backup` and the M0 stdio script after rebuilding the mock binary. Update its tool-list expectation to eight only after both M1 and M2 are implemented (seven after M1).

Acceptance: MCP and native snapshots are byte-identical for one fixture; no writer exists.

## M2 — immutable prepare

Dependencies: M1 and beta.5 stage 8. Files: shared plan module, MCP backend/DTO, fixtures/tests.

1. Add prepare for supported offline/install plans. Settings restore returns `settings_writes_authorized=false` even when an installer pair exists.
2. Use versioned canonical JSON and digest with every exact identity; reject unknown fields and duplicate keys.
3. Verify all 200 installer pairs and deny beta.3/unknown/downgrade/digest mismatch. Never return packed payload.
4. Test expiry, source/backup binding and mutation of catalog/target/backup. Execute parameters cannot edit a plan.
5. Add `mcp_prepare`-named inline regressions and run `cargo test --locked --features mcp-mock --bin ak-configurator mcp_prepare`, plus the rebuilt M0 stdio harness. Reject zero matches; test R/L admission under the actual read-only CLI.

Acceptance: prepare is offline/read-only, opens no writer and creates no approval.

## M3 — approval-bound execute in mock only

Dependencies: M2 and separate authorization to develop a writer path; this plan does not authorize a physical operation.

1. Implement execution state machine behind feature/scope gates. Beta.5 production returns stable denial until release policy changes.
2. Use the existing native/local confirmation flow for approval, bound to plan/source/generation and one use.
3. Recheck every digest and confirmation content before loader/erase. Preserve fresh Esc+left Control, flash-once and readback rules.
4. In mock test success, denial, expiry, approval replay, wrong generation/source, disconnect, timeout, cancel before/after submit and uncertain result. No failure retries.
5. Preserve terminal reports across client disconnect; another client cannot alter approval.
6. Add `mcp_execute`-named inline regressions; run `cargo test --locked --features mcp-mock --bin ak-configurator mcp_execute` and the rebuilt M0 stdio harness. Update the mock tool inventory to nine only in this authorized stage; HID inventory remains eight.

Gate: no production promotion without a separate decision and independent security review. Physical tests require separate authorization.

## M4 — platform, security and operational acceptance

1. Test stdio framing, Unicode, broken pipe, shutdown, cancellation and process exit on Linux/macOS/Windows with isolated build outputs. Q01 additionally runs C00's cached official-SDK client and pinned Inspector checks on Linux; record client/Inspector/Node/npm versions, lockfile digest and negotiated protocol. One-platform independent interoperability is required in addition to the custom subprocess harness and three-platform stdio evidence.
2. Test Windows hidapi lease/error mapping, macOS permissions and Linux udev. Permission denied is not disconnected.
3. Security checklist: no arbitrary paths/HID/shell, secrets/OEM logs, concurrent writers, scope escalation, approval forgery/replay or uncertain-result retry; enforce closed schemas and size/rate bounds.
4. Run full offline suite, docs links, fmt and clippy; record versions/counts.
5. Update MCP EN/RU docs with actual tools/scopes, launch, limitations and evidence; do not list S/T/D as supported.
6. Independent review compares implementation with original P0–P2 and this plan. If S/T/D remain deferred, report “read-only MCP complete; write scopes deferred.”

## Commit order

1. `mcp: complete read-only protocol and exact beta.5 coverage`
2. `mcp: share verified beta.5 backup snapshots`
3. `mcp: add immutable operation preparation`
4. `mcp: add approval-bound mock execution`
5. `docs: record MCP platform and security evidence`

Each requires independent review. Keep the domain beta.5 contract separate from the thin MCP adapter.

## Current completion criterion

An available-platform implementation milestone may finish while other platform runs remain pending, but it is not accepted read-only MCP completion. Accepted completion requires M0–M2, the M4 security/operational checks, cached official-SDK/Inspector interoperability results from Q01 and stdio results on all three platforms; backup/prepare contain no writer; unsupported scopes are consistently denied; documentation and independent review are complete. M3 provides execute architecture in mock only. Production execute, S/T/D and physical qualification remain outside the current beta.5 release and must be reported as limitations.
