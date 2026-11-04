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

The x86_64 static-CRT executable passed a local build, self-check, offline GUI smoke and metadata-only MCP checks. PE inspection found 23 unique OS imports and no ordinary `VCRUNTIME140.dll` or UCRT imports. This does not prove dynamic or transitive dependency availability, compatibility with every Windows version, or launch on a clean PC. ARM64 static-CRT CI build, signing and packaging remain pending.

The Cargo executable is not a complete distribution. Packaging also prepares the native CAVA helper and the pinned public firmware inputs. The current Windows packaging path is documented in the [desktop CI workflow](.github/workflows/desktop-dev.yml) and [candidate build script](scripts/build_windows_ci_candidate.py). Final package installation and local certificate trust remain unverified; the developer certificate is not a public CA.

The inherited Entropy `Taskfile` and Linux packaging scripts expect the upstream `entropy` binary. They are retained as upstream project history and are not qualified AK Configurator Linux build instructions. Linux qualification is pending. Native macOS acceptance is outside this alpha's approved scope.

See [Installation](docs/INSTALL.md) for candidate package status and user launch instructions. No physical keyboard operation is needed to build the application.
