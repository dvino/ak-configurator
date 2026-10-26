$ErrorActionPreference = 'Stop'
$target = Join-Path $env:LOCALAPPDATA 'Programs/AK Configurator'
$link = Join-Path $env:APPDATA 'Microsoft/Windows/Start Menu/Programs/AK Configurator.lnk'
if ((Test-Path $target) -and -not (Test-Path (Join-Path $target '.ak-configurator-owned'))) { throw 'Refusing to remove an unowned installation.' }
if (Get-Process -Name 'ak-configurator' -ErrorAction SilentlyContinue) { throw 'Close AK Configurator before uninstalling.' }
if (Test-Path $link) {
  $linkTarget = (New-Object -ComObject WScript.Shell).CreateShortcut($link).TargetPath
  if ($linkTarget -eq (Join-Path $target 'ak-configurator.exe')) { Remove-Item -LiteralPath $link }
  else { Write-Warning 'Leaving an unmanaged Start menu shortcut unchanged.' }
}
if (Test-Path $target) {
  $answer = Read-Host "Remove $target and installed app files? [y/N]"
  if ($answer -match '^(y|yes)$') { Remove-Item -LiteralPath $target -Recurse -Force }
}
Write-Output 'Per-user launcher removed; application files are removed only after confirmation.'
