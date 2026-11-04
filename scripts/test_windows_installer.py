#!/usr/bin/env python3
"""Offline rejection tests; pass a candidate ZIP as the first argument."""
import json
import os
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

import build_windows_installer as builder

ARCHIVE = Path(sys.argv.pop(1)) if len(sys.argv) > 1 else None


class SignToolOptionTests(unittest.TestCase):
    def test_option_keeps_command_in_one_argument_without_literal_quotes(self):
        path = Path('C:/Program Files/SDK/$tools/signtool.exe')
        option = builder.inno_sign_tool_option(path,
                                              'A' * 40, 'https://example.test/timestamp')
        escaped_path = str(path).replace('$', '$$')
        self.assertEqual(option, f'--signtool=akconfig=$q{escaped_path}$q sign '
                         '/sha1 ' + 'A' * 40 + ' /fd SHA256 /tr https://example.test/timestamp /td SHA256 $f')
        self.assertNotIn('"', option)

    @unittest.skipUnless(os.environ.get('INNO_ISCC'), 'Set INNO_ISCC to run the native compiler parser')
    def test_real_compiler_parses_sign_tool_option(self):
        with tempfile.TemporaryDirectory(prefix='inno parser ') as tmp:
            script = Path(tmp) / 'fixture script.iss'
            script.write_text('[Setup]\nAppName=Parser fixture\nAppVersion=1\n'
                              'DefaultDirName={autopf}\\ParserFixture\nSignTool=akconfig\n', encoding='utf-8')
            option = builder.inno_sign_tool_option(Path(tmp) / '$tool folder' / 'signtool.exe',
                                                  'A' * 40, 'https://example.test/timestamp')
            old = f'/Sakconfig="{Path(tmp) / "tool folder" / "signtool.exe"}" sign /sha1 ' + 'A' * 40 + ' $f'
            rejected = subprocess.run([os.environ['INNO_ISCC'], '--output=no', old, str(script)],
                                      capture_output=True, text=True, errors='replace')
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn('You may not specify more than one script filename', rejected.stdout + rejected.stderr)
            result = subprocess.run([os.environ['INNO_ISCC'], '--output=no', option, str(script)],
                                    capture_output=True, text=True, errors='replace')
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


class CandidateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if ARCHIVE is None:
            raise unittest.SkipTest('Pass the Windows candidate ZIP path')
        cls.archive = ARCHIVE
        cls.version = '0.1.0-alpha.1'
        cls.commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=builder.ROOT, text=True).strip()
        with zipfile.ZipFile(cls.archive) as z:
            cls.files = {i.filename: z.read(i) for i in z.infolist()}

    def candidate(self, mutate):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        path = Path(temp.name) / self.archive.name
        files = dict(self.files)
        mutate(files)
        with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as z:
            for name, data in files.items():
                z.writestr(name, data)
        path.with_name(path.name + '.sha256').write_text(builder.sha256(path.read_bytes()) + '  ' + path.name + '\n')
        return path

    def test_unknown_path_rejected(self):
        path = self.candidate(lambda f: f.update({'../outside': b'bad'}))
        with self.assertRaisesRegex(ValueError, 'Unsafe archive member'):
            builder.read_candidate(path, self.version, self.commit, True)

    def test_source_mismatch_rejected(self):
        path = self.candidate(lambda f: f.update({next(n for n in f if n.endswith('/LICENSE')): b'modified'}))
        with self.assertRaisesRegex(ValueError, 'Source mismatch'):
            builder.read_candidate(path, self.version, self.commit, True)

    def test_wrong_architecture_rejected_even_with_updated_checksums(self):
        def mutate(files):
            root = next(n[:-len('ak-configurator.exe')] for n in files if n.endswith('/ak-configurator.exe'))
            files[root + 'ak-configurator.exe'] = b'MZ' + bytes(100)
            manifest = json.loads(files[root + 'manifest.json'])
            manifest['binary_sha256'] = builder.sha256(files[root + 'ak-configurator.exe'])
            files[root + 'manifest.json'] = json.dumps(manifest).encode()
            names = sorted(n for n in files if n != root + 'SHA256SUMS')
            files[root + 'SHA256SUMS'] = ''.join(f'{builder.sha256(files[n])}  {n[len(root):]}\n' for n in names).encode()
        path = self.candidate(mutate)
        with self.assertRaisesRegex(ValueError, 'architecture'):
            builder.read_candidate(path, self.version, self.commit, True)

    def test_existing_output_not_overwritten(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            compiler = directory / 'iscc.exe'
            compiler.write_bytes(b'compiler')
            existing = directory / f'ak-configurator-{self.version}-windows-x86_64-setup.exe'
            existing.write_bytes(b'keep')
            probe = subprocess.CompletedProcess([], 0, stdout='7.1.1\n', stderr='')
            with patch.object(builder, 'read_candidate', return_value=({}, {'source_dirty': True}, 'hash')), patch.object(builder.subprocess, 'run', return_value=probe):
                with self.assertRaisesRegex(ValueError, 'overwrite'):
                    builder.build(self.archive, compiler, directory, True)
            self.assertEqual(existing.read_bytes(), b'keep')


if __name__ == '__main__':
    unittest.main()
