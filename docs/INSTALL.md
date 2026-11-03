Русский перевод: [Установка](ru/INSTALL.md).

# Install and run

AK Configurator `0.1.0-alpha.1` is being prepared for Windows x86_64 and ARM64. The release is unpublished and there is no public download. The native per-user GUI installer is the primary installation path; the portable ZIP is the alternative. The x86_64 executable passed isolated GUI and metadata-only MCP checks. An unsigned native GUI setup passed isolated install, same-version reinstall and uninstall checks. The final signed setup and package are not ready. ARM64 packaging and CI are configured, but ARM64 execution is unverified. Do not use an earlier platform artifact as this candidate.

## Candidate package

The intended local x86_64 portable archive is `ak-configurator-0.1.0-alpha.1-windows-x86_64.zip`; it is not a GitHub release. The final signed setup and archive, their checksums and download are still pending. The currently built unsigned `ak-configurator-0.1.0-alpha.1-windows-x86_64-setup.exe` is an isolated test artifact, not the release installer. The planned ARM64 setup name is `ak-configurator-0.1.0-alpha.1-windows-arm64-setup.exe`; its execution is unverified. The x86_64 setup smoke does not cover wizard appearance, previous-version upgrade, real settings preservation or installed MCP. When final artifacts are handed off, verify the external SHA256 and the archive's internal `SHA256SUMS` before extraction or launch. If a checksum file is missing or a value differs, stop.

For the final signed x86_64 installer, start `ak-configurator-0.1.0-alpha.1-windows-x86_64-setup.exe` from Explorer and follow its per-user setup wizard. It does not require a PowerShell command. The isolated setup test covered installation, app self-check, reinstalling the same setup, uninstall, Start Menu and Apps/Uninstall registration, and sentinel verification. It did not test a previous-version upgrade.

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

After those checks pass, start the portable app with `Start-Process .\ak-configurator.exe`. This fallback archive has no final checksum or installation result yet. The host's PowerShell policy reports `LocalMachine=AllSigned`; other scopes are undefined. No policy change or signing-check bypass was made. The native Inno Setup path avoids PowerShell; the optional legacy `.ps1` installer may be blocked by the current policy. Running the app requires no Python installation or extra Python package; repository packaging scripts use Python's standard library.

## Build from source

Use a Windows x86_64 Rust toolchain with the MSVC C++ build tools and Windows SDK. From the repository root:

```powershell
cargo build --release --locked --features mcp
.\target\release\ak-configurator.exe --self-check --output .\target\release\self-check.json
```

The optimized MCP-enabled executable passed its offline self-check for the Beta5 catalog and ten profiles. It also passed native GUI smoke in a fresh isolated configuration with live HID forbidden, and production MCP STDIO/SDK checks that requested no hardware tools. These results do not include an installed archive. The self-check itself does not open the GUI, enumerate HID devices, access the network or perform a device operation. The standalone executable is not the complete candidate package; the package also needs its verified public firmware inputs and other declared files.

## Current limits

This milestone covers Windows x86_64 and ARM64 packaging. Linux and macOS are outside the milestone; the six-target application requirement remains open. The x86_64 executable has runtime checks; actual ARM64 execution, final signing, publication and final-package installation remain open. Candidate builds include the read-only MCP feature; its server starts only with the explicit MCP arguments. A plain Cargo build omits the feature. MCP can report metadata and create a local installation preview, but has no settings-write, restore-execute or firmware-flash operation. The app does not qualify the keyboard's physical behavior. Do not unlock, reset, enter the loader or write to a keyboard as part of installation testing.

See [Status](STATUS.md) for current evidence and [Review](REVIEW.md) for the release gates.
