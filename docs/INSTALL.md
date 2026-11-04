Русский перевод: [Установка](ru/INSTALL.md).

# Install and run

AK Configurator `0.1.0-alpha.1` targets Windows x86_64 and ARM64. Download the per-user GUI installer or portable ZIP from the [GitHub release](https://github.com/dvino/ak-configurator/releases/tag/v0.1.0-alpha.1). The GUI installer is the primary path; the ZIP is the alternative. Windows packages use a local development signing certificate, not a public-CA identity. Windows may display publisher or trust warnings depending on the computer; the release makes no system-wide trust or SmartScreen claim. Non-Windows packages, if listed, are experimental and unqualified. Native macOS acceptance is outside this alpha's scope. See [Status](STATUS.md) for validation and known limits.

## Windows package

Choose the files for the Windows architecture from the release assets:

| Architecture | GUI installer | Portable archive |
|---|---|---|
| x86_64 | `ak-configurator-0.1.0-alpha.1-windows-x86_64-setup.exe` | `ak-configurator-0.1.0-alpha.1-windows-x86_64.zip` |
| ARM64 | `ak-configurator-0.1.0-alpha.1-windows-arm64-setup.exe` | `ak-configurator-0.1.0-alpha.1-windows-arm64.zip` |

Download the matching `.sha256` sidecar and verify the file's SHA256 before extraction or launch. For a ZIP, extract it only after the external check passes, then verify the archive's internal `SHA256SUMS` before launching any file. If a checksum is missing or differs, stop. The examples below use x86_64; use the corresponding ARM64 filenames for ARM64.

Start the matching `*-setup.exe` from Explorer and follow its per-user setup wizard. The ARM64 package contains a native ARM64 app and audio helper; its Inno Setup bootstrap uses x64 emulation on Windows 11 ARM. Whether Windows trusts the local development certificate is an operating-system decision. See [Status](STATUS.md) for the checked installer behavior and remaining limits.

PowerShell verification and extraction:

```powershell
$ErrorActionPreference = 'Stop'
$archive = 'ak-configurator-0.1.0-alpha.1-windows-x86_64.zip'
$expected = ((Get-Content -LiteralPath "$archive.sha256") -split '\s+', 2)[0].ToLowerInvariant()
$hashOutput = certutil -hashfile $archive SHA256
$actual = ($hashOutput | Where-Object { $_ -match '^[0-9a-fA-F]{64}$' } | Select-Object -First 1).ToLowerInvariant()
if ($actual -ne $expected) { throw 'Archive SHA256 mismatch' }
Expand-Archive -LiteralPath $archive -DestinationPath .
Set-Location ($archive -replace '\.zip$', '')
Get-Content -LiteralPath SHA256SUMS | ForEach-Object {
    $expectedHash, $relativeFile = $_ -split '  ', 2
    $hashOutput = certutil -hashfile $relativeFile SHA256
    $actualHash = ($hashOutput | Where-Object { $_ -match '^[0-9a-fA-F]{64}$' } | Select-Object -First 1).ToLowerInvariant()
    if ($actualHash -ne $expectedHash.ToLowerInvariant()) { throw "SHA256 mismatch: $relativeFile" }
}
```

After those checks pass, start the portable app with `Start-Process .\ak-configurator.exe`. The optional legacy `.ps1` installer follows the PowerShell execution policy on that PC. The GUI installer runs natively. Running the app requires no Python installation; repository packaging scripts use Python 3.11 or newer and its standard library. See [Status](STATUS.md) for validation results.

## Build from source

Use a Windows x86_64 Rust toolchain with the MSVC C++ build tools and Windows SDK. From the repository root:

```powershell
cargo build --release --locked --features mcp
.\target\release\ak-configurator.exe --self-check
```

The self-check itself does not open the GUI, enumerate HID devices, access the network or perform a device operation. The standalone executable is not the complete candidate package; the package also needs its verified public firmware inputs and other declared files.

## Current limits

This alpha covers Windows x86_64 and ARM64 packaging. Linux qualification is pending; macOS native acceptance is outside the approved scope. Neither platform is qualified. Validation of interactive ARM64 use and installer behavior is recorded in [Status](STATUS.md). Candidate builds include the read-only MCP feature; its server starts only with the explicit MCP arguments. A plain Cargo build omits the feature. MCP can report metadata and create a local installation preview, but has no settings-write, restore-execute or firmware-flash operation. The app does not qualify the keyboard's physical behavior. No physical keyboard operation was performed. Do not unlock, reset, enter the loader or write to a keyboard as part of installation testing.

See [Status](STATUS.md) for current evidence and [Review](REVIEW.md) for the release gates.
