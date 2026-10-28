#!/usr/bin/env python3
"""Offline CI fixture contracts; these tests never launch a native application."""
import hashlib
import io
import json
from pathlib import Path
import tempfile
import subprocess
import sys
import time
import unittest
from unittest.mock import patch, Mock

import run_windows_ci_runtime_smoke as fixture


def pe(machine):
    data = bytearray(128)
    data[:2] = b"MZ"
    data[0x3c:0x40] = (64).to_bytes(4, "little")
    data[64:70] = b"PE\0\0" + machine.to_bytes(2, "little")
    return bytes(data)


class WindowsGraphicsFixtureTests(unittest.TestCase):
    def test_release_pins_have_exact_native_architectures(self):
        self.assertEqual(fixture.VERSION, "26.2.5")
        self.assertEqual(fixture.PINS, {
            "aarch64": ("arm64", "e7df75398d50e07a68bbffa5cff2ece4c85e5f8ef8bda497f560dd04fe60cbbd", 0xaa64),
            "x86_64": ("x64", "d1b713ca2a7fba47a50c058099f13c435ddb76a16a422c334e4d787f9755a31d", 0x8664),
        })

    def test_download_rejects_modified_or_oversized_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            for expected, limit, message in (("0" * 64, 100, "SHA-256"),
                                              (hashlib.sha256(b"archive").hexdigest(), 3, "size limit")):
                path = Path(directory) / f"archive-{limit}"
                with patch.object(fixture.urllib.request, "urlopen", return_value=io.BytesIO(b"archive")), \
                     patch.object(fixture, "ARCHIVE_LIMIT", limit):
                    with self.assertRaisesRegex(ValueError, message):
                        fixture.download("https://example.invalid", path, expected)

    def test_only_exact_root_dll_streams_to_fixed_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "opengl32.dll"
            archive = Path(directory) / "fixture.7z"
            proc = Mock(stdout=io.BytesIO(pe(0xaa64)))
            proc.wait.return_value = proc.poll.return_value = 0
            with patch.object(fixture.subprocess, "Popen", return_value=proc) as spawn:
                fixture.extract_driver(archive, target, 0xaa64, "7z")
            self.assertEqual(spawn.call_args.args[0], ["7z", "e", "-so", str(archive), "opengl32.dll"])
            self.assertEqual(target.read_bytes(), pe(0xaa64))
            with self.assertRaisesRegex(ValueError, "architecture"):
                fixture.verify_pe(target, 0x8664)

    def test_oversized_extraction_stops_child(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Mock(stdout=io.BytesIO(b"oversized"))
            proc.poll.return_value = None
            proc.kill.side_effect = lambda: setattr(proc.poll, "return_value", -9)
            with patch.object(fixture.subprocess, "Popen", return_value=proc), patch.object(fixture, "DLL_LIMIT", 2):
                with self.assertRaisesRegex(ValueError, "size limit"):
                    fixture.extract_driver(Path("archive"), Path(directory) / "driver", 0xaa64, "7z")
            proc.kill.assert_called_once()

    def test_hung_child_is_killed_and_reported_without_launching_application(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "app.exe"
            original.write_bytes(pe(0x8664))
            report = root / "report.json"
            children = []
            popen = subprocess.Popen

            def hang(*args, **kwargs):
                # A real child keeps stdout open but produces no output.
                child = popen([sys.executable, "-c", "import time; time.sleep(30)"], **kwargs)
                children.append(child)
                return child

            started = time.monotonic()
            with patch.object(fixture, "download"), patch.object(fixture, "EXTRACT_TIMEOUT", 0.1), \
                 patch.object(fixture.subprocess, "Popen", side_effect=hang), patch.object(fixture, "run") as smoke:
                self.assertFalse(fixture.execute(original, report, "x86_64", root, "7z"))
            self.assertLess(time.monotonic() - started, 3)
            self.assertIsNotNone(children[0].poll())
            smoke.assert_not_called()
            result = json.loads(report.read_text())
            self.assertEqual(result["status"], "failed")
            self.assertIn("extraction exceeded", result["error"])
            self.assertFalse(list(root.glob("ak820-ci-gl-*")))

    def exercise(self, smoke_passed):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "package" / "ak-configurator.exe"
            original.parent.mkdir()
            original.write_bytes(pe(0xaa64))
            report = root / "report.json"
            runner_temp = root / "runner-temp"
            runner_temp.mkdir()
            seen = []

            def download(url, path, expected):
                self.assertIn("/26.2.5/mesa-llvmpipe-arm64-26.2.5.7z", url)
                self.assertEqual(expected, fixture.PINS["aarch64"][1])
                path.write_bytes(b"archive")

            def extract(archive, target, machine, seven_zip):
                self.assertEqual(machine, 0xaa64)
                target.write_bytes(pe(machine))

            def smoke(copy, output):
                seen.append(copy)
                self.assertTrue(copy.is_relative_to(runner_temp))
                self.assertNotEqual(copy, original)
                self.assertEqual(copy.read_bytes(), original.read_bytes())
                self.assertTrue(copy.with_name("opengl32.dll").is_file())
                self.assertFalse(original.with_name("opengl32.dll").exists())
                output.write_text(json.dumps({"status": "passed" if smoke_passed else "failed",
                                             "executable_sha256": fixture.digest(copy), "error": "fixture"}))
                return smoke_passed

            with patch.object(fixture, "download", side_effect=download), \
                 patch.object(fixture, "extract_driver", side_effect=extract), patch.object(fixture, "run", side_effect=smoke):
                self.assertEqual(fixture.execute(original, report, "aarch64", runner_temp, "7z"), smoke_passed)
            self.assertFalse(seen[0].exists())
            self.assertEqual(list(original.parent.iterdir()), [original])
            result = json.loads(report.read_text())
            self.assertEqual(result["status"], "passed" if smoke_passed else "failed")
            provenance = result["ci_graphics_fixture"]
            self.assertEqual(provenance["requested_driver"], "Mesa llvmpipe")
            self.assertNotIn("renderer", provenance)
            self.assertFalse(provenance["packaged_driver"])
            self.assertFalse(provenance["hardware_gpu_validated"])
            self.assertEqual(provenance["original_executable_sha256"], result["executable_sha256"])
            self.assertEqual(provenance["copied_executable_sha256"], result["executable_sha256"])

    def test_success_retains_hash_binding_and_no_packaged_dll(self):
        self.exercise(True)

    def test_smoke_failure_remains_failed_with_provenance_and_cleanup(self):
        self.exercise(False)

    def test_archive_integrity_failure_never_launches_smoke(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "app.exe"
            original.write_bytes(pe(0x8664))
            report = root / "report.json"
            with patch.object(fixture, "download", side_effect=ValueError("SHA-256 mismatch")), \
                 patch.object(fixture, "run") as smoke:
                self.assertFalse(fixture.execute(original, report, "x86_64", root, "7z"))
            smoke.assert_not_called()
            self.assertEqual(json.loads(report.read_text())["status"], "failed")
            self.assertFalse(list(root.glob("ak820-ci-gl-*")))

    def test_changed_copy_never_launches_and_always_cleans_up(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "app.exe"
            original.write_bytes(pe(0x8664))
            report = root / "report.json"
            with patch.object(fixture, "download"), patch.object(fixture, "extract_driver"), \
                 patch.object(fixture.shutil, "copyfile", side_effect=lambda source, target: target.write_bytes(b"changed")), \
                 patch.object(fixture, "run") as smoke:
                self.assertFalse(fixture.execute(original, report, "x86_64", root, "7z"))
            smoke.assert_not_called()
            self.assertIn("differs from production", json.loads(report.read_text())["error"])
            self.assertEqual(original.read_bytes(), pe(0x8664))
            self.assertFalse(list(root.glob("ak820-ci-gl-*")))

    def test_malformed_or_wrong_native_input_stops_before_download(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "app.exe"
            for data in (b"not PE", pe(0xaa64)):
                original.write_bytes(data)
                with patch.object(fixture, "download") as download, patch.object(fixture, "run") as smoke:
                    self.assertFalse(fixture.execute(original, root / "report.json", "x86_64", root, "7z"))
                download.assert_not_called()
                smoke.assert_not_called()


if __name__ == "__main__":
    unittest.main()
