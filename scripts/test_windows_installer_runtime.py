#!/usr/bin/env python3
"""Exercise install, reinstall and uninstall in an isolated Windows runner account."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import uuid
import winreg

KEY = r'Software\Microsoft\Windows\CurrentVersion\Uninstall\{98D90734-FF33-4BCE-9E69-5434767DD69B}_is1'


def programs_folder():
    # Inno resolves shell folders through Windows, independently of APPDATA.
    folder_id = ctypes.create_string_buffer(uuid.UUID('a77f5d77-2e2b-44c3-a6a2-aba601054a51').bytes_le)
    value = ctypes.c_void_p()
    shell = ctypes.windll.shell32
    shell.SHGetKnownFolderPath.argtypes = [ctypes.c_void_p, ctypes.c_uint32, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
    shell.SHGetKnownFolderPath.restype = ctypes.c_long
    status = shell.SHGetKnownFolderPath(folder_id, 0, None, ctypes.byref(value))
    if status != 0:
        raise OSError(f'Cannot resolve current-user Programs folder: {status}')
    try:
        return Path(ctypes.wstring_at(value)).resolve(strict=True)
    finally:
        free = ctypes.windll.ole32.CoTaskMemFree
        free.argtypes = [ctypes.c_void_p]
        free.restype = None
        free(value)


def new_group(programs):
    programs = programs.resolve(strict=True)
    name = 'AK Configurator Test ' + uuid.uuid4().hex
    path = programs / name
    if path.exists() or path.is_symlink() or path.parent.resolve() != programs:
        raise ValueError('Fresh Start Menu group is unavailable')
    return name, path


def existing_install():
    locations = []
    for view in (winreg.KEY_WOW64_32KEY, winreg.KEY_WOW64_64KEY):
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, KEY, 0, winreg.KEY_READ | view) as key:
                try:
                    locations.append(winreg.QueryValueEx(key, 'InstallLocation')[0])
                except FileNotFoundError:
                    locations.append('<unknown location>')
        except FileNotFoundError:
            continue
    return locations


def guard(root, install, expected=None):
    root = root.resolve(strict=True)
    if not root.is_relative_to(Path(tempfile.gettempdir()).resolve()) or root == Path(tempfile.gettempdir()).resolve():
        raise ValueError('Test root must be inside the system temporary directory')
    if not install.resolve().is_relative_to(root) or install.resolve() == root:
        raise ValueError('Install directory escaped test root')
    existing = existing_install()
    if any(expected is None or Path(location).resolve() != expected.resolve() for location in existing):
        raise ValueError('Existing Inno installation belongs to another location; refusing test')
    if expected is not None and not existing:
        raise ValueError('Expected test installation is not registered')


def execute(setup, report, signing_thumbprint=None, signtool=None):
    setup = setup.resolve(strict=True)
    if report.exists():
        raise ValueError('Report already exists')
    build = json.loads(setup.with_name(setup.name + '.build.json').read_text(encoding='utf-8'))
    if (build.get('start_menu_group_override') is not True
            or build.get('output_sha256') != hashlib.sha256(setup.read_bytes()).hexdigest()):
        raise ValueError('Setup build report does not prove the isolated group override')
    # Keep the directory on failure so an incomplete install is not deleted blindly.
    root = Path(tempfile.mkdtemp(prefix='ak-installer-test-')).resolve()
    print(f'Isolated installer test directory: {root}', flush=True)
    install = root / 'program'
    local = root / 'local'
    roaming = root / 'roaming'
    local.mkdir()
    roaming.mkdir()
    sentinel = local / 'preserve-user-data.txt'
    sentinel.write_text('keep', encoding='ascii')
    env = {**os.environ, 'LOCALAPPDATA': str(local), 'APPDATA': str(roaming)}
    group_name, group_path = new_group(programs_folder())
    guard(root, install)
    command = [str(setup), '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER',
               f'/DIR={install}', f'/GROUP={group_name}']
    subprocess.run(command, env=env, check=True, timeout=300)
    guard(root, install, install)
    if group_path.is_symlink() or not (group_path / 'AK Configurator.lnk').is_file():
        raise ValueError('Isolated Start Menu shortcut missing')
    executable = install / 'ak-configurator.exe'
    check = root / 'self-check.json'
    subprocess.run([str(executable), '--self-check', '--output', str(check)], env=env, check=True, timeout=60)
    result = json.loads(check.read_text(encoding='utf-8'))
    if result.get('status') != 'passed' or result.get('hardware_access') is not False or result.get('network_access') is not False:
        raise ValueError('Installed self-check did not pass offline')
    binary_hash = hashlib.sha256(executable.read_bytes()).hexdigest()
    guard(root, install, install)
    subprocess.run(command, env=env, check=True, timeout=300)
    guard(root, install, install)
    if group_path.is_symlink() or not (group_path / 'AK Configurator.lnk').is_file():
        raise ValueError('Isolated Start Menu shortcut missing after reinstall')
    if hashlib.sha256(executable.read_bytes()).hexdigest() != binary_hash:
        raise ValueError('Reinstall changed executable unexpectedly')
    uninstaller = install / 'unins000.exe'
    if signing_thumbprint:
        if not signtool:
            raise ValueError('Signature verification requires SignTool')
        from windows_signing import verify_file
        verify_file(uninstaller, signing_thumbprint, signtool)
    subprocess.run([str(uninstaller), '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART'],
                   env=env, check=True, timeout=300)
    guard(root, install)
    if executable.exists() or sentinel.read_text(encoding='ascii') != 'keep':
        raise ValueError('Uninstall or user data preservation failed')
    evidence = {'format': 'ak-configurator-windows-installer-smoke-1',
        'setup_sha256': hashlib.sha256(setup.read_bytes()).hexdigest(), 'installed_self_check': result,
        'reinstall_verified': True, 'uninstalled': True, 'user_data_preserved': True,
        'start_menu_group_isolated': True}
    if signing_thumbprint:
        evidence.update(uninstaller_signature_verified=True, signer_thumbprint=signing_thumbprint.upper())
    report.write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
    # Only a successful, registry-free test owns this fresh directory.
    guard(root, install)
    import shutil
    shutil.rmtree(root)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--setup', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--execute', action='store_true', help='explicitly permit install, reinstall and uninstall')
    parser.add_argument('--signing-thumbprint')
    parser.add_argument('--signtool', type=Path)
    args = parser.parse_args()
    if not args.execute:
        parser.error('--execute is required')
    try:
        if bool(args.signing_thumbprint) != bool(args.signtool):
            raise ValueError('Provide both signing thumbprint and SignTool')
        execute(args.setup, args.report, args.signing_thumbprint, args.signtool)
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        parser.exit(1, f'Installer runtime test stopped: {exc}\n')


if __name__ == '__main__':
    main()
