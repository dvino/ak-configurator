Русский перевод: [Установка и разработка](ru/INSTALL.md).

# Installation and development

AK Configurator v0.1.0-beta.2 is in release preparation. Current source checks and limits are recorded in [Status](STATUS.md) and [Review](REVIEW.md). The six-target workflow defines development binaries and a separate unsigned DRAFT candidate; no candidate publication has been executed. Windows GNU and macOS arm64 adapter typechecks are not full-app builds or runtime qualification. Browser support follows the native desktop milestone.

## Build from source

Use Rust 1.97 and the platform build dependencies. On Ubuntu 24.04, install the Linux dependencies used by `desktop-dev`:

```sh
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential pkg-config libdbus-1-dev libgtk-3-dev libudev-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libxkbcommon-x11-0 libgl1-mesa-dev zenity
```

Linux Save/Open dialogs use an available XDG desktop portal or the `zenity` fallback. Minimal desktop/container environments may provide neither; install one before using file dialogs.

From the repository root, build and test serially:

```sh
cargo build --release --locked
cargo test --locked --all-targets -- --test-threads=1
```

The serial test setting avoids shared-resource HID process-test flakes. An offline UI preview is available with:

```sh
cargo run --locked -- --demo-ak820
```

The demo does not discover or open HID devices. Its examples are not device backups. The executable is `target/release/ak-configurator` on Linux/macOS and `target/release/ak-configurator.exe` on Windows. A build or host test does not prove physical-keyboard behavior.

Current source build evidence is for Ubuntu 24.04 with Rust 1.97.0. Older Debian 12 runtime success belongs to a prior artifact and does not establish compatibility for the current build. Alpine/musl is not qualified. Linux builds require desktop runtime libraries and a portal or Zenity for file dialogs.

## Linux device access

If Linux denies HID access, install the AK820 udev rule explicitly from the repository root:

```sh
sh linux/udev/install-vial-rules.sh
```

The script requests elevation through `pkexec` or `sudo` and writes `/etc/udev/rules.d/59-ak820.rules`. The rule is limited to the AK820 USB vendor/product IDs. This optional system change is not performed by building or running the demo.

## Supported device and packages

The beta2 source contract supports ten exact v0.6.0-beta.2 packed profile images: Main, Game, Mmo, Typing, Code, Pointer, Onehand, Creative, Shortcuts and Multios. The application verifies packed CODE/settings identity and required settings before enabling edits. Unknown, custom, trial and mismatched images are rejected. See the [desktop guide](DESKTOP.md). Firmware files currently offers local byte checks only; it does not flash a device. The installer still needs a live loader transition/reconnect flow, release/revocation authority, durable safety and full-ROM readback transaction, and separate settings-result handling.

The `desktop-dev` workflow defines raw development binaries for Linux and Windows x86_64/arm64 and macOS x86_64/arm64. Each artifact contains a binary and `SHA256SUMS`; they are unsigned development binaries, not installers. A successful workflow run for the current source is required before downloading its artifacts. The known successful six-target run at commit `1afd0f1` is historical and is not evidence for beta2 source.

Choose the matching artifact name: `development-binary-linux-x86_64`, `development-binary-linux-arm64`, `development-binary-windows-x86_64`, `development-binary-windows-arm64`, `development-binary-macos-arm64`, or `development-binary-macos-x86_64`. Download its ZIP from a successful current run and extract it. Verify the executable against `SHA256SUMS` before launch. On Linux run `sha256sum -c SHA256SUMS`, then `chmod +x ak-configurator && ./ak-configurator`. On macOS run `shasum -a 256 -c SHA256SUMS`, then `chmod +x ak-configurator` and `./ak-configurator`. In Windows PowerShell compare `(Get-FileHash .\ak-configurator.exe -Algorithm SHA256).Hash.ToLowerInvariant()` with the line in `SHA256SUMS`, then launch `Start-Process .\ak-configurator.exe`.

## Desktop candidate packages

The same `desktop-dev` run also uploads a separate `desktop-candidate-${platform}` artifact for each of the six platform IDs above. It contains `ak-configurator-0.1.0-beta.2-${platform}.zip` and its adjacent `.zip.sha256` file; the archive includes the platform executable, EN/RU installation and RGB guides, retained application/font/icon/eframe licenses, firmware provenance, a source/binary manifest and an internal `SHA256SUMS`. This candidate ZIP is separate from the existing raw development binary and checksum artifacts.

The optional local packager uses only Python 3.11+ standard-library modules:

```sh
python3 scripts/package_ak820_candidate.py \
  --binary target/release/ak-configurator \
  --platform linux-x86_64 \
  --output-dir target/candidate
```

Select the binary path and `--platform` value matching the target: `linux-x86_64`, `linux-arm64`, `windows-x86_64`, `windows-arm64`, `macos-x86_64`, or `macos-arm64`. It checks the native executable format and architecture, pinned firmware catalog files, source commit and application version. A clean Git source tree is required by default. `--allow-dirty` is for a clearly labeled local trial only; the manifest records the dirty-source state. Verify the outer checksum with `sha256sum -c ak-configurator-0.1.0-beta.2-${platform}.zip.sha256` on Linux or `shasum -a 256 -c ak-configurator-0.1.0-beta.2-${platform}.zip.sha256` on macOS, then check the archive's internal `SHA256SUMS` before launching. Candidate packages are unsigned trials; architecture and hash checks do not qualify platform behavior or physical keyboard behavior.
