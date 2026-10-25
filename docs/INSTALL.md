Русский перевод: [Установка и разработка](ru/INSTALL.md).

# Installation and development

The first milestone is the native desktop configurator. Current build and test
evidence is recorded in [Status](STATUS.md). Browser support follows the native
desktop milestone.

## Build from source

Install Rust 1.97 and the platform build dependencies. On Ubuntu 24.04, install
the native Linux dependencies used by the `desktop-dev` workflow:

```sh
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential pkg-config libdbus-1-dev libgtk-3-dev libudev-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libxkbcommon-x11-0 libgl1-mesa-dev zenity
```

On Linux, Save/Open dialogs use an available XDG desktop portal or the `zenity`
fallback. Minimal desktop/container environments may provide neither; install
`zenity` or the desktop portal implementation for the session before using
file dialogs.

From the repository root, build the release executable and run host tests with:

```sh
cargo build --release --locked
cargo test --locked --all-targets
```

For an offline UI preview with example AK820 data, run:

```sh
cargo run --locked -- --demo-ak820
```

The demo does not discover or open HID devices. Its example layout is not a
backup of settings from a physical keyboard. The release executable is
`target/release/ak-configurator` on Linux and macOS, or
`target/release/ak-configurator.exe` on Windows. Builds and host tests do not
prove behavior with a physical keyboard.


Linux CI binaries are built on Ubuntu 24.04. The checked x86_64 binary requires
glibc 2.39; its actual launch on Debian 12 was rejected for missing GLIBC_2.38/2.39. The separately
provided Debian 12 x86_64 trial build was built and launched there; select it
for that environment. Neither Linux build is a native Alpine/musl executable.
Both need the desktop runtime libraries and a portal or Zenity for file dialogs.

## Linux device access

If Linux denies access to the keyboard's HID interface, explicitly install the
AK820 udev rule from the repository root:

```sh
sh linux/udev/install-vial-rules.sh
```

The script requests elevation through `pkexec` or `sudo` when needed and writes
`/etc/udev/rules.d/59-ak820.rules`. The rule grants access only to the AK820
USB vendor/product IDs. This system change is optional and is not performed by
building or running the demo.

## Supported device and packages

This alpha accepts only a wired AK820 running the verified full permanent
v0.6.0-alpha.3 image. The verified ROM range is 0x2800–0xC700, SHA-256 is
`01f0f7a979d8cfa8217b3bba333758ae3509cf7f3f8e35fbffb1ed034b808e5d`, and the
manifest is at 0xEF80. Unknown, custom and trial images are rejected. Read the
[desktop guide](DESKTOP.md) before editing settings.

The `desktop-dev` workflow defines raw development binaries for Linux and
Windows x86_64/arm64, and macOS arm64/x86_64. After a successful run, its six
platform artifacts are available from the [desktop-dev Actions runs](https://github.com/dvino/ak-configurator/actions/workflows/desktop-dev.yml?query=branch%3Adesktop-dev);
each contains a binary and `SHA256SUMS`. These are unsigned development
binaries, not installers. Check that every platform job passed before relying
on its artifact. With `--demo-ak820`, the bundled beta.1 profiles open editable local example
projects with Save/Open/Undo. Outside demo mode the catalog remains a preview;
beta firmware settings writes and flashing stay disabled. The current application
still recognizes only the exact alpha.3 device image.
OEM recovery and the OS LED helper are not included in this desktop milestone.

Choose the artifact matching both your operating system and processor:
`development-binary-linux-x86_64`, `development-binary-linux-arm64`,
`development-binary-windows-x86_64`, `development-binary-windows-arm64`,
`development-binary-macos-arm64`, or `development-binary-macos-x86_64`.
Download that artifact ZIP from the successful workflow run and extract it.
Verify the extracted executable against the included `SHA256SUMS` before
launching it. On Linux, run `sha256sum -c SHA256SUMS`, then
`chmod +x ak-configurator && ./ak-configurator`. On macOS, run
`shasum -a 256 -c SHA256SUMS`, then `chmod +x ak-configurator` and
`./ak-configurator`. On Windows PowerShell, compare
`(Get-FileHash .\ak-configurator.exe -Algorithm SHA256).Hash.ToLowerInvariant()`
with the hash on the line in `SHA256SUMS`, then launch
`Start-Process .\ak-configurator.exe`.
