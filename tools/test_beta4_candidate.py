import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import check_beta4_candidate as candidate


class CandidateContractTest(unittest.TestCase):
    def test_exact_candidate_is_blocked_by_stale_capability_images(self):
        with self.assertRaisesRegex(ValueError, r'mismatch: 10/10; match historical beta.3: 10/10'):
            candidate.check(candidate.ROOT / 'assets/beta4-candidate')

    def test_modified_input_is_rejected_before_parsing(self):
        with tempfile.TemporaryDirectory() as path:
            directory = Path(path)
            for name in candidate.FILES:
                (directory / name).write_bytes((candidate.ROOT / 'assets/beta4-candidate' / name).read_bytes())
            (directory / 'firmware-matrix.json').write_bytes(b'{}')
            with self.assertRaisesRegex(ValueError, 'SHA256 differs'):
                candidate.check(directory)

    def test_transition_matrix_rejects_missing_or_extra_pair(self):
        import json, hashlib
        with tempfile.TemporaryDirectory() as path:
            directory = Path(path)
            for name in candidate.FILES:
                (directory / name).write_bytes((candidate.ROOT / 'assets/beta4-candidate' / name).read_bytes())
            matrix_path = directory / 'firmware-matrix.json'
            matrix = json.loads(matrix_path.read_bytes())
            matrix['settings_transitions'][0]['target'] = 'unknown'
            raw = json.dumps(matrix).encode()
            matrix_path.write_bytes(raw)
            digests = dict(candidate.FILES, **{'firmware-matrix.json': hashlib.sha256(raw).hexdigest()})
            with patch.object(candidate, 'FILES', digests):
                with self.assertRaisesRegex(ValueError, 'transition matrix differs'):
                    candidate.check(directory)


if __name__ == '__main__':
    unittest.main()
