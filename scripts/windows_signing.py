"""Strict Windows signing primitives. Private keys are never passed on command lines."""
import hashlib
import json
import os
import re
import struct
import subprocess
import tempfile
from pathlib import Path

_THUMBPRINT = re.compile(r"[0-9A-F]{40}\Z")
_TIMESTAMP = re.compile(r'https://[A-Za-z0-9.-]+(?::[0-9]+)?(?:/[A-Za-z0-9._~/-]*)?\Z')
_BLOCK = re.compile(rb"(?:\r?\n)?# SIG # Begin signature block\r?\n(?:# [A-Za-z0-9+/=]+\r?\n)+# SIG # End signature block\r?\n?\Z")
CERT_THUMBPRINT = '1D9596F961B7F24D9F66AE7BE2B1EA39FF1543C7'
SIGNED_PATHS = ('ak-configurator.exe', 'audio/cava.exe', 'install-user.ps1', 'uninstall-user.ps1')
_PS_MODULES = ("$ErrorActionPreference='Stop'; "
               "Import-Module (Join-Path $PSHOME 'Modules\\Microsoft.PowerShell.Utility\\Microsoft.PowerShell.Utility.psd1') -Force; "
               "Import-Module (Join-Path $PSHOME 'Modules\\Microsoft.PowerShell.Security\\Microsoft.PowerShell.Security.psd1') -Force; ")


def thumbprint(value):
    value = value.upper()
    if not _THUMBPRINT.fullmatch(value):
        raise ValueError("Expected a 40-character hexadecimal certificate thumbprint")
    return value


def digest(data):
    return hashlib.sha256(data).hexdigest()


def validate_timestamp_url(value):
    if value != 'http://timestamp.digicert.com' and not _TIMESTAMP.fullmatch(value or ''):
        raise ValueError('A safe HTTPS timestamp URL or the exact DigiCert HTTP endpoint is required')
    return value


def pinned_certificate(root):
    data = (Path(root) / 'assets/windows/ak-configurator-development.cer').read_bytes()
    if hashlib.sha1(data).hexdigest().upper() != CERT_THUMBPRINT:
        raise ValueError('Development certificate does not match the pinned thumbprint')
    return data


def canonical_report(files):
    return (json.dumps({'format': 'ak-configurator-windows-signing-1', 'mode': 'local-development',
                        'public_trust': False, 'signer_thumbprint': CERT_THUMBPRINT,
                        'files': files}, sort_keys=True, separators=(',', ':')) + '\n').encode('ascii')


def sign_report(report, cert):
    if thumbprint(cert) != CERT_THUMBPRINT:
        raise ValueError('Unexpected development signing certificate')
    with tempfile.TemporaryDirectory(prefix='ak-cms-') as tmp:
        report_path, signature = Path(tmp) / 'report.json', Path(tmp) / 'report.p7s'
        report_path.write_bytes(report)
        command = (_PS_MODULES + "Add-Type -AssemblyName System.Security; "
                   "$data=[IO.File]::ReadAllBytes($env:AK_CMS_INPUT); "
                   "$cms=[Security.Cryptography.Pkcs.SignedCms]::new([Security.Cryptography.Pkcs.ContentInfo]::new($data),$true); "
                   "$cert=Get-Item ('Cert:\\CurrentUser\\My\\'+$env:AK_SIGN_THUMB); "
                   "$signer=New-Object Security.Cryptography.Pkcs.CmsSigner $cert; "
                   "$signer.DigestAlgorithm=New-Object Security.Cryptography.Oid '2.16.840.1.101.3.4.2.1'; "
                   "$cms.ComputeSignature($signer); [IO.File]::WriteAllBytes($env:AK_CMS_OUTPUT,$cms.Encode())")
        subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', command], check=True,
                       env={**os.environ, 'AK_CMS_INPUT': str(report_path), 'AK_CMS_OUTPUT': str(signature),
                            'AK_SIGN_THUMB': cert})
        return signature.read_bytes()


