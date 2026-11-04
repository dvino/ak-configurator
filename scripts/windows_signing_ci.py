#!/usr/bin/env python3
"""Run one Windows CI command with a temporary local-development signing key."""
import argparse
import base64
import os
from pathlib import Path
import subprocess
import tempfile

PFX = 'AK_WINDOWS_CODE_SIGN_PFX_BASE64'
PASSWORD = 'AK_WINDOWS_CODE_SIGN_PFX_PASSWORD'
THUMBPRINT = 'AK_WINDOWS_CODE_SIGN_THUMBPRINT'
MODE = 'AK_WINDOWS_CODE_SIGN_MODE'

# Scripts are fixed source text. Credentials reach PowerShell through its private
# environment, never command-line arguments or generated files other than the PFX.
IMPORT = r'''
$ErrorActionPreference = 'Stop'
$stage = 'security-module'
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1') -Force
$stage = 'pki-module'
Import-Module (Join-Path $env:windir 'System32\WindowsPowerShell\v1.0\Modules\PKI\PKI.psd1') -Force
$stage = 'administrator-check'
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Elevated GitHub-hosted runner required' }
$stage = 'public-certificate'
$thumb = $env:AK_CI_CERT_THUMB
$password = ConvertTo-SecureString $env:AK_WINDOWS_CODE_SIGN_PFX_PASSWORD -AsPlainText -Force
$public = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new($env:AK_CI_CERT_PUBLIC)
if ($public.Thumbprint -ne $thumb -or $public.HasPrivateKey) { throw 'Public certificate mismatch' }
$stage = 'pfx-ephemeral-import'
$collection = [System.Security.Cryptography.X509Certificates.X509Certificate2Collection]::new()
$flags = [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet
$collection.Import($env:AK_CI_CERT_PFX, $env:AK_WINDOWS_CODE_SIGN_PFX_PASSWORD, $flags)
$stage = 'pfx-identity'
if ($collection.Count -ne 1 -or $collection[0].Thumbprint -ne $thumb -or -not $collection[0].HasPrivateKey) { throw 'PFX identity mismatch' }
$collection[0].Dispose()
$stage = 'existing-certificate-check'
foreach ($store in @('Cert:\CurrentUser\My', 'Cert:\LocalMachine\Root', 'Cert:\LocalMachine\TrustedPublisher')) {
    if (Test-Path "$store\$thumb") { throw "Certificate already exists in $store" }
}
# Mark ownership only after all pre-existing store checks have passed.
$stage = 'ownership-marker'
[IO.File]::WriteAllText($env:AK_CI_CERT_OWNERSHIP, 'owned')
$stage = 'pfx-my-import'
$imported = Import-PfxCertificate -FilePath $env:AK_CI_CERT_PFX -CertStoreLocation Cert:\CurrentUser\My -Password $password
if ($imported.Thumbprint -ne $thumb) { throw 'Imported PFX mismatch' }
foreach ($store in @('Cert:\LocalMachine\Root', 'Cert:\LocalMachine\TrustedPublisher')) {
    if ($store -eq 'Cert:\LocalMachine\TrustedPublisher') {
        $stage = 'publisher-store-open'
        $registry = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryView]::Registry64)
        try {
            $key = $registry.OpenSubKey('SOFTWARE\Microsoft\SystemCertificates\TrustedPublisher', $false)
            try { [Console]::WriteLine(('Publisher registry container before open: {0}' -f ($null -ne $key))) }
            finally { if ($key) { $key.Dispose() } }
            $publisher = [System.Security.Cryptography.X509Certificates.X509Store]::new('TrustedPublisher', [System.Security.Cryptography.X509Certificates.StoreLocation]::LocalMachine)
            try { $publisher.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite) }
            finally { $publisher.Close() }
            $key = $registry.OpenSubKey('SOFTWARE\Microsoft\SystemCertificates\TrustedPublisher', $false)
            try { [Console]::WriteLine(('Publisher registry container after open: {0}' -f ($null -ne $key))) }
            finally { if ($key) { $key.Dispose() } }
        } finally { $registry.Dispose() }
    }
    $stage = if ($store -eq 'Cert:\LocalMachine\Root') { 'root-import' } else { 'publisher-import' }
    $imported = Import-Certificate -FilePath $env:AK_CI_CERT_PUBLIC -CertStoreLocation $store
    if ($imported.Thumbprint -ne $thumb) { throw "Imported $store certificate mismatch" }
}
'''

