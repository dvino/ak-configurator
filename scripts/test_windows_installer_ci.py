#!/usr/bin/env python3
"""Offline checks for the installer CI safety gates."""
import io
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import bootstrap_inno_setup as bootstrap
import test_windows_installer_runtime as runtime
import windows_signing_ci as signing


class BootstrapTests(unittest.TestCase):
    def test_rejects_wrong_download_before_signature_or_execution(self):
        with tempfile.TemporaryDirectory() as temp, \
             mock.patch.object(bootstrap.urllib.request, 'urlopen', return_value=io.BytesIO(b'wrong')), \
             mock.patch.object(bootstrap.subprocess, 'run') as run:
            with self.assertRaisesRegex(ValueError, 'size or SHA256'):
                bootstrap.bootstrap(Path(temp) / 'new')
            run.assert_not_called()

    def test_refuses_existing_destination(self):
        with tempfile.TemporaryDirectory() as temp, \
             mock.patch.object(bootstrap.urllib.request, 'urlopen') as download:
            with self.assertRaisesRegex(ValueError, 'Destination'):
                bootstrap.bootstrap(Path(temp))
            download.assert_not_called()


class RuntimeGuardTests(unittest.TestCase):
    def test_existing_start_menu_group_is_refused(self):
        with tempfile.TemporaryDirectory() as temp, \
             mock.patch.object(runtime.uuid, 'uuid4', return_value=runtime.uuid.UUID(int=0)):
            group = Path(temp) / ('AK Configurator Test ' + '0' * 32)
            group.mkdir()
            with self.assertRaisesRegex(ValueError, 'group is unavailable'):
                runtime.new_group(Path(temp))

    def test_setup_without_group_override_proof_is_refused_before_execution(self):
        with tempfile.TemporaryDirectory() as temp, mock.patch.object(runtime.subprocess, 'run') as run:
            setup = Path(temp) / 'setup.exe'
            setup.write_bytes(b'fixture')
            setup.with_name(setup.name + '.build.json').write_text('{}', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'group override'):
                runtime.execute(setup, Path(temp) / 'smoke.json')
            run.assert_not_called()

    def test_refuses_foreign_or_unregistered_installation(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            install = root / 'program'
            with mock.patch.object(runtime, 'existing_install', return_value=[str(root / 'other')]):
                with self.assertRaisesRegex(ValueError, 'another location'):
                    runtime.guard(root, install)
                with self.assertRaisesRegex(ValueError, 'another location'):
                    runtime.guard(root, install, install)
            with mock.patch.object(runtime, 'existing_install', return_value=[]):
                with self.assertRaisesRegex(ValueError, 'not registered'):
                    runtime.guard(root, install, install)
            with mock.patch.object(runtime, 'existing_install', return_value=[str(install), str(root / 'other')]):
                with self.assertRaisesRegex(ValueError, 'another location'):
                    runtime.guard(root, install, install)

    def test_refuses_path_outside_root(self):
        with tempfile.TemporaryDirectory() as temp, mock.patch.object(runtime, 'existing_install', return_value=[]):
            with self.assertRaisesRegex(ValueError, 'escaped'):
                runtime.guard(Path(temp), Path(temp).parent / 'elsewhere')


class SigningGuardTests(unittest.TestCase):
    def test_stock_certificate_modules_load_with_inherited_module_path_hidden(self):
        env = {**os.environ, 'PSModulePath': str(Path(tempfile.gettempdir()) / 'ak-no-modules')}
        script = r"""
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1') -Force
Import-Module (Join-Path $env:windir 'System32\WindowsPowerShell\v1.0\Modules\PKI\PKI.psd1') -Force
if (-not (Get-PSProvider Certificate)) { throw 'Certificate provider unavailable' }
foreach ($name in @('Get-AuthenticodeSignature', 'Import-PfxCertificate', 'Import-Certificate')) {
    if (-not (Get-Command $name -ErrorAction Stop)) { throw "Missing command: $name" }
}
"""
        signing.run_powershell(script, env)

    def test_powershell_failure_propagates(self):
        with self.assertRaises(signing.subprocess.CalledProcessError):
            signing.run_powershell("throw 'expected fixture failure'", dict(os.environ))

    def test_signing_diagnostics_are_bounded_and_do_not_echo_exception(self):
        error = signing.subprocess.CalledProcessError(1, ['encoded command'])
        with mock.patch.object(signing.subprocess, 'run', side_effect=error) as run:
            with self.assertRaises(signing.subprocess.CalledProcessError) as caught:
                signing.run_powershell(signing.IMPORT, {})
        encoded = run.call_args.args[0][-1]
        wrapped = signing.base64.b64decode(encoded).decode('utf-16le')
        self.assertIn("$stage = 'pfx-ephemeral-import'", wrapped)
        self.assertIn("$stage = 'pfx-my-import'", wrapped)
        self.assertIn("'root-import'", wrapped)
        self.assertIn("'publisher-import'", wrapped)
        self.assertIn('GetBaseException().HResult', wrapped)
        self.assertNotIn('$_.Exception.Message', wrapped)
        self.assertEqual(caught.exception.cmd, 'Windows PowerShell signing helper')

    def test_powershell_timeout_redacts_command(self):
        error = signing.subprocess.TimeoutExpired(['encoded-secret-marker'], 120)
        with mock.patch.object(signing.subprocess, 'run', side_effect=error):
            with self.assertRaises(signing.subprocess.TimeoutExpired) as caught:
                signing.run_powershell('fixture', {})
        self.assertEqual(caught.exception.timeout, 120)
        self.assertNotIn('encoded-secret-marker', str(caught.exception))
        self.assertEqual(caught.exception.cmd, 'Windows PowerShell signing helper')

    def test_machine_publisher_is_checked_imported_and_cleaned(self):
        machine = r'Cert:\LocalMachine\TrustedPublisher'
        user = r'Cert:\CurrentUser\TrustedPublisher'
        self.assertEqual(signing.IMPORT.count(machine), 2)
        self.assertEqual(signing.CLEANUP.count(machine), 1)
        self.assertNotIn(user, signing.IMPORT + signing.CLEANUP)
        self.assertLess(signing.IMPORT.index('Certificate already exists'),
                        signing.IMPORT.index("WriteAllText($env:AK_CI_CERT_OWNERSHIP"))
        self.assertIn('if ($imported.Thumbprint -ne $thumb)', signing.IMPORT)
        self.assertIn('if (-not (Test-Path -LiteralPath $env:AK_CI_CERT_OWNERSHIP))', signing.CLEANUP)
        self.assertIn('if (Test-Path $path) { Remove-Item $path -Force }', signing.CLEANUP)

    def test_untrusted_event_rejected_before_import(self):
        with mock.patch.dict(os.environ, {'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'github-hosted', 'GITHUB_REPOSITORY': 'dvino/ak-configurator',
                                          'GITHUB_EVENT_NAME': 'pull_request'}, clear=True), \
             mock.patch.object(signing, 'run_powershell') as powershell:
            with self.assertRaisesRegex(ValueError, 'Untrusted'):
                signing.execute(Path('unused'), ['unused'])
            powershell.assert_not_called()

    def test_cleanup_after_child_failure_and_secrets_removed_from_child(self):
        env = {'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'github-hosted', 'GITHUB_REPOSITORY': 'dvino/ak-configurator',
               'GITHUB_EVENT_NAME': 'push', signing.THUMBPRINT: 'A' * 40,
               signing.MODE: 'local-development', signing.PFX: 'YWJj', signing.PASSWORD: 'secret'}
        with tempfile.TemporaryDirectory() as temp:
            public = Path(temp) / 'public.cer'
            public.write_bytes(b'fixture')
            env['RUNNER_TEMP'] = temp
            with mock.patch.dict(os.environ, env, clear=True), \
                 mock.patch.object(signing, 'run_powershell') as powershell, \
                 mock.patch.object(signing.subprocess, 'run', side_effect=signing.subprocess.CalledProcessError(1, 'child')) as child:
                with self.assertRaises(signing.subprocess.CalledProcessError):
                    signing.execute(public, ['child'])
                self.assertEqual([call.args[0] for call in powershell.call_args_list], [signing.IMPORT, signing.CLEANUP])
                self.assertNotIn(signing.PFX, child.call_args.kwargs['env'])
                self.assertNotIn(signing.PASSWORD, child.call_args.kwargs['env'])


if __name__ == '__main__':
    unittest.main()
