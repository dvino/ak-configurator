"""Offline drift checks for the actual CAVA dependency resolver configuration."""
import json
import hashlib
import io
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import build_cava_helper as helper


class CavaDependencyPinTests(unittest.TestCase):
    def fixture(self, root):
        for filename in ('vcpkg.json', 'vcpkg-configuration.json'):
            (root / filename).write_bytes((helper.SOURCE / filename).read_bytes())

    def test_shipped_manifest_and_actual_registry_agree(self):
        self.assertEqual(helper.validate_vcpkg_configuration(),
                         '64810c6bf2bdc91ebcd75f140aec77580113f219')

    def test_explicit_registry_cannot_override_manifest_or_add_moving_artifacts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            path = root / 'vcpkg-configuration.json'
            original = json.loads(path.read_text())
            drifted = json.loads(path.read_text())
            drifted['default-registry']['baseline'] = '66c0373dc7fca549e5803087b9487edfe3aca0a1'
            path.write_text(json.dumps(drifted))
            with self.assertRaisesRegex(ValueError, 'resolver'):
                helper.validate_vcpkg_configuration(root)
            original['registries'] = [{'kind': 'artifact', 'location': 'https://example.invalid/main.zip'}]
            path.write_text(json.dumps(original))
            with self.assertRaisesRegex(ValueError, 'resolver'):
                helper.validate_vcpkg_configuration(root)

    def test_archived_port_recipes_must_match_resolved_version_pins(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            for port in json.loads((root / 'vcpkg.json').read_text())['overrides']:
                path = root / 'ports' / port['name']
                path.mkdir(parents=True)
                (path / 'vcpkg.json').write_text(json.dumps(port))
            baseline = helper.validate_vcpkg_configuration(root)
            with patch.object(helper.subprocess, 'check_output', return_value=baseline + '\n'):
                helper.validate_vcpkg_configuration(root, root)
                path = root / 'ports/fftw3/vcpkg.json'
                port = json.loads(path.read_text())
                port['version'] = '3.3.11'
                path.write_text(json.dumps(port))
                with self.assertRaisesRegex(ValueError, 'Archived vcpkg recipe'):
                    helper.validate_vcpkg_configuration(root, root)
            with patch.object(helper.subprocess, 'check_output', return_value='0' * 40):
                with self.assertRaisesRegex(ValueError, 'VCPKG_ROOT'):
                    helper.validate_vcpkg_configuration(root, root)


class WindowsPkgconfTests(unittest.TestCase):
    def test_supported_windows_architectures_and_unknown_target_rejection(self):
        for machine, expected in (('AMD64', 'x64'), ('x86_64', 'x64'),
                                  ('ARM64', 'arm64'), ('aarch64', 'arm64')):
            with self.subTest(machine=machine), patch.object(helper.platform, 'machine', return_value=machine):
                self.assertEqual(helper.windows_architecture(), expected)
        with patch.object(helper.platform, 'machine', return_value='x86'):
            with self.assertRaisesRegex(ValueError, 'Unsupported Windows architecture'):
                helper.windows_architecture()

    def test_verified_msi_is_extracted_without_product_installation(self):
        data = b'pinned MSI fixture'
        for arch in ('x64', 'arm64'):
            with self.subTest(arch=arch), tempfile.TemporaryDirectory(prefix='pkgconf test ') as directory:
                output = Path(directory)
                executable = output / arch / 'Program Files' / 'pkgconf-3.0.10' / 'pkgconf.exe'
                def extract(command, **kwargs):
                    self.assertEqual(command[:2], ['msiexec.exe', '/a'])
                    self.assertNotIn('/i', command)
                    self.assertIn('TARGETDIR=' + str(output / arch), command)
                    self.assertEqual(Path(command[2]).read_bytes(), data)
                    executable.parent.mkdir(parents=True)
                    executable.write_bytes(b'native exe fixture')
                pins = {arch: (hashlib.sha256(data).hexdigest(), len(data))}
                with patch.object(helper.platform, 'system', return_value='Windows'), \
                     patch.object(helper, 'windows_architecture', return_value=arch), \
                     patch.object(helper, 'PKGCONF_WINDOWS', pins), \
                     patch.object(helper.urllib.request, 'urlopen', return_value=io.BytesIO(data)) as download, \
                     patch.object(helper, 'run', side_effect=extract), \
                     patch.object(helper.subprocess, 'check_output', return_value='3.0.10\n'):
                    self.assertEqual(helper.prepare_windows_pkgconf(output), executable)
                self.assertEqual(download.call_args.args[0],
                                 f'https://github.com/pkgconf/pkgconf/releases/download/pkgconf-3.0.10/pkgconf-{arch}-3.0.10.msi')

    def test_changed_and_oversized_downloads_never_reach_windows_installer(self):
        original = b'valid MSI fixture'
        pins = {'x64': (hashlib.sha256(original).hexdigest(), len(original))}
        for data in (b'X' * len(original), original + b'X'):
            with self.subTest(data=data), tempfile.TemporaryDirectory() as directory, \
                 patch.object(helper.platform, 'system', return_value='Windows'), \
                 patch.object(helper, 'windows_architecture', return_value='x64'), \
                 patch.object(helper, 'PKGCONF_WINDOWS', pins), \
                 patch.object(helper.urllib.request, 'urlopen', return_value=io.BytesIO(data)), \
                 patch.object(helper, 'run') as run:
                with self.assertRaisesRegex(ValueError, 'size or SHA256 mismatch'):
                    helper.prepare_windows_pkgconf(directory)
                run.assert_not_called()
                self.assertFalse(list(Path(directory).glob('*.msi')))

    def test_corrupt_cached_msi_is_rejected_without_retry(self):
        with tempfile.TemporaryDirectory() as directory, \
             patch.object(helper.platform, 'system', return_value='Windows'), \
             patch.object(helper, 'windows_architecture', return_value='x64'), \
             patch.object(helper.urllib.request, 'urlopen') as download, \
             patch.object(helper, 'run') as run:
            (Path(directory) / 'pkgconf-x64-3.0.10.msi').write_bytes(b'corrupt')
            with self.assertRaisesRegex(ValueError, 'size or SHA256 mismatch'):
                helper.prepare_windows_pkgconf(directory)
            download.assert_not_called()
            run.assert_not_called()

    def test_extraction_failure_is_not_hidden(self):
        data = b'MSI fixture'
        with tempfile.TemporaryDirectory() as directory, \
             patch.object(helper.platform, 'system', return_value='Windows'), \
             patch.object(helper, 'windows_architecture', return_value='x64'), \
             patch.object(helper, 'PKGCONF_WINDOWS', {'x64': (hashlib.sha256(data).hexdigest(), len(data))}), \
             patch.object(helper.urllib.request, 'urlopen', return_value=io.BytesIO(data)), \
             patch.object(helper, 'run', side_effect=subprocess.CalledProcessError(1603, 'msiexec')):
            with self.assertRaises(subprocess.CalledProcessError):
                helper.prepare_windows_pkgconf(directory)

    def test_missing_or_ambiguous_extracted_tool_is_rejected(self):
        data = b'MSI fixture'
        for count in (0, 2):
            with self.subTest(count=count), tempfile.TemporaryDirectory() as directory, \
                 patch.object(helper.platform, 'system', return_value='Windows'), \
                 patch.object(helper, 'windows_architecture', return_value='x64'), \
                 patch.object(helper, 'PKGCONF_WINDOWS', {'x64': (hashlib.sha256(data).hexdigest(), len(data))}), \
                 patch.object(helper.urllib.request, 'urlopen', return_value=io.BytesIO(data)), \
                 patch.object(helper, 'run'):
                for index in range(count):
                    candidate = Path(directory) / 'x64' / str(index) / 'pkgconf.exe'
                    candidate.parent.mkdir(parents=True)
                    candidate.write_bytes(b'fixture')
                with self.assertRaisesRegex(ValueError, 'exactly one'):
                    helper.prepare_windows_pkgconf(directory)

    def test_missing_relative_and_wrong_version_tools_fail_before_cmake(self):
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaisesRegex(ValueError, 'prepare-windows-pkgconf'):
                helper.windows_build_environment()
        with patch.dict(os.environ, {'PKG_CONFIG': 'pkgconf.exe'}, clear=True):
            with self.assertRaisesRegex(ValueError, 'absolute executable path'):
                helper.windows_build_environment()
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / 'pkgconf.exe'
            executable.write_bytes(b'fixture')
            with patch.dict(os.environ, {'PKG_CONFIG': str(executable)}, clear=True), \
                 patch.object(helper.subprocess, 'check_output', return_value='2.3.0\n'):
                with self.assertRaisesRegex(ValueError, 'Expected native pkgconf'):
                    helper.windows_build_environment()

    def test_pkgconfig_survives_clean_vcpkg_environment_and_keeps_existing_settings(self):
        with tempfile.TemporaryDirectory(prefix='native tool ') as directory:
            executable = Path(directory) / 'pkgconf.exe'
            executable.write_bytes(b'fixture')
            for existing, expected in (('', 'PKG_CONFIG'), ('OTHER', 'OTHER;PKG_CONFIG'),
                                       ('OTHER;PKG_CONFIG', 'OTHER;PKG_CONFIG')):
                values = {'PKG_CONFIG': str(executable), 'VCPKG_KEEP_ENV_VARS': existing, 'OTHER': 'preserved'}
                with self.subTest(existing=existing), patch.dict(os.environ, values, clear=True), \
                     patch.object(helper.subprocess, 'check_output', return_value='3.0.10\n'):
                    env = helper.windows_build_environment()
                    self.assertEqual(env['PKG_CONFIG'], str(executable))
                    self.assertEqual(env['VCPKG_KEEP_ENV_VARS'], expected)
                    self.assertEqual(env['OTHER'], 'preserved')
                    self.assertEqual(os.environ['VCPKG_KEEP_ENV_VARS'], existing)

    def test_both_windows_cmake_steps_use_native_pkgconf_and_keep_static_targets(self):
        for machine, triplet, generator in (('AMD64', 'x64-windows-static', 'x64'),
                                             ('ARM64', 'arm64-windows-static', 'ARM64')):
            with self.subTest(machine=machine), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                output = root / 'audio'
                vcpkg = root / 'vcpkg'
                executable = root / 'pkgconf.exe'
                executable.write_bytes(b'fixture')
                for port in ('fftw3', 'pthreads'):
                    (vcpkg / 'buildtrees' / port / 'src').mkdir(parents=True)
                    (vcpkg / 'ports' / port).mkdir(parents=True)
                    (vcpkg / 'ports' / port / 'portfile.cmake').write_text('recipe')
                (output / 'build/vcpkg_installed' / triplet / 'share').mkdir(parents=True)
                (output / 'build/Release').mkdir(parents=True)
                (output / 'build/Release/cava.exe').write_bytes(b'built helper')
                with patch.object(helper.platform, 'system', return_value='Windows'), \
                     patch.object(helper.platform, 'machine', return_value=machine), \
                     patch.object(helper, 'validate_vcpkg_configuration'), \
                     patch.dict(os.environ, {'PKG_CONFIG': str(executable), 'VCPKG_ROOT': str(vcpkg)}, clear=True), \
                     patch.object(helper.subprocess, 'check_output', return_value='3.0.10\n'), \
                     patch.object(helper, 'run') as run:
                    self.assertEqual(helper.build(output), output / 'cava.exe')
                self.assertEqual(run.call_count, 2)
                configure, build = run.call_args_list
                self.assertIn('-DVCPKG_TARGET_TRIPLET=' + triplet, configure.args[0])
                self.assertEqual(configure.args[0][configure.args[0].index('-A') + 1], generator)
                self.assertIn('-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded', configure.args[0])
                self.assertIn('-DCMAKE_POLICY_DEFAULT_CMP0091=NEW', configure.args[0])
                for invocation in (configure, build):
                    self.assertEqual(invocation.kwargs['env']['PKG_CONFIG'], str(executable))
                    self.assertEqual(invocation.kwargs['env']['VCPKG_KEEP_ENV_VARS'], 'PKG_CONFIG')
                self.assertEqual((output / 'cava.exe').read_bytes(), b'built helper')
                with tarfile.open(output / 'cava-windows-dependency-sources.tar.gz') as archive:
                    self.assertIn('fftw3/vcpkg-port/portfile.cmake', archive.getnames())
                    self.assertIn('pthreads/vcpkg-port/portfile.cmake', archive.getnames())
                    self.assertIn('licenses-and-build-metadata', archive.getnames())


if __name__ == '__main__':
    unittest.main()
