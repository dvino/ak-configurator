#!/usr/bin/env python3
"""Build a local Windows installer from a strictly checked portable candidate."""
import argparse
import json
import re
import subprocess
import tempfile
import tomllib
import zipfile
from pathlib import Path, PurePosixPath

from ak820_package_metadata import provenance_bytes
from package_ak820_candidate import ROOT, check_architecture
from windows_signing import CERT_THUMBPRINT, sign_report, thumbprint, validate_timestamp_url, verify_file, verify_signed_payload
from verify_ak820_release_candidate import (expected_static_files, expected_runtime_identity,
    normalize_runtime_profiles, sha256, verify_internal_checksums, verify_cava_source,
    verify_windows_audio_sources)

PLATFORM = "windows-x86_64"


def read_candidate(archive, version, commit, allow_dirty, platform=PLATFORM):
    if platform not in ('windows-x86_64', 'windows-arm64'):
        raise ValueError('Unsupported Windows installer platform')
    name = f"ak-configurator-{version}-{platform}.zip"
    if archive.name != name:
        raise ValueError("Unexpected candidate archive name")
    raw = archive.read_bytes()
    if archive.with_name(name + '.sha256').read_text(encoding='ascii').strip() != f"{sha256(raw)}  {name}":
        raise ValueError("Candidate external checksum mismatch")
    root = name[:-4] + '/'
    expected = expected_static_files(platform, commit)
    nameset = set(expected) | {'ak-configurator.exe', 'audio/cava.exe', 'manifest.json',
        'SHA256SUMS', 'runtime-self-check.json', 'audio-sources/cava-patched-source.tar.gz',
        'audio-sources/windows-built-dependencies.tar.gz'}
    with zipfile.ZipFile(archive) as z:
        infos = z.infolist()
        names = [i.filename for i in infos]
        signed_names = {root + 'windows-signing.json', root + 'windows-signing.p7s'}
        present = signed_names.intersection(names)
        if present and present != signed_names:
            raise ValueError('Incomplete Windows signing evidence')
        signed = bool(present)
        if signed:
            nameset |= {'windows-signing.json', 'windows-signing.p7s'}
        if len(names) != len(set(names)) or len({n.casefold() for n in names}) != len(names):
            raise ValueError('Duplicate archive path')
        for i in infos:
            n = i.filename
            p = PurePosixPath(n)
            if (not n.startswith(root) or '\\' in n or ':' in n or n.startswith('/')
                    or any(x in ('', '.', '..') for x in n.split('/'))
                    or i.is_dir() or ((i.external_attr >> 16) & 0o170000) == 0o120000):
                raise ValueError('Unsafe archive member')
        if {n[len(root):] for n in names} != nameset:
            raise ValueError('Unexpected candidate contents')
        contents = {n[len(root):]: z.read(n) for n in names}
    if signed:
        verify_signed_payload(contents, expected, ROOT)
    for n, data in expected.items():
        if signed and n in ('install-user.ps1', 'uninstall-user.ps1'):
            continue
        if contents[n] != data:
            raise ValueError(f'Source mismatch: {n}')
    verify_internal_checksums(contents)
    verify_cava_source(contents['audio-sources/cava-patched-source.tar.gz'])
    verify_windows_audio_sources(contents['audio-sources/windows-built-dependencies.tar.gz'])
    manifest = json.loads(contents['manifest.json'])
    if not isinstance(manifest, dict):
        raise ValueError('Candidate manifest is not an object')
    firmware = json.loads(provenance_bytes())
    binary, cava = contents['ak-configurator.exe'], contents['audio/cava.exe']
    check_architecture(binary, platform)
    check_architecture(cava, platform)
    cava_identity = json.loads((ROOT / 'vendor/cava-provenance.json').read_text())
    required = {'format': 'ak-configurator-desktop-candidate-1', 'platform': platform,
        'application_version': version, 'source_commit': commit,
        'source_repository': 'https://github.com/dvino/ak-configurator',
        'firmware_release': firmware['release'], 'firmware_commit': firmware['tag_commit'],
        'firmware_tag_object': firmware['tag_object'], 'firmware_provenance_sha256': sha256(provenance_bytes()),
        'binary_sha256': sha256(binary), 'audio_helper': {**cava_identity, 'path': 'audio/cava.exe', 'sha256': sha256(cava)}}
    if any(manifest.get(k) != v for k, v in required.items()) or not isinstance(manifest.get('source_dirty'), bool):
        raise ValueError('Candidate provenance or runtime binary identity mismatch')
    if manifest['source_dirty'] and not allow_dirty:
        raise ValueError('Dirty candidate requires --allow-dirty')
    runtime = json.loads(contents['runtime-self-check.json'])
    if not isinstance(runtime, dict):
        raise ValueError('Runtime identity is not an object')
    identity = expected_runtime_identity(platform)
    expected_runtime = {'format': 'ak-configurator-self-check-1', 'platform': platform,
        'application_version': version, 'status': 'passed', 'hardware_access': False,
        'network_access': False, **identity}
    if any(normalize_runtime_profiles(runtime).get(k) != v for k, v in expected_runtime.items()):
        raise ValueError('Runtime identity mismatch')
    return contents, manifest, sha256(raw)


