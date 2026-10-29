Русский перевод: [Установка и разработка](ru/INSTALL.md).

# Installation and development

AK Configurator `0.1.0-alpha.1` is an unpublished alpha candidate. Current source, Linux execution checks and remaining platform limits are recorded in [Status](STATUS.md) and [Review](REVIEW.md). The workflow prepared six unsigned native packages. All six passed final CI runtime and install checks; signing and publication remain open. Historical Windows/macOS or cross-compilation results do not qualify this source. The bundled firmware inputs match the published AK820 beta.3 release. Browser support follows the native desktop milestone.

## Build from source

Use Rust 1.97 and the platform build dependencies. On Ubuntu 24.04, install the Linux dependencies used by `desktop-dev`:

```sh
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential pkg-config libdbus-1-dev libgtk-3-dev libudev-dev libpulse-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libxkbcommon-x11-0 libgl1-mesa-dev zenity
```

Linux Save/Open dialogs use an available XDG desktop portal or the `zenity` fallback. Minimal desktop/container environments may provide neither; install one before using file dialogs.

From the repository root, build and test serially:

```sh
cargo build --release --locked
python3 scripts/build_cava_helper.py --output-dir target/release/audio
cargo test --locked --all-targets -- --test-threads=1
```

The serial test setting avoids shared-resource HID process-test flakes. An offline UI preview is available with:

```sh
cargo run --locked -- --demo-ak820
```

The demo does not discover or open HID devices. Its examples are not device backups. The executable is `target/release/ak-configurator` on Linux/macOS and `target/release/ak-configurator.exe` on Windows. A build or host test does not prove physical-keyboard behavior.

Current source build evidence is for Ubuntu 24.04 with Rust 1.97.0. Older Debian 12 runtime success belongs to a prior artifact and does not establish compatibility for the current build. Alpine/musl is not qualified. Linux builds require desktop runtime libraries and a portal or Zenity for file dialogs.

### Bundled music component

Full candidate packages include `audio/cava` (`audio/cava.exe` on Windows) beside the application; the macOS app contains it under `Contents/MacOS/audio`. Keep that directory with the executable. Per-user installers copy it automatically. Separate OpenRGB or CAVA installation is not required for music. The package also carries exact third-party source archives and notices; retain them when redistributing.

The source build command above builds pinned CAVA and its static FFTW/iniparser inputs. Linux additionally needs the PulseAudio client runtime (`libpulse0`) and a running PulseAudio or PipeWire-Pulse service. For `cargo run` development, use `--output-dir target/debug/audio` instead. Windows helper builds use CMake/MSVC and the pinned vcpkg manifest with static dependencies; set `VCPKG_ROOT` to that checkout. macOS needs Xcode command-line tools and a 14.2+ SDK for the audio helper. Native system-audio capture on macOS requires 14.2+ and user consent; non-audio application features retain their separate requirements. See [THIRD-PARTY-AUDIO.md](THIRD-PARTY-AUDIO.md).

## Linux device access

If Linux denies HID access, install the AK820 udev rule explicitly from the repository root:

```sh
sh linux/udev/install-vial-rules.sh
```

The script requests elevation through `pkexec` or `sudo` and writes `/etc/udev/rules.d/59-ak820.rules`. The rule is limited to the AK820 USB vendor/product IDs. This optional system change is not performed by building or running the demo.

## Supported device and packages

The beta.3 source contract targets ten exact v0.6.0-beta.3 packed profile images: Main, Game, Mmo, Typing, Code, Pointer, Onehand, Creative, Shortcuts and Multios. The application verifies packed CODE/settings identity and required settings before enabling edits. Unknown, custom, trial and mismatched images are rejected. See the [desktop guide](DESKTOP.md). Firmware files currently offers local byte checks only; it does not flash a device. The bundled beta.3 catalog has `flashing_authorized=false` and no authorized image bindings, so beta.3 installation is currently blocked even with prerelease opt-in and exact identity checks. Beta.4.1 is selected by default for flashing its exact beta.4 images, bound to the pinned release catalog, firmware matrix and packed-CODE hashes; beta.3 remains an alternate. Beta.4.1 does not claim hardware validation and does not authorize saved-settings writes or restore. GitHub API digests for the beta.3 release catalog, firmware bundle, five JSON assets and `SHA256SUMS` were verified; all ten IHX/settings identities match reviewed candidates, map hashes match the release matrix, and approved metadata pins all ten release images to the exact installer release. This confirms release-input identity, not physical device behavior. Installation binds the loader transition and reconnect to the USB port, saves and rereads durable safety data, makes one attempt, and checks the full ROM with separate firmware/settings outcomes. A fresh install-only/OEM owner without loaded project settings may skip unsupported settings capture, but must pass exact full-ROM and safety checks. Once settings capture has been attempted, any capture failure stops preparation, including when defaults or rollback were selected. An exactly classified project source can supply the checked recovery bytes for an admitted update without an older personal OEM reference; this does not create or replace a personal OEM recovery archive. Supplied personal OEM references must be supported and valid; an invalid reference never falls back. Required safety data and recorded recovery-manifest trust are synced/reread before erase; failure stops and retains saved paths. These are source workflows: no physical flash, retention or OEM rollback qualification is claimed.

