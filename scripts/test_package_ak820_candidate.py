"""Offline package boundary tests; synthetic headers are not runnable builds."""
import hashlib
import json
from pathlib import Path
import struct
import re
import tempfile
import tarfile
import tomllib
import unittest
import zipfile
from urllib.parse import urlsplit

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

    def test_missing_helper_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "CAVA helper is required"):
            candidate.build_package("unused", "linux-x86_64", "unused")

    def test_runtime_report_must_belong_to_this_version_and_catalog(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "synthetic"
            binary.write_bytes(header("linux-x86_64"))
            runtime = root / "runtime.json"
            expected = {
                "format": "ak-configurator-self-check-1",
                "platform": "linux-x86_64", "status": "passed",
                "hardware_access": False, "network_access": False,
                "application_version": tomllib.loads((candidate.ROOT / "Cargo.toml").read_text())["package"]["version"],
                "firmware_release": "v0.6.0-beta.3",
                "catalog_sha256": candidate.digest((candidate.ROOT / "assets/ak820/beta3-capability-catalog.json").read_bytes()),
            }
            for field, wrong in (("application_version", "0.1.0-beta.2"),
                                 ("firmware_release", "v0.6.0-beta.2"),
                                 ("catalog_sha256", "0" * 64)):
                with self.subTest(field=field):
                    runtime.write_text(json.dumps({**expected, field: wrong}))
                    with self.assertRaisesRegex(ValueError, "different application or firmware catalog"):
                        candidate.build_package(binary, "linux-x86_64", root / "out", allow_dirty=True, runtime_check=runtime, cava_helper=binary)
                    self.assertFalse((root / "out").exists())
            runtime.write_text("[]")
            with self.assertRaisesRegex(ValueError, "Runtime self-check"):
                candidate.build_package(binary, "linux-x86_64", root / "out", allow_dirty=True, runtime_check=runtime, cava_helper=binary)

    def test_candidate_contents_identity_hashes_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "synthetic"
            binary.write_bytes(header("linux-x86_64"))
            archive = candidate.build_package(binary, "linux-x86_64", root / "out", allow_dirty=True, cava_helper=binary)
            with tarfile.open(archive, "r:gz") as package:
                contents = {item.name.split("/", 1)[1]: package.extractfile(item).read() for item in package.getmembers()}
                self.assertTrue({"ak-configurator", "INSTALL.en.md", "INSTALL.ru.md", "RGB-PLAN.en.md", "RGB-PLAN.ru.md", "manifest.json", "firmware-provenance.json", "SHA256SUMS", "LICENSE", "FONT-LICENSES.txt", "LUCIDE-LICENSE.txt", "eframe-LICENSE-MIT", "eframe-LICENSE-APACHE", "offline/firmware-beta3/main/application.ihx", "offline/firmware-beta3/main/application.map", "offline/capability-catalog.json"}.issubset(contents))
                self.assertEqual(contents["audio/cava"], binary.read_bytes())
                self.assertIn("audio-sources/fftw-3.3.10.tar.gz", contents)
                self.assertIn("audio-sources/cava-patched-source.tar.gz", contents)
                self.assertEqual(contents["LICENSE"], (candidate.ROOT / "LICENSE").read_bytes())
                for name in candidate.DOCUMENTS:
                    self.assertIn(name, contents)
                    for target in re.findall(r"\[[^\]\n]*\]\(([^\s)]+)\)", contents[name].decode("utf-8")):
                        link = urlsplit(target)
                        if not link.scheme and not link.netloc and link.path:
                            self.assertIn(link.path, contents, (name, target))
                self.assertIn(b"(INSTALL.en.md)", contents["DESKTOP.en.md"])
                self.assertIn(b"(INSTALL.ru.md)", contents["DESKTOP.ru.md"])
                self.assertIn(b"(audio-sources/REBUILD.md)", contents["THIRD-PARTY-AUDIO.en.md"])
                manifest = json.loads(contents["manifest.json"])
                self.assertEqual(manifest["audio_helper"]["sha256"], hashlib.sha256(binary.read_bytes()).hexdigest())
                self.assertEqual(manifest["audio_helper"]["path"], "audio/cava")
                self.assertEqual(manifest["firmware_release"], "v0.6.0-beta.3")
                provenance = json.loads(contents["firmware-provenance.json"])
                self.assertEqual(provenance["tag_object"], "4cabae9af9bb68492a8cf079c59f8b388c5cf788")
                self.assertEqual(provenance["tag_commit"], "88d190abb156960915a0f1f8ac7f602bb401eab3")
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
                candidate.build_package(binary, "linux-x86_64", root / "out", allow_dirty=True, cava_helper=binary)


if __name__ == "__main__":
    unittest.main()