def check_start_menu_override(setup_text):
    sections = re.split(r'^\s*\[Setup\]\s*$', setup_text, flags=re.I | re.M)
    if len(sections) != 2:
        raise ValueError('Installer must contain exactly one Setup section')
    setup_section = re.split(r'^\s*\[', sections[1], maxsplit=1, flags=re.M)[0]
    unconditional = re.split(r'^\s*#if', setup_section, maxsplit=1, flags=re.I | re.M)[0]
    for directive, expected in (('DisableProgramGroupPage', 'no'), ('UsePreviousGroup', 'no')):
        pattern = rf'^\s*{directive}\s*=\s*([^\r\n;]+)'
        values = re.findall(pattern, setup_section, re.I | re.M)
        if len(values) != 1 or values[0].strip().lower() != expected or not re.search(pattern, unconditional, re.I | re.M):
            raise ValueError(f'Installer must explicitly support isolated Start Menu group: {directive}')


def build(archive, compiler, output, allow_dirty=False, signing_thumbprint=None, signtool=None, timestamp_url=None, platform=PLATFORM):
    installer_source = ROOT / 'installer/ak-configurator.iss'
    check_start_menu_override(installer_source.read_text(encoding='utf-8-sig'))
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
    match = re.fullmatch(r'(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?', version)
    if not match or any(int(x) > 65535 for x in match.groups()):
        raise ValueError('Unsupported application version')
    numeric = '.'.join(match.groups()) + '.0'
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT).strip())
    if dirty and not allow_dirty:
        raise ValueError('Dirty checkout requires --allow-dirty')
    contents, manifest, archive_hash = read_candidate(archive, version, commit, allow_dirty, platform)
    compiler = compiler.resolve(strict=True)
    compiler_hash = sha256(compiler.read_bytes())
    probe = subprocess.run([str(compiler), '--version'], capture_output=True, text=True, errors='replace', check=True)
    compiler_version = re.fullmatch(r'(\d+\.\d+\.\d+)', probe.stdout.strip())
    if not compiler_version:
        raise ValueError('Could not identify ISCC compiler version')
    output = output.resolve(strict=True)
    base = f'ak-configurator-{version}-{platform}-setup.exe'
    destinations = [output / base, output / (base + '.sha256'), output / (base + '.build.json')]
    if signing_thumbprint:
        destinations.append(output / (base + '.build.p7s'))
    if any(p.exists() for p in destinations):
        raise ValueError('Refusing to overwrite existing output')
    # A private, bounded temporary directory contains only this build's staged payload.
    with tempfile.TemporaryDirectory(prefix='ak-setup-', dir=output) as tmp:
        tmp = Path(tmp).resolve()
        stage = tmp / 'payload'
        stage.mkdir()
        for name, data in contents.items():
            path = stage.joinpath(*PurePosixPath(name).parts)
            if not path.resolve().is_relative_to(stage.resolve()):
                raise ValueError('Staging path escaped')
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        command = [str(compiler), f'/DSourceDir={stage}', f'/DOutputDir={tmp}',
                   f'/DAppVersion={version}', f'/DNumericVersion={numeric}', f'/DMyPlatform={platform}']
        if signing_thumbprint or signtool or timestamp_url:
            signing_thumbprint = thumbprint(signing_thumbprint or '')
            if signing_thumbprint != CERT_THUMBPRINT:
                raise ValueError('Unexpected development signing certificate')
            if not signtool:
                raise ValueError('Signing requires signtool and a permitted timestamp URL')
            validate_timestamp_url(timestamp_url)
            signtool = signtool.resolve(strict=True)
            if any(c in str(signtool) for c in '"&|<>^%!'):
                raise ValueError('Invalid signtool path')
            # A signed installer must not silently contain unsigned payloads.
            for name in ('ak-configurator.exe', 'audio/cava.exe', 'install-user.ps1', 'uninstall-user.ps1'):
                verify_file(stage / name, signing_thumbprint, signtool)
            command += ['/DSigningEnabled', f'/Sakconfig="{signtool}" sign /sha1 {signing_thumbprint} /fd SHA256 /tr {timestamp_url} /td SHA256 $f']
        command.append(str(installer_source))
        subprocess.run(command, check=True)
        exe = tmp / base
        if signing_thumbprint:
            verify_file(exe, signing_thumbprint, signtool)
        digest = sha256(exe.read_bytes())
        report = {'format': 'ak-configurator-windows-installer-build-1', 'qualification': 'local single-platform build; not full release qualification',
            'application_version': version, 'platform': platform, 'setup_architecture': 'x64', 'source_commit': commit, 'checkout_dirty': dirty,
            'start_menu_group_override': True,
            'candidate_source_dirty': manifest['source_dirty'], 'input_zip': archive.name,
            'input_zip_sha256': archive_hash, 'input_executable_sha256': manifest['binary_sha256'],
            'compiler_name': compiler.name, 'compiler_sha256': compiler_hash,
            'compiler_version': compiler_version.group(1), 'output': base, 'output_sha256': digest,
            'signing': {'mode': 'local-development', 'signer_thumbprint': signing_thumbprint,
                        'public_trust': False, 'setup_signature_verified': True,
                        'signed_uninstaller_requested': True} if signing_thumbprint else {'mode': 'unsigned'}}
        payloads = [exe.read_bytes(), f'{digest}  {base}\n'.encode(),
                    (json.dumps(report, indent=2, sort_keys=True) + '\n').encode()]
        if signing_thumbprint:
            payloads.append(sign_report(payloads[2], signing_thumbprint))
        created = []
        try:
            for dest, data in zip(destinations, payloads):
                with dest.open('xb') as f:
                    created.append(dest)
                    f.write(data)
        except BaseException:
            for path in created:
                path.unlink(missing_ok=True)
            raise
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--iscc', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--allow-dirty', action='store_true')
    parser.add_argument('--signing-thumbprint')
    parser.add_argument('--signtool', type=Path)
    parser.add_argument('--timestamp-url')
    parser.add_argument('--platform', choices=('windows-x86_64', 'windows-arm64'), default=PLATFORM)
    args = parser.parse_args()
    try:
        print(json.dumps(build(args.archive, args.iscc, args.output_dir, args.allow_dirty,
                               args.signing_thumbprint, args.signtool, args.timestamp_url, args.platform), indent=2))
    except (OSError, ValueError, KeyError, TypeError, zipfile.BadZipFile, subprocess.CalledProcessError) as exc:
        parser.exit(1, f'Installer build rejected: {exc}\n')


if __name__ == '__main__':
    main()