The `desktop-dev` workflow defines raw development binaries for Linux and Windows x86_64/arm64 and macOS x86_64/arm64. Each artifact contains a binary and `SHA256SUMS`; they are unsigned development binaries, not installers. The final native workflow completed for application source `b3ecae1`; do not treat historical artifacts as current alpha.1 packages. The known successful six-target run at commit `1afd0f1` is historical and is not evidence for the current alpha.1 source.

Choose the matching artifact name: `development-binary-linux-x86_64`, `development-binary-linux-arm64`, `development-binary-windows-x86_64`, `development-binary-windows-arm64`, `development-binary-macos-arm64`, or `development-binary-macos-x86_64`. Download its ZIP from a successful current run and extract it. Verify the executable against `SHA256SUMS` before launch. On Linux run `sha256sum -c SHA256SUMS`, then `chmod +x ak-configurator && ./ak-configurator`. On macOS run `shasum -a 256 -c SHA256SUMS`, then `chmod +x ak-configurator` and `./ak-configurator`. In Windows PowerShell compare `(Get-FileHash .\ak-configurator.exe -Algorithm SHA256).Hash.ToLowerInvariant()` with the line in `SHA256SUMS`, then launch `Start-Process .\ak-configurator.exe`.

## Desktop candidate packages

The `desktop-dev` run also uploads a separate `desktop-candidate-${platform}` artifact for each of the six platform IDs above. Linux candidates are `.tar.gz` archives, macOS candidates are `.app.zip` application bundles, and Windows candidates are portable `.zip` archives. Each archive has a neighboring `.sha256` file and contains the localized install/RGB/Desktop/Changelog/Status/Review guides, application/font/icon/eframe licenses, firmware provenance, exact beta.3 offline catalog and public profile IHX/map assets, source/binary manifest, and an internal `SHA256SUMS`. Help text is also built into the executable. These are unsigned trial packages with explicit per-user installation and removal scripts. They do not provide MSI/DEB/PKG packages or signed/notarized distribution.

The optional local packager uses only Python 3.11+ standard-library modules:

```sh
mkdir -p target/candidate
target/release/ak-configurator --self-check --output target/candidate/runtime-self-check.json
python3 scripts/package_ak820_candidate.py \
  --binary target/release/ak-configurator \
  --cava-helper target/release/audio/cava \
  --platform linux-x86_64 \
  --runtime-check target/candidate/runtime-self-check.json \
  --output-dir target/candidate
```

Select the binary path and `--platform` value matching the target: `linux-x86_64`, `linux-arm64`, `windows-x86_64`, `windows-arm64`, `macos-x86_64`, or `macos-arm64`. The packager checks executable format/architecture and verifies bundled public firmware assets against the release manifest. A clean Git source tree is required by default. `--allow-dirty` is for a clearly labeled local trial only; the manifest records the dirty-source state. Verify the adjacent archive checksum before extraction (`sha256sum -c <archive>.sha256` on Linux; `shasum -a 256 -c <archive>.sha256` on macOS). Then check the internal `SHA256SUMS` before launch. Linux: extract the `.tar.gz`, make `ak-configurator` executable if needed, and launch it. macOS: extract the `.app.zip` and open `AK-Configurator.app`; unsigned builds may be blocked by Gatekeeper and are not notarized. Windows: extract the ZIP and run `ak-configurator.exe` for portable use, or use the per-user installer below. Candidate packages are unsigned trials; architecture and hash checks do not qualify platform behavior or physical keyboard behavior.

### Extract, verify and launch a candidate

These examples start with an archive and its adjacent `.sha256` already downloaded from a successful current workflow or produced by the local packager. Use the exact filename for the selected OS/CPU; no published candidate download is implied. Stop if either checksum check reports a mismatch.

Linux x86_64:

```sh
set -e
archive=ak-configurator-0.1.0-alpha.1-linux-x86_64.tar.gz
sha256sum -c "${archive}.sha256"
tar -xzf "$archive"
cd "${archive%.tar.gz}"
sha256sum -c SHA256SUMS
./ak-configurator
```

macOS arm64:

```sh
set -e
archive=ak-configurator-0.1.0-alpha.1-macos-arm64.app.zip
shasum -a 256 -c "${archive}.sha256"
ditto -x -k "$archive" .
cd "${archive%.app.zip}"
shasum -a 256 -c SHA256SUMS
open AK-Configurator.app
```

Windows x86_64, PowerShell:

