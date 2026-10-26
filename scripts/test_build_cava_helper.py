"""Offline drift checks for the actual CAVA dependency resolver configuration."""
import json
from pathlib import Path
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


if __name__ == '__main__':
    unittest.main()
