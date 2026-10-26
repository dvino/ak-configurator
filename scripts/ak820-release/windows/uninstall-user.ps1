$ErrorActionPreference = 'Stop'
$target = Join-Path $env:LOCALAPPDATA 'Programs/AK Configurator'
$link = Join-Path $env:APPDATA 'Microsoft/Windows/Start Menu/Programs/AK Configurator.lnk'
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
if ((Test-Path $target) -and -not (Test-Path (Join-Path $target '.ak-configurator-owned'))) { throw 'Refusing to remove an unowned installation.' }
if (Get-Process -Name 'ak-configurator' -ErrorAction SilentlyContinue) { throw 'Close AK Configurator before uninstalling.' }
if (Test-Path $link) {
  $linkTarget = (New-Object -ComObject WScript.Shell).CreateShortcut($link).TargetPath
  $expectedLinkTarget = Join-Path $target 'ak-configurator.exe'
  if ([StringComparer]::OrdinalIgnoreCase.Equals((Get-CanonicalWindowsPath $linkTarget), (Get-CanonicalWindowsPath $expectedLinkTarget))) { Remove-Item -LiteralPath $link }
  else { Write-Warning 'Leaving an unmanaged Start menu shortcut unchanged.' }
}
if (Test-Path $target) {
  $answer = Read-Host "Remove $target and installed app files? [y/N]"
  if ($answer -match '^(y|yes)$') { Remove-Item -LiteralPath $target -Recurse -Force }
}
Write-Output 'Per-user launcher removed; application files are removed only after confirmation.'
