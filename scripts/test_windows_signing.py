import unittest
import struct
import tempfile
from pathlib import Path
from unittest.mock import patch
from build_windows_installer import check_start_menu_override

from windows_signing import (SIGNED_PATHS, canonical_report, check_pe_signature_only,
                             check_signed_script, digest, thumbprint, validate_timestamp_url, verify_file, verify_signed_payload)


class SigningContractTests(unittest.TestCase):
    def test_timestamp_endpoint_restricts_http_and_shell_syntax(self):
        self.assertEqual(validate_timestamp_url('http://timestamp.digicert.com'), 'http://timestamp.digicert.com')
        self.assertEqual(validate_timestamp_url('https://example.com/rfc3161'), 'https://example.com/rfc3161')
        for invalid in ('http://timestamp.digicert.com/', 'http://timestamp.digicert.com.evil',
                        'http://example.com', 'http://timestamp.digicert.com&whoami',
                        'https://example.com/a&whoami', 'https://example.com/$(whoami)'):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                validate_timestamp_url(invalid)

    def test_start_menu_override_requires_unconditional_setup_directives(self):
        text = '[Setup]\nDisableProgramGroupPage=no\nUsePreviousGroup=no\n[Files]\n'
        check_start_menu_override(text)
        for invalid in (text.replace('Page=no', 'Page=yes'),
                        text.replace('UsePreviousGroup=no', ''),
                        text.replace('UsePreviousGroup=no', '#ifdef X\nUsePreviousGroup=no\n#endif'),
                        text + '[Setup]\n',
                        text.replace('UsePreviousGroup=no', 'UsePreviousGroup=no\nUsePreviousGroup=yes')):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                check_start_menu_override(invalid)

    @patch('windows_signing.subprocess.run')
    def test_powershell_uses_host_modules_with_unusable_module_path(self, run):
        with tempfile.TemporaryDirectory() as tmp, patch.dict('os.environ', {'PSModulePath': 'invalid'}):
            path = Path(tmp) / 'fixture.ps1'
            path.write_bytes(b'test')
            verify_file(path, 'A' * 40, 'unused')
        args, kwargs = run.call_args
        command = args[0]
        self.assertIn('Microsoft.PowerShell.Security.psd1', command[-1])
        self.assertIn('Microsoft.PowerShell.Utility.psd1', command[-1])
        self.assertEqual({k.upper(): v for k, v in kwargs['env'].items()}['PSMODULEPATH'], 'invalid')
        self.assertEqual(kwargs['env']['AK_SIGN_PATH'], str(path.resolve()))

    @patch('windows_signing.verify_report_signature')
    def test_descriptor_binds_known_paths_and_script_body(self, verify_cms):
        block = b'# SIG # Begin signature block\r\n# YWJjZA==\r\n# SIG # End signature block\r\n'
        static = {'install-user.ps1': b'install\r\n', 'uninstall-user.ps1': b'uninstall\r\n'}
        contents = {'ak-configurator.exe': b'signed exe', 'audio/cava.exe': b'signed helper',
                    **{name: body + block for name, body in static.items()}, 'windows-signing.p7s': b'fixture'}
        entries = {name: {'unsigned_sha256': digest(static.get(name, b'original PE')),
                          'signed_sha256': digest(contents[name])} for name in SIGNED_PATHS}
        contents['windows-signing.json'] = canonical_report(entries)
        verify_signed_payload(contents, static, '.')
        verify_cms.assert_called_once()
        contents['install-user.ps1'] = b'changed\r\n' + block
        entries['install-user.ps1']['signed_sha256'] = digest(contents['install-user.ps1'])
        contents['windows-signing.json'] = canonical_report(entries)
        with self.assertRaises(ValueError):
            verify_signed_payload(contents, static, '.')
        entries['unexpected.exe'] = entries['ak-configurator.exe']
        contents['windows-signing.json'] = canonical_report(entries)
        with self.assertRaises(ValueError):
            verify_signed_payload(contents, static, '.')

    def test_pe_signature_only_and_tampering(self):
        unsigned = bytearray(0x305)
        unsigned[:2] = b'MZ'
        struct.pack_into('<I', unsigned, 0x3c, 0x80)
        unsigned[0x80:0x84] = b'PE\0\0'
        struct.pack_into('<H', unsigned, 0x86, 1)
        struct.pack_into('<H', unsigned, 0x94, 240)
        opt = 0x98
        struct.pack_into('<H', unsigned, opt, 0x20b)
        struct.pack_into('<I', unsigned, opt + 108, 16)
        struct.pack_into('<II', unsigned, opt + 240 + 16, 0x100, 0x200)
        signed = unsigned + b'\0' * 3 + struct.pack('<IHH', 12, 0x200, 2) + b'PKCS' + b'\0' * 4
        struct.pack_into('<II', signed, opt + 144, 0x308, 16)
        struct.pack_into('<I', signed, opt + 64, 1234)
        check_pe_signature_only(unsigned, signed)
        for where in (0x210, 0x40, 0x304):
            altered = bytearray(signed)
            altered[where] ^= 1
            with self.subTest(where=where), self.assertRaises(ValueError):
                check_pe_signature_only(unsigned, altered)
        for offset, size in ((0x200, 16), (0x308, 100), (0x309, 16)):
            altered = bytearray(signed)
            struct.pack_into('<II', altered, opt + 144, offset, size)
            with self.subTest(offset=offset, size=size), self.assertRaises(ValueError):
                check_pe_signature_only(unsigned, altered)

    def test_appended_block_preserves_source_exactly(self):
        source = b'\xef\xbb\xbfWrite-Output "hello"\r\n'
        block = b'# SIG # Begin signature block\r\n# YWJjZA==\r\n# SIG # End signature block\r\n'
        check_signed_script(source, source + block)
        for invalid in (source.replace(b'hello', b'other') + block,
                        source + block + b'Write-Output "injected"\r\n',
                        source + block.replace(b'YWJjZA==', b'!'),
                        source + block + block):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                check_signed_script(source, invalid)

    def test_source_signature_and_invalid_pin_rejected(self):
        with self.assertRaises(ValueError):
            check_signed_script(b'# SIG # Begin signature block', b'')
        for invalid in ('a' * 39, 'g' * 40, 'a' * 40 + ' ', ''):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                thumbprint(invalid)
        self.assertEqual(thumbprint('ab' * 20), 'AB' * 20)


if __name__ == '__main__':
    unittest.main()
