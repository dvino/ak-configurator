Русский перевод: [Диагностика через MCP](ru/MCP.md).

# MCP diagnostic preview

This is the first, **partial P0 implementation** of [the integration plan](MCP_IMPLEMENTATION_PLAN.md). Current beta.5-aware execution and acceptance gates are in [the MCP completion plan](MCP-COMPLETION-PLAN.md). It is read-only and requires the `mcp` build feature. Desktop candidate CI includes that feature; a plain Cargo build omits it. It is not a completed P0 acceptance or an alpha release qualification. The application version remains `0.1.0-alpha.1`. See [REVIEW.md](REVIEW.md) for the source and platforms actually checked.

## Build and connect

Build from this source with the pinned lockfile:

```sh
cargo build --locked --release --features mcp
```

Configure a local MCP client to launch `target/release/ak-configurator` on Linux or `target/release/ak-configurator.exe` on Windows with these arguments:

```json
["--mcp-stdio", "--mcp-access=read-only"]
```

Use an absolute executable path. The process speaks MCP over inherited stdin/stdout using the official `rmcp = 3.8.0` SDK. It starts without a GUI or display and exposes no MCP network listener. Handshake and discovery of tools/resources do not enumerate USB or open a keyboard. The executable still links its existing GUI dependencies. Desktop candidate CI builds production MCP into the packaged executable. A build without the `mcp` feature rejects MCP startup; `mcp-mock` is reserved for separate tests.

Close the GUI connection or background helper before asking MCP to open the same keyboard. The existing cooperative device lease is retained; MCP does not force another owner out. A busy or inaccessible device produces an error. Other programs may not honor this application's lease.

## Available tools

| Tool | Current actions | Device effect |
| --- | --- | --- |
| `ak820_devices` | List candidates; `refresh` requests a fresh enumeration | USB enumeration only; a candidate is not a recognized image |
| `ak820_session` | `open` with `load: "diagnostics"`, cached `status`, `close` | Explicit exclusive owner and diagnostic bootstrap; no settings load |
| `ak820_read` | `identity`, `capabilities`, `storage`, `connection` | Identity/capabilities are cached; storage/connection issue typed FD01 reads |
| `ak820_tests` | `list`, `run` with `protocol.smoke`; mock also has `protocol.errors` | Bounded diagnostic read or synthetic error checks; no setters |
| `ak820_trace` | `start`, `status`, `stop`, metadata mode only | Records exchange metadata through the existing read-only transport |
| `ak820_operation` | `status`, `cancel` | Inspect an open/backup operation or cancel while it is cancellable; backup publication cannot be cancelled once started |
| `ak820_backup` | Create a verified backup and retain it in the private artifact store | Read-only device capture; returns metadata and artifact ID, not raw settings |
| `ak820_prepare` | Create the fixed offline install preview from an existing backup | Offline; reads local release/backup data and stores a bounded plan artifact; `settings_writes_authorized` is false; settings-restore plans are not supported |

Inputs have closed schemas. There is no arbitrary HID command, address, shell, path, unlock, settings setter, reset, loader or flash tool. The read-only HID policy is irreversible for the process. Neither successful replies nor a `protocol.smoke` pass establish physical switch, encoder, LED or retention behavior.

## Diagnostic sequence

1. Call `ak820_devices` with `{"refresh":true}` and select a returned opaque `device_id`. Never construct one from a USB path.
2. Call `ak820_session` with `{"action":"open","device_id":"RETURNED_ID","load":"diagnostics"}`. An `accepted` result returns `data.operation_id`; it does not mean recognition succeeded.
3. Poll `ak820_operation` with `{"action":"status","operation_id":"RETURNED_OPERATION"}`. On completion, inspect `data.state` and the nested `data.result` envelope. Obtain `device_session_id` and `session_generation` from the successful nested result. A failed storage check can retain a stopped session for cached inspection and explicit close.
4. Read fixed sections or run `protocol.smoke` with that session ID. The smoke suite currently reads diagnostic status and explicitly marks physical checks `SKIP`.
5. To capture a verified backup before closing the session, call `ak820_backup` with `{"device_session_id":"RETURNED_SESSION","expected_generation":RETURNED_GENERATION}`. Poll its `operation_id` with `ak820_operation` until terminal. A successful result contains `artifact_id`, payload and native-manifest SHA256 values, payload length, QS version, completeness and durability status. `directory_flush_unsupported` reports the Windows store limit. The private store retains the artifact; the response contains no payload bytes or arbitrary path. Cancellation is available before publication starts; publication is non-cancellable and is not rolled back.
6. Close the session explicitly. Session and operation IDs are local to the process; do not treat them as filesystem paths.

For a real keyboard, bootstrap reads the manifest and complete packed CODE through bounded FD00 requests and matches the exact compiled image contract. Only recognized beta.3 and beta.5 images reach FD01 and subsequent ordinary protocol/UID getters. FD01 is checked before those getters and after them. Dirty/batch/unlock activity, faults, malformed replies, timeout or disconnect stop the session without speculative follow-up or automatic retry. Diagnostics never mark settings ready. The normal GUI verification path now also identifies CODE before its ordinary getters.

There is one owner, one device operation at a time and a bounded worker queue. Full CODE recognition has a 120-second boundary deadline and cancellation checks between packets; an individual transport exchange retains its existing timeout. Operation state exposes coarse stages, not byte-level progress. Standard MCP request cancellation detaches that response; it does **not** cancel unrelated worker work. Use `ak820_operation.cancel` to request cancellation of an asynchronous open or backup before publication starts; backup publication cannot be cancelled. EOF requests worker cancellation and waits for the worker/helper to finish. Ownership is not released merely because a client stopped waiting.

