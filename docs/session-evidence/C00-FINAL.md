# C00 — final-source offline verification, 2026-11-03

Russian: [RU](../ru/session-evidence/C00-FINAL.md).

## Status and scope

At this report's original checkpoint, the required local checks passed on the final C00 source, the independent scoped completion review and documentation correction were closed, and native-goal acceptance remained pending. Subsequently native goal `mvzpnqkx-lhzhes` was accepted and archived as complete; its predecessor native session was `01a19df2-4f0c-7093-a9a8-b71a60654fdd`. The successor confirmed the archived goal and the operator's acceptance handoff. C02 is admitted on C00; B00 remains accepted, while B01 and C01 are not admitted. This is not a general final-project review or hardware qualification.

HEAD remains `936a989a8fd7d8e24cbd70ddba0e768e3107494b`. Dirty/recovered work is preserved. No reset, clean, stash, commit or push was performed. This session used `low` with explicit user permission; the independent reviewers used Terra/medium. The later carte-blanche instruction authorized technical/API cleanup, not lint suppression, fake uses, admission weakening, device writes or loss of working functionality.

## Current evidence

All paths below are under `LOCAL_WORK/ak-c00-mvzpnqkx-evidence/`. Each command's exit, complete output and memory record are retained separately. Runs were sequential, constrained to the owned build process group; no shared previous target was used.

| Check | Log | Result |
| --- | --- | --- |
| C(all): `cargo test --locked --features mcp-mock --bin ak-configurator -- --test-threads=1` | `test-all-final-c00.log` | exit 0; 2048 passed, 14 ignored, 260.93s |
| `cargo fmt --check` | `fmt-final-c00.log` | exit 0 |
| LINT-C: `cargo clippy --locked --all-targets --features mcp-mock -- -D warnings` | `clippy-all-final-c00.log` | exit 0 |
| `cargo build --locked --features mcp-mock` | `build-mock-final-c00.log` | exit 0 |
| Separate `cargo build --locked --features mcp` | `build-production-final-c00.log` | exit 0 |
| `python3 scripts/test_mcp_stdio.py --binary <mock> --production-binary <production>` | `stdio-final-c00.log` | exit 0; protocols 2025-11-25 and 2026-07-28; production metadata and mock rejection; no hardware calls requested |
| `npm ci --offline --ignore-scripts --prefix scripts/mcp-interop` | `npm-ci-real-hid-boundary-complete.log` | exit 0; locked dependencies; no lifecycle scripts |
| `node scripts/mcp-interop/check.mjs --binary <mock>` | `sdk-inspector-final-c00.log` | exit 0; SDK handshake/schema/fixture identity/status and Inspector discovery/tools-list passed; six tools, 0 portability errors, 43 warnings |

The pinned SDK version is `1.32.2`; Inspector is `2.13.0`. The lockfile SHA256 is `f6ddac847d502033e99b399bbd86b9a5dc5327cf9849eb1cf89665458969836e`; npm's offline install used its integrity metadata. The SDK validator warns that unknown `uint64` formats are ignored. Passing interop does not prove format enforcement by that validator. Ignored Rust tests were not run. Live device behavior, flashing, retention, physical input, other-OS builds and Wasm execution were not qualified. Previous failing Clippy, mock/production, STDIO and SDK results are historical, not substitutes for the rows above. The final formatting-only removal of an empty line in `src/app_init.rs` precedes every `final-c00` run.

## Provenance and retained resources

`provenance-final-c00.log` records log hashes, exits and binary identities. `source-final-c00.sha256` lists 706 source/build/asset/script/test/vendor files; its SHA256 is `742530bbb2889009ac76eacc0d7d7151651859caaf27e1cedd1353791334837c`. Documentation and goal state are outside that source manifest. It is not an assertion that the entire dirty tree equals HEAD.

- Mock: `LOCAL_WORK/ak-c00-mvzpnqkx-mock-target/debug/ak-configurator`, 152489032 bytes, SHA256 `fbcbc97e7380a3aa88e84db14ceba09a629f892ea7b041fda9a6d39d8f7d226f`.
- Production: `LOCAL_WORK/ak-c00-mvzpnqkx-production-target/debug/ak-configurator`, 152473920 bytes, SHA256 `3a80d20d3f6a62be86f1e0a716d908c3f7cc8ddc25befef0a140792853ffdaef`.

Both unique target directories and the evidence directory are retained for reproducibility/handoff. Earlier targets/evidence/recovery resources remain untouched. The session-created `scripts/mcp-interop/node_modules` was removed after checks exited; it was absent before this session's last offline npm install. No owned test/build/SDK process remained at cleanup inspection. Independent reviewer artifacts are retained by the harness.

## Code boundaries and independent review

The eight originally authorized API guards retain their original comparison/proof; later metadata, platform and dormant-API changes supersede that source. Historical catalog tests do not admit new firmware. Hash-pinned description fields are test-only only where runtime loaders check exact embedded bytes before deserialization; identity/admission fields and pins remain. Typed application-layout deserialization and error ordering remain before refusal; single-profile reference payload/dialog helpers are test-only, not claimed equivalent to native device-layout import/export.

Dormant UI/reference operations are separated from reachable production paths. Actual slot pictogram upload, device layouts, QSID1 settings, updater runtime, Linux backend-row install/delete/reload and macOS setup routes remain. Historical page/slider tests use inert isolated state and make no live operations. Real `Local`/`LinuxBle` backends and their hardware-only dependency closure are excluded from unit-test compilation, not production. `Test`/`Proxy`, fail-closed test live-open, framing/reply/timeout pure fixtures and ownership paths remain. No lint attributes or dummy consumers were introduced.

Earlier scoped source and EN/RU evidence reviews are recorded in the historical C00 handoff. The latest setup closure (`54cafe49-dbfe-4e16-aade-c66034419300`), reference-operation closure (`89fd810d-93c5-47eb-b19d-641473142d0b`), slider (`2953bfbd-d924-4c23-8563-d9d53b918eb5`), navigation/checkpoint (`ca865ff8-fc9d-4c5d-8964-ee008a16804a`) and real-HID closure (`e0d784fb-bb7e-40ec-9451-dd08e793c493`) reviews found no issues. They did not rerun builds/tests or qualify devices/cross-platform builds. Prior documentation findings were corrected and rereviewed; none of those scoped verdicts independently accepts all C00.

## Scoped completion review record

Review `75e3cff5-25c8-46ac-8909-e762cf34aa97` checked the final logs/exits, latest source boundaries, EN/RU report and resource state. It supports bounded C00 closure, not uncovered historical code or hardware/cross-platform qualification. Its P2 finding was contradictory unlabeled API-cleanup paragraphs in six PLAN/STATUS/ARCHITECTURE documents; all six are now explicitly historical. Follow-up `3bd706a3-6a5e-4078-847b-3d5219f2c6ed` found no issues and closed the documentation contradiction; it did not independently verify the prior review artifact or the author's manifest recheck. The reviewer could not independently recompute source/binary hashes/sizes or isolate the final formatting-only edit; provenance remains author-verified retained evidence, not an independently recomputed identity. The author rechecked all 706 manifest entries after documentation updates; source files were unchanged.

## Handoff gate

After scoped completion review and acceptance, stop this writer before handing C02 to exactly one new ordinary session under the session chain. The successor must verify current ownership/model, preserve all dirty/recovered material and use its own target directory. It must not reuse C00 acceptance as admission for B01/C01, live writes, commit/push or a general final review.
