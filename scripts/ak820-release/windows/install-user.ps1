$ErrorActionPreference = 'Stop'
$source = Split-Path -Parent $MyInvocation.MyCommand.Path
$manifestPath = Join-Path $source 'manifest.json'
$sumsPath = Join-Path $source 'SHA256SUMS'
$exe = Join-Path $source 'ak-configurator.exe'
if (-not (Test-Path $exe) -or -not (Test-Path $manifestPath) -or -not (Test-Path $sumsPath)) { throw 'Run this from a complete extracted Windows bundle.' }
$manifest = Get-Content -Raw $manifestPath | ConvertFrom-Json
if ($manifest.platform -notlike 'windows-*' -or $manifest.format -ne 'ak-configurator-desktop-candidate-1' -or $manifest.binary_sha256 -ne (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Bundle manifest or executable digest is invalid for Windows.' }
$seen = @{}
if ((Get-Item $sumsPath).Length -eq 0) { throw 'Empty internal checksum manifest.' }
foreach ($line in Get-Content $sumsPath) {
  if ($line -notmatch '^([0-9a-f]{64})  ([A-Za-z0-9._/-]+)$') { throw 'Malformed internal checksum manifest.' }
  $expected, $relative = $Matches[1], $Matches[2]
  if ($relative.StartsWith('/') -or $relative.Contains('..') -or $seen.ContainsKey($relative)) { throw 'Unsafe or duplicate checksum path.' }
  $seen[$relative] = $true
  $path = Join-Path $source ($relative -replace '/', [IO.Path]::DirectorySeparatorChar)
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Bundle file missing: $relative" }
  $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actual -ne $expected) { throw "Bundle checksum mismatch: $relative" }
}
if (-not $seen.ContainsKey('manifest.json') -or -not $seen.ContainsKey('ak-configurator.exe')) { throw 'Checksum manifest does not cover the executable and candidate manifest.' }
if (Get-Process -Name 'ak-configurator' -ErrorAction SilentlyContinue) { throw 'Close AK Configurator before installing an update.' }
$programs = Join-Path $env:LOCALAPPDATA 'Programs'
New-Item -ItemType Directory -Path $programs -Force | Out-Null
$target = Join-Path $programs 'AK Configurator'
if (Test-Path $target) {
  if (-not (Test-Path (Join-Path $target '.ak-configurator-owned'))) { throw "Refusing to replace unmanaged installation: $target" }
}
$stage = Join-Path $programs ('.ak-configurator-stage-' + [Guid]::NewGuid().ToString('N'))
$backup = Join-Path $programs ('.ak-configurator-backup-' + [Guid]::NewGuid().ToString('N'))
$menu = Join-Path $env:APPDATA 'Microsoft/Windows/Start Menu/Programs'
New-Item -ItemType Directory -Path $menu -Force | Out-Null
$link = Join-Path $menu 'AK Configurator.lnk'
$linkStage = Join-Path $menu ('.AK Configurator-' + [Guid]::NewGuid().ToString('N') + '.lnk')
$oldLink = $null
$hadTarget = Test-Path $target
$committed = $false
Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
using System.Text;
public static class AKConfiguratorWindowsPath {
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern uint GetLongPathName(string shortPath, StringBuilder longPath, uint bufferLength);
}
'@
function Get-CanonicalWindowsPath([string]$path) {
  $fullPath = [IO.Path]::GetFullPath($path)
  $buffer = New-Object Text.StringBuilder 32768
  $length = [AKConfiguratorWindowsPath]::GetLongPathName($fullPath, $buffer, $buffer.Capacity)
  if ($length -eq 0 -or $length -ge $buffer.Capacity) { throw "Cannot resolve Windows path: $path" }
  return $buffer.ToString().TrimEnd([char]92)
}
try {
  New-Item -ItemType Directory -Path $stage -ErrorAction Stop | Out-Null
  Get-ChildItem $source -Force | Copy-Item -Destination $stage -Recurse
  Set-Content -NoNewline -Path (Join-Path $stage '.ak-configurator-owned') -Value 'ak-configurator-desktop-candidate-1'
  if (Test-Path $link) {
    $oldLink = (New-Object -ComObject WScript.Shell).CreateShortcut($link).TargetPath
    $expectedLinkTarget = Join-Path $target 'ak-configurator.exe'
    if (-not [StringComparer]::OrdinalIgnoreCase.Equals((Get-CanonicalWindowsPath $oldLink), (Get-CanonicalWindowsPath $expectedLinkTarget))) {
      throw "Refusing to replace unmanaged Start menu shortcut: $link"
    }
  }
  if (Test-Path $target) { Move-Item -LiteralPath $target -Destination $backup }
  try { Move-Item -LiteralPath $stage -Destination $target }
  catch { if (Test-Path $backup) { Move-Item -LiteralPath $backup -Destination $target }; throw }
  $shell = New-Object -ComObject WScript.Shell
  $shortcut = $shell.CreateShortcut($linkStage)
  $shortcut.TargetPath = Join-Path $target 'ak-configurator.exe'
  $shortcut.WorkingDirectory = $target
  $shortcut.Save()
  Move-Item -LiteralPath $linkStage -Destination $link -Force
  $committed = $true
} catch {
  if (-not $committed) {
    if ((Test-Path $target) -and (Test-Path $backup)) { Remove-Item -LiteralPath $target -Recurse -Force; Move-Item -LiteralPath $backup -Destination $target }
    elseif ((Test-Path $target) -and -not $hadTarget) { Remove-Item -LiteralPath $target -Recurse -Force }
  }
  throw
} finally {
  if (Test-Path $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
  if (Test-Path $linkStage) { Remove-Item -LiteralPath $linkStage -Force }
}
if (Test-Path $backup) {
  try { Remove-Item -LiteralPath $backup -Recurse -Force }
  catch { Write-Warning "Update succeeded; previous installation remains at $backup." }
}
Write-Output "Installed for this user at $target. Launch from the Start menu."