## Reports and traces

Tool results use `ak820-mcp-result-1` in `structuredContent` with the SDK's text fallback. They carry backend/evidence labels, image/session identity when available, verification limits and structured errors. An accepted operation must be followed to its terminal result. Tool failures set `isError`; physical behavior is always marked unvalidated.

Resources include `ak820://server`, the compiled public catalog at `ak820://catalog/<hash>`, empty registered inputs at `ak820://inputs`, and recent smoke reports at `ak820://report/<id>`. Up to 32 smoke reports are retained in memory. The error suite returns its checks directly. Nothing in this preview writes a report to an arbitrary user path; a client can save returned JSON itself.

A trace requires an open session and `mode: "metadata"`. Limits are at most 8 MiB and 60 seconds. Start returns an opaque trace ID and a page resource such as `ak820://trace/<id>/page/0`. Pages contain at most 128 records: command family, byte counts, elapsed time and transport result. No request/response payload, memory address, USB path or serial is exported. Traces are not replayable. Only the latest trace is kept, in process memory; a new trace replaces a stopped/expired capture. Closing the process discards reports and traces. Bootstrap itself is not captured because tracing starts after session open.

## Offline development and native GUI checks

```sh
cargo test --locked --features mcp-mock --bin ak-configurator -- --test-threads=1
cargo build --locked --features mcp-mock
python3 scripts/test_mcp_stdio.py --binary target/debug/ak-configurator
dbus-run-session -- xvfb-run -a env AK820_RUNTIME_SMOKE_TRAY_FIXTURE=1 \
  python3 scripts/test_ak820_runtime_smoke.py \
  --binary target/debug/ak-configurator --report target/runtime-smoke-linux.json
```

On Windows use `python` and the `.exe` path. On Windows and macOS run the runtime script directly without `dbus-run-session` or `xvfb-run`. `mcp-mock` is development-only and must stay out of shipped packages. Its strict FD01 scripts exercise parser/error handling and are labeled `host_model`; they do not simulate a successful complete packed CODE bootstrap. C04 adds the eighth tool, `ak820_prepare`. The current full native Windows mock-feature suite passed 2,015 tests with 18 ignored; three relevant ignored C04 fixtures passed separately, and the remaining ignored cases are unqualified. Focused `mcp_prepare`, strict Clippy/format, mock/production builds, combined STDIO and SDK/Inspector checks passed. Reviewer `c04_final_review` verified the 103-file inventory and source/patch/report/binary hashes, closed four documentation findings and accepted the limited offline Windows C04 scope. The inventory is sparse; this acceptance does not establish source-to-binary linkage, three-platform/GUI/device qualification. See [C04 evidence](session-evidence/C04.md). The transport guard rejects malformed, oversized and duplicate-key JSON lines before RMCP parsing; complete in-limit frames accepted by the guard are forwarded byte-for-byte. Its inclusive line limit is 65,536 bytes, not a peak-memory guarantee. See [C04 input guard evidence](session-evidence/C04-INPUT-GUARD.md). This is a local protocol client, not third-party certification.

For the separate development-only official SDK and pinned Inspector check, prepare the npm cache and run `npm ci --offline --ignore-scripts --prefix scripts/mcp-interop`, then `node scripts/mcp-interop/check.mjs --binary "$CARGO_TARGET_DIR/debug/ak-configurator"` against an `mcp-mock` build. The check uses only the synthetic fixture and records the negotiated protocol; it does not qualify an exact firmware image or physical device. The package lock records the client dependencies; they are not application dependencies. See the C00 handoff for actual results and pending checks.

`--runtime-smoke` launches the actual native application renderer with an offline model and fresh temporary preferences. It checks window launch, RGB/application-settings navigation and continuing frames. The report also records process-to-window, splash, editor initialization and first-editor-frame timings; the harness and release gate reject missing or incorrectly ordered startup evidence. These are frame-submission timings, not an OS display-fence measurement. All three OS families create the production tray, hide/restore the native window and verify native visibility/rendering. On Linux the isolated fixture acts as a synthetic StatusNotifier host, checks the exported icon/menu, invokes production D-Bus activation and then disappears to check automatic window recovery. It does not render a desktop panel. A real Linux desktop needs a compatible StatusNotifier host; absent/lost hosts keep or restore the visible window. It does not exercise a physical tray mouse click, live OS helper scheduling, HID latency, or the firmware. Linux Xvfb is X11 evidence, not Wayland qualification. `--demo-ak820` uses the same isolation policy. Both modes reject mixed startup modes and block production hardware, autostart, capture, updates and native integration entry points. This is application-level isolation, not an OS sandbox or a claim that every inherited/ignored test is safe on a personal account.

CI runs GUI smoke on all six Linux/Windows/macOS candidates and binds its report to the packaged executable SHA256. The release verifier requires those reports. Configuring this gate does not mean the new Windows/macOS executables have already run; check the actual reports in [REVIEW.md](REVIEW.md).

## Work still open

Broader live ownership/helper cancellation and failure qualification, report/trace coverage, native GUI/device execution, and physical latency/LED checks remain open. The combined Windows STDIO mock/production check passed for C03; it does not qualify GUI or device behavior. Settings loading/editing, reversible functional suites, OS indicator control/observation and protected updates remain later plan work. C03 adds verified backup capture with metadata and a private artifact ID; its production-feature build and combined production STDIO passed, with its limited offline scope independently accepted; device behavior and source-to-binary linkage remain unqualified. The artifact store reports `DirectoryFlushUnsupported` on Windows. The MCP process does not control an already-running GUI. UI lifecycle checks use the separate native smoke runner. The current preview is useful for developing agent diagnostics and repeatable host failures, while keeping these remaining acceptance items visible.