def verify_report_signature(report, signature, root):
    certificate = pinned_certificate(root)
    with tempfile.TemporaryDirectory(prefix='ak-cms-verify-') as tmp:
        directory = Path(tmp)
        cert_path, report_path, sig_path = (directory / name for name in ('cert.der', 'report.json', 'report.p7s'))
        cert_path.write_bytes(certificate)
        report_path.write_bytes(report)
        sig_path.write_bytes(signature)
        if os.name == 'nt':
            command = (_PS_MODULES + "Add-Type -AssemblyName System.Security; "
                       "$b=[IO.File]::ReadAllBytes($env:AK_CMS_INPUT); "
                       "$cms=[Security.Cryptography.Pkcs.SignedCms]::new([Security.Cryptography.Pkcs.ContentInfo]::new($b),$true); "
                       "$cms.Decode([IO.File]::ReadAllBytes($env:AK_CMS_SIGNATURE)); "
                       "if ($cms.SignerInfos.Count -ne 1 -or $cms.SignerInfos[0].Certificate.Thumbprint -ne $env:AK_SIGN_THUMB) { throw 'Wrong signer' }; "
                       "$cms.CheckSignature($true)")
            subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', command], check=True,
                           env={**os.environ, 'AK_CMS_INPUT': str(report_path), 'AK_CMS_SIGNATURE': str(sig_path),
                                'AK_SIGN_THUMB': CERT_THUMBPRINT})
        else:
            pem = directory / 'cert.pem'
            subprocess.run(['openssl', 'x509', '-inform', 'DER', '-in', str(cert_path), '-out', str(pem)], check=True, capture_output=True)
            subprocess.run(['openssl', 'cms', '-verify', '-binary', '-inform', 'DER', '-in', str(sig_path),
                            '-content', str(report_path), '-nointern', '-certfile', str(pem), '-noverify',
                            '-out', os.devnull], check=True, capture_output=True)


def verify_signed_payload(contents, expected_static, root):
    report_bytes = contents['windows-signing.json']
    verify_report_signature(report_bytes, contents['windows-signing.p7s'], root)
    report = json.loads(report_bytes)
    files = report.get('files') if isinstance(report, dict) else None
    if not isinstance(files, dict) or set(files) != set(SIGNED_PATHS) or report_bytes != canonical_report(files):
        raise ValueError('Malformed signed payload report')
    for name in SIGNED_PATHS:
        info = files[name]
        if (not isinstance(info, dict) or set(info) != {'unsigned_sha256', 'signed_sha256'}
                or not re.fullmatch('[0-9a-f]{64}', info.get('unsigned_sha256', ''))
                or info.get('signed_sha256') != digest(contents[name])):
            raise ValueError(f'Invalid signed payload identity: {name}')
        if name.endswith('.ps1'):
            source = expected_static[name]
            check_signed_script(source, contents[name])
            if info['unsigned_sha256'] != digest(source):
                raise ValueError(f'Signed script source mismatch: {name}')
    return report


def check_signed_script(source, signed):
    """Accept only an appended Authenticode block, preserving every source byte."""
    if b'# SIG # Begin signature block' in source:
        raise ValueError('Source already contains a signature block')
    if not signed.startswith(source):
        raise ValueError('Signing changed script source bytes')
    suffix = signed[len(source):]
    if not _BLOCK.fullmatch(suffix):
        raise ValueError('Invalid or missing appended signature block')


def _pe_parts(data):
    if len(data) < 64 or data[:2] != b'MZ':
        raise ValueError('Invalid PE DOS header')
    pe = struct.unpack_from('<I', data, 0x3c)[0]
    if pe > len(data) - 24 or data[pe:pe + 4] != b'PE\0\0':
        raise ValueError('Invalid PE header')
    count, optional_size = struct.unpack_from('<H12xH', data, pe + 6)
    opt = pe + 24
    if optional_size < 152 or opt + optional_size > len(data) or struct.unpack_from('<H', data, opt)[0] != 0x20b:
        raise ValueError('Invalid PE32+ optional header')
    if struct.unpack_from('<I', data, opt + 108)[0] < 5:
        raise ValueError('PE security directory absent')
    directory, checksum = opt + 144, opt + 64
    sections = opt + optional_size
    if count == 0 or sections + count * 40 > len(data):
        raise ValueError('Invalid PE sections')
    end = sections + count * 40
    for index in range(count):
        size, offset = struct.unpack_from('<II', data, sections + index * 40 + 16)
        if size and (offset > len(data) or size > len(data) - offset):
            raise ValueError('PE section out of bounds')
        if size:
            end = max(end, offset + size)
    cert_offset, cert_size = struct.unpack_from('<II', data, directory)
    if bool(cert_offset) != bool(cert_size):
        raise ValueError('Invalid PE certificate directory')
    if cert_offset:
        if cert_offset % 8 or cert_offset < end or cert_offset + cert_size != len(data):
            raise ValueError('Invalid PE certificate placement')
        cursor = cert_offset
        while cursor < len(data):
            if cursor + 8 > len(data):
                raise ValueError('Truncated WIN_CERTIFICATE')
            length, revision, kind = struct.unpack_from('<IHH', data, cursor)
            if length <= 8 or revision != 0x200 or kind != 2 or cursor + length > len(data):
                raise ValueError('Invalid WIN_CERTIFICATE')
            aligned = (length + 7) & ~7
            if cursor + aligned > len(data) or any(data[cursor + length:cursor + aligned]):
                raise ValueError('Invalid WIN_CERTIFICATE padding')
            cursor += aligned
        if cursor != len(data):
            raise ValueError('Invalid WIN_CERTIFICATE sequence')
    return checksum, directory, cert_offset, cert_size


