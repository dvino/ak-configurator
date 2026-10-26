#!/usr/bin/env python3
"""Build the pinned CAVA helper without starting capture or touching a device.

Unix: FFTW and iniparser link statically; Linux uses the system PulseAudio client
ABI (also available on PipeWire through pipewire-pulse). Windows uses vcpkg's
static triplet. No package manager runs implicitly.
"""
import argparse
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import tarfile
import hashlib
import json

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'vendor/cava'


def run(command, **kwargs):
    subprocess.run([str(arg) for arg in command], check=True, **kwargs)


def validate_vcpkg_configuration(source=SOURCE, tool_root=None):
    """Fail closed if the resolver, manifest, tool or archived port recipe drift."""
    manifest = json.loads((Path(source) / 'vcpkg.json').read_text())
    configuration = json.loads((Path(source) / 'vcpkg-configuration.json').read_text())
    baseline = manifest['builtin-baseline']
    expected_registry = {'kind': 'git', 'baseline': baseline,
                         'repository': 'https://github.com/microsoft/vcpkg'}
    if configuration != {'default-registry': expected_registry}:
        raise ValueError('CAVA vcpkg resolver must use the manifest baseline without extra registries')
    versions = {port['name']: (port['version'], port.get('port-version', 0))
                for port in manifest.get('overrides', [])}
    if versions != {'fftw3': ('3.3.10', 10), 'pthreads': ('3.0.0', 14)}:
        raise ValueError('CAVA vcpkg dependency version pins differ from the supported recipes')
    if tool_root is not None:
        actual = subprocess.check_output(['git', '-C', str(tool_root), 'rev-parse', 'HEAD'], text=True).strip()
        if actual != baseline:
            raise ValueError('VCPKG_ROOT must use pinned commit ' + baseline)
        for name, expected_version in versions.items():
            port = json.loads((Path(tool_root) / 'ports' / name / 'vcpkg.json').read_text())
            if (port.get('version'), port.get('port-version', 0)) != expected_version:
                raise ValueError('Archived vcpkg recipe differs from resolved pin: ' + name)
    return baseline


