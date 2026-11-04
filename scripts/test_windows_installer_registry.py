"""Offline checks for the Windows installer runtime registry guard."""
import json
import hashlib
import subprocess
import unittest
from unittest import mock

import test_windows_installer_runtime as runtime


class RegistryGuardTests(unittest.TestCase):
    def setUp(self):
        patcher = mock.patch.object(runtime.ctypes, 'windll')
        windll = patcher.start()
        self.addCleanup(patcher.stop)
        windll.kernel32.GetCurrentPackageFullName.return_value = 15700

    def query(self, value):
        output = json.dumps(value).encode('utf-8')
        with mock.patch.object(runtime.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, output)) as run:
            result = runtime.existing_install()
        self.assertEqual(run.call_args.kwargs['env']['AK_INSTALLER_TEST_REGISTRY_KEY'], runtime.KEY)
        return result

    def test_absent_and_present_views(self):
        self.assertEqual(self.query([{'present': False, 'location': None},
                                     {'present': True, 'location': r'C:\Installed'}]), [r'C:\Installed'])

    def test_two_absent_views(self):
        self.assertEqual(self.query([{'present': False, 'location': None}] * 2), [])

    def test_unknown_location_is_not_absent(self):
        self.assertEqual(self.query([{'present': True, 'location': '<unknown location>'},
                                     {'present': False, 'location': None}]), ['<unknown location>'])

    def test_invalid_output_fails_closed(self):
        for value in ([], {}, [{'present': False, 'location': None}] * 3,
                      [{'present': 0, 'location': None}] * 2,
                      [{'present': False, 'location': 'unexpected'}] * 2,
                      [{'present': True, 'location': ''}] * 2):
            with self.subTest(value=value), self.assertRaises(ValueError):
                self.query(value)
        with mock.patch.object(runtime.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, b'bad json')):
            with self.assertRaises(ValueError):
                runtime.existing_install()

    def test_failed_query_fails_closed(self):
        for error in (OSError('unavailable'), subprocess.TimeoutExpired('powershell', 30),
                      subprocess.CalledProcessError(1, 'powershell')):
            with self.subTest(error=error), mock.patch.object(runtime.subprocess, 'run', side_effect=error):
                with self.assertRaises(type(error)):
                    runtime.existing_install()

    def test_packaged_or_unknown_identity_fails_closed(self):
        for status in (122, 5):
            runtime.ctypes.windll.kernel32.GetCurrentPackageFullName.return_value = status
            with self.subTest(status=status), mock.patch.object(runtime.subprocess, 'run') as run:
                with self.assertRaises(OSError):
                    runtime.existing_install()
                run.assert_not_called()

    def test_execute_checks_identity_before_creating_test_state(self):
        setup = mock.MagicMock()
        setup.resolve.return_value = setup
        setup.read_bytes.return_value = b'setup'
        setup.name = 'setup.exe'
        setup.with_name.return_value.read_text.return_value = json.dumps({
            'start_menu_group_override': True,
            'output_sha256': hashlib.sha256(b'setup').hexdigest(),
        })
        report = mock.MagicMock()
        report.exists.return_value = False
        for status in (122, 5):
            runtime.ctypes.windll.kernel32.GetCurrentPackageFullName.return_value = status
            with self.subTest(status=status), \
                    mock.patch.object(runtime.tempfile, 'mkdtemp') as mkdtemp, \
                    mock.patch.object(runtime, 'programs_folder') as programs, \
                    mock.patch.object(runtime, 'new_group') as group, \
                    mock.patch.object(runtime.subprocess, 'run') as run:
                with self.assertRaises(OSError):
                    runtime.execute(setup, report)
                mkdtemp.assert_not_called()
                programs.assert_not_called()
                group.assert_not_called()
                run.assert_not_called()


if __name__ == '__main__':
    unittest.main()
