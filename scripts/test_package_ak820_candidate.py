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
from unittest.mock import patch
import zipfile
import shutil
from urllib.parse import urlsplit

import package_ak820_candidate as candidate
from ak820_package_metadata import public_inputs, runtime_identity
from test_ak820_installed_candidate import guard_windows_smoke_paths


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
    def test_missing_document_link_stops_packaging(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "guide.md").write_text("[missing](missing.md)", encoding="utf-8")
            with patch.object(candidate, "ROOT", root), patch.object(candidate, "DOCUMENTS", {"guide.md": "guide.md"}):
                with self.assertRaisesRegex(ValueError, "Broken documentation link"):
                    candidate.package_document("guide.md", "unused")
                (root / "missing.md").write_text("Public source", encoding="utf-8")
                self.assertIn(b"source document not included in this package", candidate.package_document("guide.md", "unused"))

    def test_windows_smoke_paths_must_stay_in_fresh_root(self):
        with tempfile.TemporaryDirectory() as directory, tempfile.TemporaryDirectory() as outside:
            root = Path(directory)
            local = root / "home/AppData/Local"
            roaming = root / "home/AppData/Roaming"
            guard_windows_smoke_paths(root, local, roaming)
            with self.assertRaisesRegex(ValueError, "escapes temporary root"):
                guard_windows_smoke_paths(root, Path(outside), roaming)
            with self.assertRaisesRegex(ValueError, "escapes temporary root"):
                guard_windows_smoke_paths(root, local, Path(outside))

    def test_public_inputs_reject_tamper_truncation_and_unknown_profile(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = candidate.ROOT / "assets/ak820"
            target = root / "assets/ak820"
            target.mkdir(parents=True)
            for name in ("release-catalog.json", "capability-catalog.json", "firmware-matrix.json", "module-descriptions.json"):
                shutil.copyfile(source / ("beta5-" + name), target / ("beta5-" + name))
            shutil.copytree(source / "firmware-beta5", target / "firmware-beta5")
            public_inputs(root)
            image = target / "firmware-beta5/main/application.ihx"
            original = image.read_bytes()
            for bad in (original[:-1], bytes([original[0] ^ 1]) + original[1:]):
                image.write_bytes(bad)
                with self.assertRaises(ValueError):
                    public_inputs(root)
            image.write_bytes(original)
            extra = target / "firmware-beta5/unknown/application.ihx"
            extra.parent.mkdir()
            extra.write_bytes(original)
            with self.assertRaises(ValueError):
                public_inputs(root)

    def test_runtime_profiles_are_required_and_bound_to_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "synthetic"
            binary.write_bytes(header("linux-x86_64"))
            runtime = root / "runtime.json"
            report = {"format": "ak-configurator-self-check-1", "platform": "linux-x86_64", "status": "passed",
                      "hardware_access": False, "network_access": False,
                      "application_version": tomllib.loads((candidate.ROOT / "Cargo.toml").read_text())["package"]["version"],
                      **runtime_identity()}
            for profiles in ([], report["profiles"][:-1], report["profiles"] + [report["profiles"][0]],
                             [{**p, "default_settings_sha256": "0" * 64} if i == 0 else p for i, p in enumerate(report["profiles"])]):
                runtime.write_text(json.dumps({**report, "profiles": profiles}))
                with self.assertRaisesRegex(ValueError, "profile identities"):
                    candidate.build_package(binary, "linux-x86_64", root / "out", allow_dirty=True, runtime_check=runtime, cava_helper=binary)
            runtime.write_text(json.dumps(report))
            archive = candidate.build_package(binary, "linux-x86_64", root / "out", allow_dirty=True, runtime_check=runtime, cava_helper=binary)
            with tarfile.open(archive) as package:
                files = {x.name.split("/", 1)[1]: package.extractfile(x).read() for x in package.getmembers()}
            self.assertEqual(json.loads(files["runtime-self-check.json"])["profiles"], report["profiles"])
            self.assertEqual(json.loads(files["manifest.json"])["firmware_release"], report["firmware_release"])

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
                "firmware_release": "v0.6.0-beta.5",
                "catalog_sha256": candidate.digest((candidate.ROOT / "assets/ak820/beta5-release-catalog.json").read_bytes()),
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
                self.assertTrue({"ak-configurator", "INSTALL.en.md", "INSTALL.ru.md", "RGB-PLAN.en.md", "RGB-PLAN.ru.md", "manifest.json", "firmware-provenance.json", "SHA256SUMS", "LICENSE", "FONT-LICENSES.txt", "LUCIDE-LICENSE.txt", "eframe-LICENSE-MIT", "eframe-LICENSE-APACHE", "offline/firmware-beta5/main/application.ihx", "offline/firmware-beta5/main/application.map", "offline/capability-catalog.json"}.issubset(contents))
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
                self.assertEqual(manifest["firmware_release"], "v0.6.0-beta.5")
                provenance = json.loads(contents["firmware-provenance.json"])
                self.assertEqual(provenance["tag_object"], "1d960eabb5fd9abf078e6815b0c5c01bfb13a803")
                self.assertEqual(provenance["tag_commit"], "dfd0770f9ef64bf5c7ab6d40e9f9e7b4354cd2cd")
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