def check_pe_signature_only(unsigned, signed):
    """Prove byte changes are confined to PE signing fields and certificate table."""
    old_checksum, old_directory, old_offset, _ = _pe_parts(unsigned)
    checksum, directory, offset, _ = _pe_parts(signed)
    if old_offset or not offset or (checksum, directory) != (old_checksum, old_directory):
        raise ValueError('Expected unsigned input and signed output with matching headers')
    padding = (-len(unsigned)) % 8
    if offset != len(unsigned) + padding or signed[len(unsigned):offset] != b'\0' * padding:
        raise ValueError('Unexpected bytes before certificate table')
    original, result = bytearray(unsigned), bytearray(signed[:len(unsigned)])
    for part in (original, result):
        part[checksum:checksum + 4] = b'\0' * 4
        part[directory:directory + 8] = b'\0' * 8
    if original != result:
        raise ValueError('Signing changed PE content')


def verify_file(path, cert, signtool):
    path = Path(path).resolve(strict=True)
    cert = thumbprint(cert)
    if path.suffix.lower() == '.exe':
        subprocess.run([str(signtool), 'verify', '/pa', '/all', str(path)], check=True)
    verify = (_PS_MODULES + "$s=Get-AuthenticodeSignature -LiteralPath $env:AK_SIGN_PATH; "
              "if ($s.Status -ne 'Valid' -or $null -eq $s.SignerCertificate -or "
              "$s.SignerCertificate.Thumbprint -ne $env:AK_SIGN_THUMB -or $null -eq $s.TimeStamperCertificate) "
              "{ throw 'Signature, signer or timestamp verification failed' }")
    subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', verify,
                    ], check=True, env={**os.environ, 'AK_SIGN_PATH': str(path), 'AK_SIGN_THUMB': cert})


def sign_file(path, cert, signtool, timestamp_url):
    """Sign an owned staging file and verify its signature and pinned signer on Windows."""
    path = Path(path).resolve(strict=True)
    cert = thumbprint(cert)
    timestamp_url = validate_timestamp_url(timestamp_url)
    before = path.read_bytes()
    if path.suffix.lower() == '.ps1':
        # Environment values are data, not interpolated PowerShell source.
        script = (_PS_MODULES + "$c=Get-Item ('Cert:\\CurrentUser\\My\\'+$env:AK_SIGN_THUMB); "
                  "$s=Set-AuthenticodeSignature -LiteralPath $env:AK_SIGN_PATH -Certificate $c "
                  "-HashAlgorithm SHA256 -TimestampServer $env:AK_SIGN_TIMESTAMP; "
                  "if ($s.Status -ne 'Valid') { throw 'Script signing failed' }")
        subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', script], check=True,
                       env={**os.environ, 'AK_SIGN_PATH': str(path), 'AK_SIGN_THUMB': cert,
                            'AK_SIGN_TIMESTAMP': timestamp_url})
        check_signed_script(before, path.read_bytes())
    elif path.suffix.lower() == '.exe':
        subprocess.run([str(signtool), 'sign', '/sha1', cert, '/fd', 'SHA256', '/tr', timestamp_url,
                        '/td', 'SHA256', str(path)], check=True)
        check_pe_signature_only(before, path.read_bytes())
    else:
        raise ValueError('Unsupported signing file type')
    verify_file(path, cert, signtool)
    return {'unsigned_sha256': digest(before), 'signed_sha256': digest(path.read_bytes()),
            'signer_thumbprint': cert, 'windows_signature_verified': True,
            'public_trust': False}
