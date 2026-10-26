"""Offline package boundary tests; synthetic headers are not runnable builds."""
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import tarfile
import unittest
import zipfile

import package_ak820_candidate as candidate


def header(platform):
    kind, machine = candidate.PLATFORMS[platform]
    data = bytearray(512)
    if kind == "elf":
        data[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<H", data, 18, machine)
    elif kind == "pe":
        data[:2] = b"MZ"
        struct.pack_into("<I", data, 0x3C, 128)
        data[128:132] = b"PE\0\0"
        struct.pack_into("<H", data, 132, machine)
        struct.pack_into("<H", data, 152, 0x20B)
    else:
        data[:4] = b"\xcf\xfa\xed\xfe"
        struct.pack_into("<I", data, 4, machine)
    return bytes(data)


class CandidateTests(unittest.TestCase):
    def test_wrong_architecture_and_truncated_headers_are_rejected(self):
        for platform in candidate.PLATFORMS:
            data = header(platform)
            candidate.check_architecture(data, platform)
            for other in candidate.PLATFORMS:
                if other != platform:
                    with self.assertRaises(ValueError):
                        candidate.check_architecture(data, other)
            with self.assertRaises(ValueError):
                candidate.check_architecture(data[:20], platform)

    def test_candidate_contents_identity_hashes_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "synthetic"
            binary.write_bytes(header("linux-x86_64"))
            archive = candidate.build_package(binary, "linux-x86_64", root / "out", allow_dirty=True)
            with tarfile.open(archive, "r:gz") as package:
                contents = {item.name.split("/", 1)[1]: package.extractfile(item).read() for item in package.getmembers()}
                self.assertTrue({"ak-configurator", "INSTALL.en.md", "INSTALL.ru.md", "RGB-PLAN.en.md", "RGB-PLAN.ru.md", "manifest.json", "firmware-provenance.json", "SHA256SUMS", "LICENSE", "FONT-LICENSES.txt", "LUCIDE-LICENSE.txt", "eframe-LICENSE-MIT", "eframe-LICENSE-APACHE", "offline/firmware-beta2/main/application.ihx", "offline/firmware-beta2/main/application.map", "offline/capability-catalog.json"}.issubset(contents))
                self.assertEqual(contents["LICENSE"], (candidate.ROOT / "LICENSE").read_bytes())
                manifest = json.loads(contents["manifest.json"])
                self.assertEqual(manifest["firmware_release"], "v0.6.0-beta.2")
                provenance = json.loads(contents["firmware-provenance.json"])
                self.assertEqual(provenance["tag_object"], "2e0b28b384872c482e0e358a4b353f00ddd976a7")
                self.assertEqual(provenance["tag_commit"], "2990ef877c5c6c54013b8821450c5a91878b8c3a")
                self.assertEqual(manifest["firmware_commit"], provenance["tag_commit"])
                self.assertEqual(manifest["firmware_tag_object"], provenance["tag_object"])
                self.assertEqual(manifest["binary_sha256"], hashlib.sha256(binary.read_bytes()).hexdigest())
                for line in contents["SHA256SUMS"].decode().splitlines():
                    expected, name = line.split("  ")
                    self.assertEqual(expected, hashlib.sha256(contents[name]).hexdigest())
                executable = next(item for item in package.getmembers() if item.name.endswith("/ak-configurator"))
                self.assertEqual(executable.mode & 0o777, 0o755)
            expected = Path(str(archive) + ".sha256").read_text().split()[0]
            self.assertEqual(expected, hashlib.sha256(archive.read_bytes()).hexdigest())
            with self.assertRaisesRegex(ValueError, "already exists"):
                candidate.build_package(binary, "linux-x86_64", root / "out", allow_dirty=True)


if __name__ == "__main__":
    unittest.main()
