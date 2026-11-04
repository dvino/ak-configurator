# Build from source

Russian: [Сборка из исходников](docs/ru/BUILD.md).

## Windows development build

Use a native Windows MSVC toolchain for x86_64 or ARM64 with Rust 1.94.1, the matching MSVC C++ build tools and Windows SDK. The offline Python checks require Python 3.11 or newer. From the repository root:

```powershell
cargo build --locked --release --features mcp
.\target\release\ak-configurator.exe --self-check
python scripts/test_mcp_production_metadata.py --binary .\target\release\ak-configurator.exe
```

For concurrent builds, set a unique `CARGO_TARGET_DIR` for each build so outputs are not shared; adjust the executable paths above to that directory. The checked-in [Cargo configuration](.cargo/config.toml) enables the static MSVC CRT for x64 and ARM64 builds. Do not override it with `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS` or target-specific `CARGO_TARGET_*_RUSTFLAGS` when preparing packages.

Frozen software build `65e1a90041c434b98401203bb160e8986c4f0a46` passed CI run [41794559734](https://github.com/dvino/ak-configurator/actions/runs/41794559734): both signing preflights, all six native jobs, aggregate package verification including EOL, four PE parser checks and full offline package-contract tests. Windows x86_64 and ARM64 packages use the static CRT; each packaged app has 23 OS imports and its helper has three. Neither app directly imports VCRUNTIME or UCRT. This does not prove dynamic or transitive dependency availability, compatibility with every Windows version, or launch on a clean PC.

The Cargo executable is not a complete distribution. Packaging also prepares the native CAVA helper and the pinned public firmware inputs. The current Windows packaging path is documented in the [desktop CI workflow](.github/workflows/desktop-dev.yml) and [candidate build script](scripts/build_windows_ci_candidate.py). Both architectures were packaged, signed and independently reviewed. Local installation and installed MCP checks passed. The locally issued signing certificate is pinned to thumbprint 1D9596F961B7F24D9F66AE7BE2B1EA39FF1543C7, not a public CA. Current-user Root and TrustedPublisher trust is still absent; Windows signature status remains UnknownError for the installed app, helpers, PowerShell scripts and uninstaller. A manual OS confirmation remains pending.

The inherited Entropy `Taskfile` and Linux packaging scripts expect the upstream `entropy` binary. They are retained as upstream project history and are not qualified AK Configurator Linux build instructions. Linux qualification is pending. Native macOS acceptance is outside this alpha's approved scope.

See [Installation](docs/INSTALL.md) for candidate package status and user launch instructions. No physical keyboard operation is needed to build the application.
