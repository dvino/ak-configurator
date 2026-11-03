#!/usr/bin/env python3
"""Install the pinned, authenticated Inno compiler into a new private directory."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import urllib.request

URL = 'https://github.com/jrsoftware/issrc/releases/download/is-7_1_1/innosetup-7.1.1-x64.exe'
SIZE = 14336568
SHA256 = '8a5a6711c3766a6e5fb74f280ba0e3551e719ce94538d901fdbd1f8f9920b3f0'


def bootstrap(destination):
    if os.name != 'nt':
        raise ValueError('Windows required')
    destination = destination.resolve()
    if destination.exists() or not destination.parent.is_dir():
        raise ValueError('Destination must be new and its parent must exist')
    with tempfile.TemporaryDirectory(prefix='inno-bootstrap-') as temp:
        installer = Path(temp) / 'inno.exe'
        with urllib.request.urlopen(URL, timeout=120) as source, installer.open('xb') as target:
            data = source.read(SIZE + 1)
            target.write(data)
        if len(data) != SIZE or hashlib.sha256(data).hexdigest() != SHA256:
            raise ValueError('Inno download size or SHA256 mismatch')
        # The exact pinned bytes and Windows trust verification must both pass.
        check = subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command',
            "$ErrorActionPreference='Stop'; "
            "Import-Module (Join-Path $PSHOME 'Modules\\Microsoft.PowerShell.Security\\Microsoft.PowerShell.Security.psd1') -Force; "
            '$s=Get-AuthenticodeSignature -LiteralPath $env:INNO_INSTALLER; '
            'if ($s.Status -ne "Valid" -or $s.SignerCertificate.Subject -notmatch "Pyrsys B.V.") { exit 1 }'],
            env={**os.environ, 'INNO_INSTALLER': str(installer)}, check=False, timeout=60)
        if check.returncode:
            raise ValueError('Inno Authenticode verification failed')
        subprocess.run([str(installer), '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART',
                        '/CURRENTUSER', f'/DIR={destination}'], check=True, timeout=300,
                       capture_output=True)
    compiler = destination / 'ISCC.exe'
    if not compiler.is_file():
        raise ValueError('Installed ISCC.exe missing')
    probe = subprocess.run([str(compiler), '--version'], capture_output=True, text=True, errors='replace', timeout=30, check=True)
    if probe.stdout.strip() != '7.1.1':
        raise ValueError('Unexpected compiler version')
    return {'version': '7.1.1', 'compiler': str(compiler), 'installer_sha256': SHA256}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--destination', type=Path, required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(bootstrap(args.destination)))
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        parser.exit(1, f'Inno bootstrap failed: {exc}\n')


if __name__ == '__main__':
    main()