```powershell
$ErrorActionPreference = 'Stop'
$archive = 'ak-configurator-0.1.0-alpha.1-windows-x86_64.zip'
$expectedHash = ((Get-Content -LiteralPath "$archive.sha256") -split '\s+', 2)[0]
$actualHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualHash -ne $expectedHash) { throw 'Archive SHA256 mismatch' }
Expand-Archive -LiteralPath $archive -DestinationPath .
Set-Location ($archive -replace '\.zip$', '')
Get-Content -LiteralPath SHA256SUMS | ForEach-Object {
    $expectedHash, $relativeFile = $_ -split '  ', 2
    $actualHash = (Get-FileHash -LiteralPath $relativeFile -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) { throw "SHA256 mismatch: $relativeFile" }
}
Start-Process .\ak-configurator.exe
```

### Install, update and remove for the current user

After verifying and extracting the package, use **Quit** to exit the application and helper completely. Run the matching command from the extracted package directory:

| Platform | Install or update | Installed location |
| --- | --- | --- |
| Linux | `sh ./install-user.sh` | `${XDG_DATA_HOME:-$HOME/.local/share}/ak-configurator/current`, stable launcher in `${XDG_BIN_HOME:-$HOME/.local/bin}` and an application-menu entry |
| macOS | `sh ./install-user.sh` | `~/Applications/AK-Configurator.app` |
| Windows PowerShell | `powershell -NoProfile -File .\install-user.ps1` | `%LOCALAPPDATA%\Programs\AK Configurator`, with a Start menu shortcut |

The scripts verify the package before replacement, stage the new application and retain the previous installation through activation. They do not start the app, enable autostart, or touch the separate application preferences and keyboard backups. The Linux installed executable uses a stable `current` path so existing autostart registration continues to select the updated application. OS policies for unsigned applications/scripts still apply; these candidate scripts are not signed.

To remove the application, run `sh ./uninstall-user.sh` on Linux/macOS or `powershell -NoProfile -File .\uninstall-user.ps1` on Windows. Use the copy retained with the installation or the extracted package. Removal asks before deleting owned application files; application preferences, cache and separately stored backups remain. It does not disable an existing OS autostart registration: disable **Launch at startup** in the application before uninstalling.

### Download an application update

Open **About AK Configurator → Application updates**, then choose **Check for updates**. Stable releases are selected by default; enable **Include prereleases** deliberately to select an alpha or beta. **Download and verify** appears for a newer version only. The client requires the project's six-target index, valid GitHub asset digests and exact OS/architecture matches; a release without that metadata is unavailable through this path.

The earlier unpublished application build `0.1.0-beta.2` sorts above `0.1.0-alpha.1` under SemVer. To move from that development build to the first alpha, download and verify the alpha package manually and use the install/update procedure above. The updater deliberately does not offer it as a newer version.

After verification, copy the displayed archive path, quit the application/helper, extract it and run the matching per-user installer above. A cancelled or failed download is not offered for installation. No installer, firmware flash or keyboard command starts automatically. The cached archive filename includes its SHA256 prefix; the directory inside the archive retains the ordinary `ak-configurator-VERSION-PLATFORM` name. Existing helper preferences are preserved by the application installer.

### Offline executable check and release preparation

`./ak-configurator --self-check --output check.json` validates built-in public sources/defaults and emits a report without reading settings, creating a GUI, accessing a device or using the network. The output path must be new. On Windows use the `.exe`; inside the macOS bundle use `AK-Configurator.app/Contents/MacOS/ak-configurator`. This command is a package execution probe, not a GUI or hardware acceptance test.

The six native workflow jobs are configured to run their target binary, package its exact report and run platform-specific tests. Candidate verification checks complete archives and generates `app-update-index.json` alongside `release-manifest.json`. The final six-platform packaged CI pass completed for source `b3ecae1`; candidate publication has not been completed. A later published application release must include the exact six archives and `app-update-index.json` under the matching version tag; GitHub must report their SHA256 digests. The index is generated only after aggregate verification. Signing/notarization and physical acceptance remain separate release gates.

### Run the OS layout helper

First enable **OS layout indicator** and save the EN/RU mappings in the application. The helper is opt-in; no HID indication starts merely from installing or extracting the package. Explicit background launch from the extracted directory:

| Platform | Command |
| --- | --- |
| Linux | `./ak-configurator --ak820-helper` |
| macOS application bundle | `./AK-Configurator.app/Contents/MacOS/ak-configurator --ak820-helper` |
| Windows PowerShell | `.\ak-configurator.exe --ak820-helper` |

Opted-in `--minimized`/autostart can also enter the helper. **Close interface, keep helper**, or ordinary Close with the helper enabled, releases the GUI and retains the small loop; opening the application again recreates the interface in the same process. Explicit Quit exits. Unknown layouts release indication; device errors stop refresh without blind retry. The helper checks exact firmware and yields device ownership to settings/backup/restore/installation. Physical layout/LED timing, GUI handoff and platform resource behavior need separate qualification.