CLEANUP = r'''
$ErrorActionPreference = 'Stop'
$stage = 'cleanup-security-module'
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1') -Force
if (-not (Test-Path -LiteralPath $env:AK_CI_CERT_OWNERSHIP)) { exit 0 }
$stage = 'cleanup-owned-certificate'
$thumb = $env:AK_CI_CERT_THUMB
$errors = @()
try {
    $cert = Get-Item "Cert:\CurrentUser\My\$thumb" -ErrorAction SilentlyContinue
    if ($cert -and $cert.HasPrivateKey) {
        $rsa = [System.Security.Cryptography.X509Certificates.RSACertificateExtensions]::GetRSAPrivateKey($cert)
        if ($rsa -is [System.Security.Cryptography.RSACng]) { $rsa.Key.Delete(); $rsa.Dispose() }
        elseif ($rsa -is [System.Security.Cryptography.RSACryptoServiceProvider]) { $rsa.PersistKeyInCsp = $false; $rsa.Clear() }
        else { throw 'Unsupported private-key provider' }
    }
} catch { $errors += 'Private-key cleanup failed' }
foreach ($store in @('Cert:\CurrentUser\My', 'Cert:\LocalMachine\Root', 'Cert:\LocalMachine\TrustedPublisher')) {
    try {
        $path = "$store\$thumb"
        if (Test-Path $path) { Remove-Item $path -Force }
        if (Test-Path $path) { throw 'Certificate remains' }
    } catch { $errors += "Certificate cleanup failed in $store" }
}
if ($errors.Count) {
    foreach ($message in $errors) { [Console]::Error.WriteLine($message) }
    throw 'Signing cleanup failed'
}
'''


def run_powershell(script, env):
    wrapped = ("$stage = 'initialization'\ntry {\n" + script +
               "\n} catch { [Console]::Error.WriteLine(('Windows signing stage={0} HRESULT=0x{1:X8} base-HRESULT=0x{2:X8}' -f $stage, ($_.Exception.HResult -band 0xFFFFFFFFL), ($_.Exception.GetBaseException().HResult -band 0xFFFFFFFFL))); exit 1 }\n")
    encoded = base64.b64encode(wrapped.encode('utf-16le')).decode('ascii')
    try:
        return subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-EncodedCommand', encoded],
                              text=True, env=env, check=True, timeout=120)
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as exc:
        exc.cmd = 'Windows PowerShell signing helper'
        raise


def execute(public_cert, command):
    if os.name != 'nt' or os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('RUNNER_ENVIRONMENT') != 'github-hosted':
        raise ValueError('Signing is restricted to GitHub-hosted Windows runners')
    if os.environ.get('GITHUB_REPOSITORY') != 'dvino/ak-configurator' or os.environ.get('GITHUB_EVENT_NAME') not in ('push', 'workflow_dispatch'):
        raise ValueError('Untrusted repository or event')
    thumb = os.environ.get(THUMBPRINT, '').upper()
    if len(thumb) != 40 or any(c not in '0123456789ABCDEF' for c in thumb):
        raise ValueError('Invalid signing thumbprint')
    if os.environ.get(MODE) != 'local-development':
        raise ValueError('Only local-development signing mode is supported')
    if not os.environ.get(PFX) or not os.environ.get(PASSWORD):
        raise ValueError('Required signing secrets are missing')
    if not command:
        raise ValueError('Missing signing command')
    raw = base64.b64decode(os.environ[PFX], validate=True)
    if not raw or len(raw) > 1048576:
        raise ValueError('Invalid PFX size')
    public_cert = public_cert.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='ak-ci-sign-', dir=os.environ.get('RUNNER_TEMP')) as tmp:
        pfx = Path(tmp) / 'signing.pfx'
        pfx.write_bytes(raw)
        env = dict(os.environ)
        env.pop(PFX, None)
        env.update(AK_CI_CERT_THUMB=thumb, AK_CI_CERT_PFX=str(pfx),
                   AK_CI_CERT_PUBLIC=str(public_cert), AK_CI_CERT_OWNERSHIP=str(Path(tmp) / 'owned'))
        try:
            run_powershell(IMPORT, env)
            pfx.unlink()
            child_env = {k: v for k, v in os.environ.items() if k not in (PFX, PASSWORD)}
            subprocess.run(command, check=True, env=child_env, timeout=1800)
        finally:
            run_powershell(CLEANUP, env)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--public-cert', type=Path, required=True)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    try:
        execute(args.public_cert, args.command[1:] if args.command[:1] == ['--'] else args.command)
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        parser.exit(1, f'Windows CI signing failed: {exc}\n')


if __name__ == '__main__':
    main()
