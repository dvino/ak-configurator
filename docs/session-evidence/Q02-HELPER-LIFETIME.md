# Q02 — Windows helper lifetime

Russian: [RU](../ru/session-evidence/Q02-HELPER-LIFETIME.md).

## Current evidence

**Helper-lifetime correction is source-reviewed and has focused native-host test evidence; Q02 as a whole remains pending B12/C04 integration.** The Windows proxy now creates an unnamed, non-inheritable Job Object with `KILL_ON_JOB_CLOSE` and stores it in the existing slot. The child must be assigned before it can complete the stdin startup barrier, so it cannot open HID before assignment. Assignment failure is fail-closed. Nested-job restrictions remain a compatibility limit.

Reviewer `helper_lifetime_review` identified a PID-reuse risk in the test fixture before any parent-death probe ran. The fixture now duplicates the actual child process handle from the retained live parent. The reviewer closed that finding; no process IDs are recorded.

| Check | Result |
|---|---|
| Startup barrier: `rtk cargo test --locked --bin ak-configurator hid::hid_proxy::tests::windows_helper_start_barrier_requires_complete_token -- --exact --nocapture` | 1 passed, 1961 filtered, 0.00 s, exit 0 |
| Parent-death fake-child test on the current test binary | 1 passed, 1961 filtered, 0.08 s, exit 0 |
| Full `hid::hid_proxy::tests` group on the same binary | 32 passed, 3 ignored, 1927 filtered, 7.96 s, exit 0; includes blocked read/write timeout and reaper tests with the parent alive |
| Marker-based parent termination | A fake child accepts the startup marker, then waits unconditionally for 60 seconds. Both normal parent `process::exit` and parent termination caused child exit within 5 seconds, confirmed by waiting on a retained process handle |
| Build/test environment | Native Windows/MSVC; isolated `CARGO_TARGET_DIR=D:\codex-ak-configurator-c02-target`; DEV/TEST debug info disabled, incremental compilation disabled, 2 Cargo jobs, `RUSTFLAGS=-C debuginfo=0` |
| Tested source and executable | Test snapshot `src/hid_proxy.rs` SHA256 `66f04c275f53660add0b822f5ee67262c1112800b42aebf24636b6813c82e9f2`; test executable 99,862,528 bytes, SHA256 `67bef0ac18a014cacc091afa5792b7838499aace65920d27997fd43d76a71d3d` |
| Formatting | Initial `rustfmt --edition 2021 --check src/hid_proxy.rs` found a formatting diff. `rustfmt --edition 2021 src/hid_proxy.rs` was applied and the per-file check then passed (exit 0). Current formatted file SHA256 `70a11ad321e63faad8e2cda04f840dab835bbd322e481738efb49826d29c995d`; the tests above ran on the pre-format source snapshot |
| Integrated formatting and strict lints | After the integrated snapshot reached the shared 71-file B08-C03 source checkpoint, whole-project `cargo fmt --check` and strict all-target `LINT-N`/`LINT-C` passed (exit 0). The helper production code was unchanged. The helper-specific tests above remain historical and were not rerun on this snapshot |

An earlier exact-filter command used a stale executable and matched zero tests (1958 filtered); it is not a pass. The corrected direct run against the current test binary is the one counted above. The earlier bounded 250 ms no-HID probe is retained as historical only: it did not prove the child entered blocked I/O.

These tests exercise a deliberately delayed fake child and process-handle cleanup. They do not prove termination during a blocked HID syscall or ordinary GUI exit, and they do not qualify native window behavior, physical device behavior, macOS, or the complete Q02 platform/artifact card. No HID packet or device interaction was performed. Q02 remains pending B12/C04 integration and its remaining platform checks. No user-specific paths or process IDs are included.

The tested source snapshot and executable are retained outside the repository in the private handoff checkpoint. The later integrated-snapshot formatting and lint passes do not establish ordinary GUI exit, termination during blocked HID syscalls, Q02 completion, or device behavior.
