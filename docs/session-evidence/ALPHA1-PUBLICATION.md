# Alpha.1 publication evidence

**Published:** 2026-11-04 12:16:16 UTC, release 433846095, prerelease `v0.1.0-alpha.1`.

Tag `v0.1.0-alpha.1` is lightweight and points to `71abd382cbcefa554c87639635536e5f526be226` on `main`. The Rust application sources, Cargo inputs, build script, assets, installer definitions, i18n and vendored inputs matched checkpoint `152d02d`. Workflow files, the package workflow test and documentation changed afterward. No source-to-binary attestation or binary code-equivalence claim is made.

CI run [41818771438](https://github.com/dvino/ak-configurator/actions/runs/41818771438) completed successfully: six native jobs, two Windows signing preflights, and aggregate verification job 126549766798 (32 tests). Windows x86_64 and ARM64 used Rust 1.94.1; each reported 1,978 passed and 10 ignored native tests, plus 34 passed and 6 ignored MCP tests. Windows GUI runtime smoke passed; installer build/exercise and offline self-check passed. The legacy Build and Nix workflow jobs were skipped by upstream guards. See the recorded CI and test-summary JSON for exact job steps and results.

The workflow's Windows test commands were `cargo test --locked --all-targets -- --test-threads=1` and `cargo test --locked --features mcp-mock --bin ak-configurator 'mcp::' -- --test-threads=1`; it then ran `python scripts/test_mcp_stdio.py --binary target/debug/ak-configurator.exe`, the Windows GUI smoke harness, and the offline self-check. Aggregate verification ran `python3 scripts/test_package_line_endings.py`, `python3 scripts/test_windows_runtime_dependencies.py`, and `python3 scripts/test_ak820_release_workflow.py`; counts were 1, 4, and 32 passed respectively. Workflow source is pinned in the CI JSON. These command/test results describe CI; they are not a local rerun or physical-device test.

The release has 37 assets: 34 candidates, `release-manifest.json`, `app-update-index.json`, and `SHA256SUMS`. Server sizes and SHA256 values matched local publication evidence. Independent artifact review passed, including 34 raw-to-flat asset comparisons and aggregate manifest/index verification. This does not attest that the binaries were compiled from the tagged source.

The public Windows app hashes are x86_64 `b1ceb499c244bd71ac30971f00827bf5d73ebda4ea4bdc6f5b29ca59590bdcee` (55,023,288 bytes) and ARM64 `502d468311c9b5125f3c4776a55142a2f1d20460849e1416c3c94aa47ac40124` (51,303,096 bytes). The user's installed copy remains the separately frozen build 65; public commit 71 was not installed. Current-user Root and TrustedPublisher trust remain false, and Windows reports `UnknownError` for installed signatures. No HID/device write occurred.

This was an owner-authorized experimental preview, not B12 acceptance. Linux assets are experimental; ARM interactive and clean-PC use are unqualified; native macOS acceptance is excluded. Independent issue audit items #2–#7 remain open pending live acceptance. Native macOS acceptance, physical keyboard behavior, clean-PC launch, installed public-71 behavior, and source-to-binary provenance are not established. The exact firmware beta.5 release-input recheck is included as separate read-only evidence.

## Retained evidence

- `ALPHA1-PUBLICATION-RECEIPT.json`: release identity, all asset names, byte counts and public digests.
- `ALPHA1-CI-41818771438.json`: sanitized CI job and step summary.
- `ALPHA1-CI71-TEST-SUMMARY.json`: Windows and aggregate test counts and Rust version.
- `ALPHA1-FIRMWARE-RECHECK.json`: public beta.5 release-input identity recheck; no device or firmware build.
- `ALPHA1-INDEPENDENT-REVIEW.json` and `ALPHA1-BYTE-REVIEW.json`: sanitized independent artifact-review findings.

No private backup, source ROM, user data, or local filesystem path is included in these reports.
