#!/usr/bin/env python3
"""Offline release-verifier tests using real packages and synthetic headers."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import subprocess
import tempfile
import tomllib
import unittest
import zipfile

from package_ak820_candidate import PLATFORMS, build_package
from verify_ak820_release_candidate import EXPECTED, verify

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
COMMIT = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def executable(platform):
    kind, machine = PLATFORMS[platform]
    if kind == "elf":
        data = bytearray(64)
        data[:6] = b"\x7fELF\x02\x01"
        data[18:20] = machine.to_bytes(2, "little")
        return bytes(data)
    if kind == "pe":
        data = bytearray(128)
        data[:2] = b"MZ"
        data[0x3c:0x40] = (64).to_bytes(4, "little")
        data[64:68] = b"PE\0\0"
        data[68:70] = machine.to_bytes(2, "little")
        data[88:90] = (0x20b).to_bytes(2, "little")
        return bytes(data)
    data = bytearray(32)
    data[:4] = b"\xcf\xfa\xed\xfe"
    data[4:8] = machine.to_bytes(4, "little")
    return bytes(data)


def update_internal_sums(contents):
    contents["SHA256SUMS"] = "".join(
        f"{hashlib.sha256(value).hexdigest()}  {name}\n"
        for name, value in sorted(contents.items()) if name != "SHA256SUMS"
    ).encode("ascii")


def replace_archive(archive, transform):
    with zipfile.ZipFile(archive) as source:
        contents = {PurePosixPath(info.filename).name: source.read(info) for info in source.infolist()}
    transform(contents)
    update_internal_sums(contents)
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as target:
        root = archive.stem + "/"
        for name, data in sorted(contents.items()):
            target.writestr(root + name, data)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(f"{digest}  {archive.name}\n", encoding="ascii")


class ReleaseVerificationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.directory = Path(self.temp.name)
        for platform in EXPECTED:
            binary = self.directory / ("ak-configurator.exe" if platform.startswith("windows-") else "ak-configurator")
            binary.write_bytes(executable(platform))
            build_package(binary, platform, self.directory, allow_dirty=True)
            archive = self.directory / f"ak-configurator-{VERSION}-{platform}.zip"
            # Exercise the actual packager, while normalizing its local dirty-tree marker
            # to the clean checkout that a release runner builds from.
            replace_archive(archive, lambda files: files.update({
                "manifest.json": json.dumps({**json.loads(files["manifest.json"]), "source_dirty": False},
                                             indent=2, sort_keys=True).encode() + b"\n"
            }))

    def tearDown(self):
        self.temp.cleanup()

    def test_accepts_complete_six_real_packages_and_reports_hashes(self):
        report = verify(self.directory, VERSION, COMMIT)
        self.assertEqual(set(report["platforms"]), set(PLATFORMS))
        self.assertTrue(all(len(item["sha256"]) == 64 for item in report["platforms"].values()))
        self.assertIn("unsigned desktop trial", report["status"])

    def test_rejects_missing_platform(self):
        (self.directory / f"ak-configurator-{VERSION}-macos-arm64.zip").unlink()
        with self.assertRaisesRegex(ValueError, "Missing candidate"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_wrong_platform_manifest(self):
        archive = self.directory / f"ak-configurator-{VERSION}-linux-x86_64.zip"
        def mutate(files):
            manifest = json.loads(files["manifest.json"])
            manifest["platform"] = "linux-arm64"
            files["manifest.json"] = json.dumps(manifest).encode()
        replace_archive(archive, mutate)
        with self.assertRaisesRegex(ValueError, "Wrong platform"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_version_mismatch(self):
        archive = self.directory / f"ak-configurator-{VERSION}-linux-x86_64.zip"
        def mutate(files):
            manifest = json.loads(files["manifest.json"])
            manifest["application_version"] = "9.9.9"
            files["manifest.json"] = json.dumps(manifest).encode()
        replace_archive(archive, mutate)
        with self.assertRaisesRegex(ValueError, "Wrong application version"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_source_commit_mismatch(self):
        with self.assertRaisesRegex(ValueError, "source commit"):
            verify(self.directory, VERSION, "b" * 40)

    def test_rejects_wrong_architecture_even_when_hashes_match(self):
        archive = self.directory / f"ak-configurator-{VERSION}-linux-x86_64.zip"
        def mutate(files):
            binary = executable("linux-arm64")
            files["ak-configurator"] = binary
            manifest = json.loads(files["manifest.json"])
            manifest["binary_sha256"] = hashlib.sha256(binary).hexdigest()
            files["manifest.json"] = json.dumps(manifest).encode()
        replace_archive(archive, mutate)
        with self.assertRaisesRegex(ValueError, "does not match"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_missing_package_member(self):
        archive = self.directory / f"ak-configurator-{VERSION}-linux-x86_64.zip"
        replace_archive(archive, lambda files: files.pop("INSTALL.en.md"))
        with self.assertRaisesRegex(ValueError, "missing or unexpected members"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_changed_bundled_firmware_provenance(self):
        archive = self.directory / f"ak-configurator-{VERSION}-linux-x86_64.zip"
        replace_archive(archive, lambda files: files.update({"firmware-provenance.json": b"{}"}))
        with self.assertRaisesRegex(ValueError, "Bundled source file differs"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_corrupt_internal_checksum_manifest(self):
        archive = self.directory / f"ak-configurator-{VERSION}-linux-x86_64.zip"
        with zipfile.ZipFile(archive) as source:
            entries = [(info.filename, source.read(info)) for info in source.infolist()]
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as target:
            for name, data in entries:
                target.writestr(name, b"" if name.endswith("/SHA256SUMS") else data)
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        archive.with_name(archive.name + ".sha256").write_text(f"{digest}  {archive.name}\n", encoding="ascii")
        with self.assertRaisesRegex(ValueError, "does not cover|Malformed entry"):
            verify(self.directory, VERSION, COMMIT)


if __name__ == "__main__":
    unittest.main()
