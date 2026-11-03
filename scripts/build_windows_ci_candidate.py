#!/usr/bin/env python3
"""Package and exercise one native Windows candidate on its CI runner."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--platform', choices=('windows-x86_64', 'windows-arm64'), required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--runtime-check', type=Path, required=True)
    parser.add_argument('--cava-helper', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--iscc', type=Path, required=True)
    parser.add_argument('--signtool', type=Path)
    parser.add_argument('--timestamp-url')
    parser.add_argument('--signed', action='store_true')
    args = parser.parse_args()
    common = [sys.executable]
    package = common + ['scripts/package_ak820_candidate.py', '--binary', str(args.binary),
        '--platform', args.platform, '--output-dir', str(args.output_dir),
        '--runtime-check', str(args.runtime_check), '--cava-helper', str(args.cava_helper)]
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
    base = f'ak-configurator-{version}-{args.platform}'
    installer = common + ['scripts/build_windows_installer.py', '--archive', str(args.output_dir / (base + '.zip')),
        '--platform', args.platform, '--iscc', str(args.iscc), '--output-dir', str(args.output_dir)]
    if args.signed:
        thumb = os.environ.get('AK_WINDOWS_CODE_SIGN_THUMBPRINT', '')
        if not thumb or not args.signtool or not args.timestamp_url:
            parser.error('Signed builds require thumbprint, signtool and timestamp URL')
        package += ['--windows-signing-thumbprint', thumb, '--signtool', str(args.signtool), '--timestamp-url', args.timestamp_url]
        installer += ['--signing-thumbprint', thumb, '--signtool', str(args.signtool), '--timestamp-url', args.timestamp_url]
    subprocess.run(package, cwd=ROOT, check=True)
    # The release verifier binds GUI smoke to the actual packaged bytes, which
    # differ from the pre-signing binary. Extract only the named executable.
    with tempfile.TemporaryDirectory(prefix='ak-signed-smoke-') as temp:
        binary = Path(temp) / 'ak-configurator.exe'
        with zipfile.ZipFile(args.output_dir / (base + '.zip')) as archive:
            binary.write_bytes(archive.read(base + '/ak-configurator.exe'))
        subprocess.run(common + ['scripts/test_mcp_production_metadata.py', '--binary', str(binary)], cwd=ROOT, check=True)
        subprocess.run(common + ['scripts/run_windows_ci_runtime_smoke.py', '--binary', str(binary),
            '--report', str(args.output_dir / ('runtime-smoke-' + args.platform + '.json'))], cwd=ROOT, check=True)
    subprocess.run(installer, cwd=ROOT, check=True)
    subprocess.run(common + ['scripts/test_ak820_installed_candidate.py', '--archive',
        str(args.output_dir / (base + '.zip')), '--platform', args.platform,
        '--report', str(args.output_dir / ('install-smoke-' + args.platform + '.json'))], cwd=ROOT, check=True)
    setup_test = common + ['scripts/test_windows_installer_runtime.py', '--execute',
        '--setup', str(args.output_dir / (base + '-setup.exe')),
        '--report', str(args.output_dir / ('setup-smoke-' + args.platform + '.json'))]
    if args.signed:
        setup_test += ['--signing-thumbprint', thumb, '--signtool', str(args.signtool)]
    subprocess.run(setup_test, cwd=ROOT, check=True)


if __name__ == '__main__':
    main()