def build(output, prefix=None):
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    validate_vcpkg_configuration()
    system = platform.system()
    if system == 'Windows':
        triplet = 'arm64-windows-static' if platform.machine().lower() in ('arm64', 'aarch64') else 'x64-windows-static'
        vcpkg = Path(os.environ['VCPKG_ROOT'])
        validate_vcpkg_configuration(tool_root=vcpkg)
        run(['cmake', '-S', SOURCE, '-B', output / 'build', '-A', 'ARM64' if triplet.startswith('arm64') else 'x64',
             '-DCMAKE_TOOLCHAIN_FILE=' + str(vcpkg / 'scripts/buildsystems/vcpkg.cmake'),
             '-DVCPKG_TARGET_TRIPLET=' + triplet, '-DUSING_VCPKG=ON',
             '-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded', '-DCMAKE_POLICY_DEFAULT_CMP0091=NEW'])
        run(['cmake', '--build', output / 'build', '--config', 'Release', '--parallel', '2'])
        shutil.copy2(output / 'build/Release/cava.exe', output / 'cava.exe')
        # Preserve the actual patched vcpkg source and build recipes used by this
        # runner, rather than assuming that a moving registry used our versions.
        with tarfile.open(output / 'cava-windows-dependency-sources.tar.gz', 'w:gz') as archive:
            for port in ('fftw3', 'pthreads'):
                source_root = vcpkg / 'buildtrees' / port / 'src'
                if not source_root.is_dir():
                    raise ValueError('Corresponding dependency source missing: ' + port)
                archive.add(source_root, arcname=port + '/source')
                archive.add(vcpkg / 'ports' / port, arcname=port + '/vcpkg-port')
            archive.add(output / 'build/vcpkg_installed' / triplet / 'share', arcname='licenses-and-build-metadata')
        return output / 'cava.exe'
    if system not in ('Linux', 'Darwin'):
        raise ValueError('Unsupported native platform')
    # Explicit dependency prefix supports isolated development builds and Homebrew.
    if prefix is None:
        prefix = output / 'dependencies'
        prefix.mkdir(exist_ok=True)
        archives = ROOT / 'vendor/cava-dependencies'
        identities = json.loads((archives / 'provenance.json').read_text())
        for filename, identity in identities.items():
            archive = archives / filename
            if hashlib.sha256(archive.read_bytes()).hexdigest() != identity['sha256']:
                raise ValueError('Dependency source hash mismatch: ' + filename)
            if not filename.endswith(".tar.gz"):
                continue
            with tarfile.open(archive) as source_archive:
                source_archive.extractall(output / 'sources', filter='data')
        fftw = output / 'sources/fftw-3.3.10'
        run(['sh', fftw / 'configure', '--prefix=' + str(prefix), '--disable-shared', '--enable-static', '--disable-fortran'], cwd=fftw)
        run(['make', '-j2'], cwd=fftw)
        run(['make', 'install'], cwd=fftw)
        ini = output / 'sources/iniparser-4.2.6'
        (prefix / 'lib').mkdir(exist_ok=True)
        (prefix / 'include/iniparser').mkdir(parents=True, exist_ok=True)
        for source in ('dictionary', 'iniparser'):
            run([os.environ.get('CC', 'cc'), '-O2', '-c', ini / ('src/' + source + '.c'), '-o', output / (source + '.o')])
            shutil.copy2(ini / ('src/' + source + '.h'), prefix / 'include/iniparser')
        run(['ar', 'rcs', prefix / 'lib/libiniparser.a', output / 'dictionary.o', output / 'iniparser.o'])
    prefix = Path(prefix)
    def static_library(name):
        if prefix:
            choices = list(prefix.rglob('lib' + name + '.a'))
        else:
            libdir = subprocess.check_output(['pkg-config', '--variable=libdir', 'fftw3' if name == 'fftw3' else 'libiniparser'], text=True).strip()
            choices = [Path(libdir) / ('lib' + name + '.a')]
        if not choices or not choices[0].is_file():
            raise ValueError('Static dependency missing: ' + name)
        return choices[0]
    includes = []
    if prefix:
        includes = ['-I' + str(prefix / 'include'), '-I' + str(prefix / 'include/iniparser'), '-I' + str(prefix / 'usr/include'), '-I' + str(prefix / 'usr/include/iniparser')]
    else:
        includes = shlex.split(subprocess.check_output(['pkg-config', '--cflags', 'fftw3', 'libiniparser'], text=True))
    sources = ['cava.c', 'cavacore.c', 'config.c', 'input/common.c', 'input/fifo.c', 'input/shmem.c', 'output/terminal_noncurses.c', 'output/raw.c', 'output/noritake.c']
    flags = ['-I.', '-O2', '-std=gnu99', '-pthread', '-DHAVE_ALLOCA_H', '-DPACKAGE="cava"', '-DVERSION="1.0.0"'] + includes
    libraries = [static_library('fftw3'), static_library('iniparser'), '-lm']
    if system == 'Linux':
        flags += ['-DPULSE']
        sources += ['input/pulse.c']
        libraries += ['-lpulse-simple', '-lpulse', '-lrt']
        if prefix:
            flags += ['-L' + str(path.parent) for path in prefix.rglob('libpulse.so')]
    else:
        flags += ['-DCOREAUDIO', '-DCOREAUDIO_TAP', '-DNORT', '-mmacosx-version-min=14.2']
        sources += ['input/coreaudio.c', 'input/coreaudio_tap.m']
        libraries += ['-framework', 'CoreAudio', '-framework', 'AudioUnit', '-framework', 'CoreFoundation', '-framework', 'Foundation']
    run([os.environ.get('CC', 'cc'), *flags, *sources, *libraries, '-o', output / 'cava'], cwd=SOURCE)
    return output / 'cava'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--dependency-prefix', type=Path)
    args = parser.parse_args()
    print(build(args.output_dir, args.dependency_prefix))
