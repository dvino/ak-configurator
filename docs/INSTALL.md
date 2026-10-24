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
  libxkbcommon-dev libgl1-mesa-dev
```

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

The `desktop-dev` workflow defines raw development-binary builds for Linux
x86_64, Windows x86_64, and macOS arm64/x86_64. A workflow definition does not
mean its CI builds have passed. Release installers and release checks remain
pending. Firmware flashing, OEM recovery and the OS LED helper are not included
in this alpha.
