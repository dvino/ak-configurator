#!/usr/bin/env python3
"""Offline release-verifier tests using real packages and synthetic headers."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import subprocess
import os
import plistlib
import tempfile
import tarfile
import io
import tomllib
import unittest
import zipfile

from package_ak820_candidate import PLATFORMS, build_package
from verify_ak820_release_candidate import EXPECTED, expected_runtime_identity, verify
from test_ak820_installed_candidate import extract
from test_ak820_runtime_smoke import STARTUP_PHASES

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
    archive_root = archive.name.removesuffix(".app.zip").removesuffix(".tar.gz").removesuffix(".zip") + "/"
    if archive.name.endswith(".tar.gz"):
        with tarfile.open(archive, "r:gz") as source:
            contents = {item.name[len(archive_root):]: source.extractfile(item).read() for item in source.getmembers()}
    else:
        with zipfile.ZipFile(archive) as source:
            contents = {info.filename[len(archive_root):]: source.read(info) for info in source.infolist()}
    transform(contents)
    update_internal_sums(contents)
    root = archive_root
    if archive.name.endswith(".tar.gz"):
        with tarfile.open(archive, "w:gz") as target:
            for name, data in sorted(contents.items()):
                info = tarfile.TarInfo(root + name)
                info.size = len(data)
                target.addfile(info, io.BytesIO(data))
    else:
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as target:
            for name, data in sorted(contents.items()):
                target.writestr(root + name, data)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(f"{digest}  {archive.name}\n", encoding="ascii")
    smoke_path = archive.parent / f"install-smoke-{archive_root.removeprefix('ak-configurator-' + VERSION + '-')[:-1]}.json"
    if smoke_path.exists():
        smoke = json.loads(smoke_path.read_text(encoding="utf-8"))
        smoke["archive_sha256"] = digest
        if archive.name.endswith(".tar.gz"):
            with tarfile.open(archive, "r:gz") as package:
                manifest_data = package.extractfile(archive_root + "manifest.json").read()
        else:
            with zipfile.ZipFile(archive) as package:
                manifest_data = package.read(archive_root + "manifest.json")
        smoke["binary_sha256"] = json.loads(manifest_data)["binary_sha256"]
        smoke_path.write_text(json.dumps(smoke), encoding="utf-8")


class WorkflowDiagnosticsTests(unittest.TestCase):
    def test_windows_uses_isolated_graphics_fixture_without_packaging_driver(self):
        workflow = (ROOT / ".github/workflows/desktop-dev.yml").read_text(encoding="utf-8")
        steps = {block.splitlines()[0]: block for block in workflow.split("      - name: ")[1:]}
        windows = steps["Execute native GUI runtime smoke (Windows)"]
        self.assertIn("python scripts/run_windows_ci_runtime_smoke.py", windows)
        self.assertIn('--binary "${{ matrix.binary }}"', windows)
        self.assertIn("if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }", windows)
        for name in ("Upload development binary and checksum", "Upload candidate package and checksum",
                     "Package unsigned desktop alpha candidate"):
            self.assertNotIn("opengl32", steps[name])
            self.assertNotIn("ak820-ci-gl", steps[name])

    def test_failed_runtime_reports_survive_without_publishing_candidates(self):
        workflow = (ROOT / ".github/workflows/desktop-dev.yml").read_text(encoding="utf-8")
        steps = {block.splitlines()[0]: block for block in workflow.split("      - name: ")[1:]}
        diagnostic_name = "Retain failed native GUI runtime smoke diagnostics"
        diagnostic = steps[diagnostic_name]
        self.assertIn("if: always() && (", diagnostic)
        for platform, suffix in (("Linux", " under Xvfb"), ("Windows", ""), ("macOS", "")):
            smoke = steps[f"Execute native GUI runtime smoke{suffix} ({platform})"]
            step_id = f"runtime_smoke_{platform.lower()}"
            self.assertIn(f"id: {step_id}", smoke)
            self.assertIn(f"steps.{step_id}.outcome == 'failure'", diagnostic)
            self.assertNotIn("continue-on-error", smoke)
            self.assertLess(workflow.index(smoke), workflow.index(diagnostic))
        self.assertIn("uses: actions/upload-artifact@v7", diagnostic)
        self.assertIn("name: runtime-smoke-failure-${{ matrix.platform }}", diagnostic)
        self.assertIn("path: target/candidate/runtime-smoke-${{ matrix.platform }}.json", diagnostic)
        self.assertIn("if-no-files-found: error", diagnostic)
        self.assertIn("retention-days: 7", diagnostic)
        self.assertNotIn("continue-on-error", workflow)
        for name in ("Package unsigned desktop alpha candidate", "Upload development binary and checksum",
                     "Upload candidate package and checksum"):
            # These steps retain GitHub Actions' implicit success() gate.
            self.assertNotIn("if:", steps[name])
            self.assertLess(workflow.index(diagnostic), workflow.index(steps[name]))


class ReleaseVerificationTests(unittest.TestCase):
    def test_release_workflow_only_retains_verified_artifacts(self):
        workflow = (ROOT / ".github/workflows/ak820-release.yml").read_text(encoding="utf-8")
        self.assertIn("candidate-artifacts/*.tar.gz", workflow)
        self.assertIn("actions/upload-artifact@v7", workflow)
        self.assertNotIn("gh release create", workflow)
        self.assertNotIn("contents: write", workflow)

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.directory = Path(self.temp.name)
        for platform in EXPECTED:
            binary = self.directory / ("ak-configurator.exe" if platform.startswith("windows-") else "ak-configurator")
            binary.write_bytes(executable(platform))
            runtime = self.directory / f"runtime-{platform}.json"
            runtime.write_text(json.dumps({
                "format": "ak-configurator-self-check-1", "application_version": VERSION,
                "platform": platform, **expected_runtime_identity(platform),
                "hardware_access": False, "network_access": False, "status": "passed",
            }), encoding="utf-8")
            if platform.startswith("windows-"):
                with tarfile.open(binary.parent / "cava-windows-dependency-sources.tar.gz", "w:gz") as sources:
                    for filename in ("fftw3/source/fixture/api/fftw3.h", "pthreads/source/fixture/pthread.h",
                                     "fftw3/vcpkg-port/portfile.cmake", "pthreads/vcpkg-port/portfile.cmake",
                                     "licenses-and-build-metadata/fftw3/copyright", "licenses-and-build-metadata/pthreads/copyright"):
                        member = tarfile.TarInfo(filename)
                        member.size = 7
                        sources.addfile(member, io.BytesIO(b"fixture"))
            build_package(binary, platform, self.directory, allow_dirty=True, runtime_check=runtime, cava_helper=binary)
            suffix = ".app.zip" if platform.startswith("macos-") else (".tar.gz" if platform.startswith("linux-") else ".zip")
            archive = self.directory / f"ak-configurator-{VERSION}-{platform}{suffix}"
            # Exercise the actual packager, while normalizing its local dirty-tree marker
            # to the clean checkout that a release runner builds from.
            replace_archive(archive, lambda files: files.update({
                "manifest.json": json.dumps({**json.loads(files["manifest.json"]), "source_dirty": False},
                                             indent=2, sort_keys=True).encode() + b"\n"
            }))
            report = {
                "format": "ak-configurator-installed-smoke-1", "platform": platform,
                "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
                "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "audio_helper_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "audio_helper_version": "cava 1.0.0",
                "installed_self_check": json.loads(runtime.read_text(encoding="utf-8")),
                "user_data_preserved": True, "upgrade_verified": True, "uninstalled": True,
            }
            report["installed_self_check"]["profiles"].reverse()
            (self.directory / f"install-smoke-{platform}.json").write_text(json.dumps(report), encoding="utf-8")
            if platform.startswith(("linux-", "windows-", "macos-")):
                os_name, arch, _ = {
                    "linux-x86_64": ("linux", "x86_64", ""), "linux-arm64": ("linux", "aarch64", ""),
                    "windows-x86_64": ("windows", "x86_64", ""), "windows-arm64": ("windows", "aarch64", ""),
                    "macos-x86_64": ("macos", "x86_64", ""), "macos-arm64": ("macos", "aarch64", ""),
                }[platform]
                cases = [{"name": name, "status": "passed", "detail": "fixture"} for name in
                         ("window_launch", "rgb_navigation", "settings_navigation", "render_after_navigation", "native_tray")]
                runtime_smoke = {"format": "ak820-runtime-smoke-1", "application_version": VERSION,
                                 "os": os_name, "arch": arch, "status": "passed", "rendered_frames": 16,
                                 "cases": cases, "live_hid_allowed": False, "isolated_settings": True,
                                 "startup_timings": {
                                     "unit": "milliseconds_from_process_entry", "display_fence_measured": False,
                                     "phases": [{"phase": phase, "elapsed_ms": index * 10}
                                                for index, phase in enumerate(STARTUP_PHASES)]},
                                 "physical_behavior_validated": False}
                harness_smoke = {"format": "ak820-runtime-smoke-harness-1", "status": "passed",
                                 "os": os_name, "arch": arch, "application_version": VERSION,
                                 "exit_code": 0,
                                 "executable_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                                 "runtime_smoke": runtime_smoke}
                (self.directory / f"runtime-smoke-{platform}.json").write_text(json.dumps(harness_smoke), encoding="utf-8")

    def tearDown(self):
        self.temp.cleanup()

    def test_accepts_complete_six_real_packages_and_reports_hashes(self):
        report = verify(self.directory, VERSION, COMMIT)
        self.assertEqual(set(report["platforms"]), set(PLATFORMS))
        self.assertTrue(all(len(item["sha256"]) == 64 for item in report["platforms"].values()))
        self.assertIn("unsigned desktop trial", report["status"])
        index = report["app_update_index"]
        self.assertEqual(set(index), {"schema_version", "tag", "version", "prerelease", "assets"})
        self.assertEqual(index["schema_version"], 1)
        self.assertEqual(index["tag"], f"v{VERSION}")
        self.assertTrue(index["prerelease"])
        self.assertEqual({asset["name"] for asset in index["assets"]}, {item["archive"] for item in report["platforms"].values()})
        for asset in index["assets"]:
            self.assertEqual(set(asset), {"name", "target", "platform", "arch", "url", "sha256", "size"})
            self.assertEqual(asset["url"], f"https://github.com/dvino/ak-configurator/releases/download/v{VERSION}/{asset['name']}")
            self.assertEqual(asset["size"], (self.directory / asset["name"]).stat().st_size)

    def test_package_contains_platform_install_and_uninstall_helpers(self):
        for platform in EXPECTED:
            suffix = ".app.zip" if platform.startswith("macos-") else (".tar.gz" if platform.startswith("linux-") else ".zip")
            archive = self.directory / f"ak-configurator-{VERSION}-{platform}{suffix}"
            with (tarfile.open(archive, "r:gz") if suffix == ".tar.gz" else zipfile.ZipFile(archive)) as package:
                names = package.getnames() if suffix == ".tar.gz" else package.namelist()
                if platform.startswith("macos-"):
                    plist_path = next(name for name in names if name.endswith("/AK-Configurator.app/Contents/Info.plist"))
                    plist_bytes = package.extractfile(plist_path).read() if suffix == ".tar.gz" else package.read(plist_path)
                    plist = plistlib.loads(plist_bytes)
                    self.assertRegex(plist["CFBundleShortVersionString"], r"^\d+\.\d+\.\d+$")
                    self.assertIn("CFBundleVersion", plist)
                    self.assertEqual(plist["LSMinimumSystemVersion"], "11.0" if platform == "macos-arm64" else "10.15")
            ending = ".ps1" if platform.startswith("windows-") else ".sh"
            for action in ("install-user", "uninstall-user"):
                self.assertTrue(any(name.endswith(f"/{action}{ending}") for name in names), (platform, action))

    def test_linux_per_user_install_and_uninstall_keep_user_data(self):
        root = self.directory / "linux-fixture"
        root.mkdir()
        binary = root / "ak-configurator"
        binary.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        binary.chmod(0o755)
        (root / "audio").mkdir()
        (root / "audio/cava").write_bytes(b"synthetic helper")
        (root / "manifest.json").write_text(json.dumps({
            "format": "ak-configurator-desktop-candidate-1", "application_version": VERSION,
            "platform": "linux-x86_64", "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        }), encoding="utf-8")
        (root / "install-user.sh").write_bytes((ROOT / "scripts/ak820-release/linux/install-user.sh").read_bytes())
        (root / "uninstall-user.sh").write_bytes((ROOT / "scripts/ak820-release/linux/uninstall-user.sh").read_bytes())
        sums = "".join(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(root).as_posix()}\n"
                       for path in sorted(root.rglob("*")) if path.is_file() and path.name != "SHA256SUMS")
        (root / "SHA256SUMS").write_text(sums, encoding="ascii")
        home = self.directory / "per-user"
        env = os.environ.copy()
        env.update(HOME=str(home), XDG_DATA_HOME=str(home / ".local/share"), XDG_BIN_HOME=str(home / ".local/bin"))
        saved_manifest = (root / "manifest.json").read_bytes()
        (root / "manifest.json").write_text("{}", encoding="utf-8")
        failed = subprocess.run(["sh", str(root / "install-user.sh")], env=env, capture_output=True, text=True)
        self.assertNotEqual(failed.returncode, 0)
        self.assertFalse((home / ".local/share/ak-configurator/current").exists())
        (root / "manifest.json").write_bytes(saved_manifest)
        subprocess.run(["sh", str(root / "install-user.sh")], env=env, check=True, capture_output=True, text=True)
        self.assertTrue((home / ".local/bin/ak-configurator").is_symlink())
        self.assertTrue((home / ".local/share/ak-configurator/current/ak-configurator").is_file())
        self.assertTrue((home / ".local/share/ak-configurator/current/uninstall-user.sh").is_file())
        first_binary = (home / ".local/share/ak-configurator/current/ak-configurator").read_bytes()
        binary.write_text("#!/bin/sh\necho updated\n", encoding="utf-8")
        binary.chmod(0o755)
        manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
        manifest["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
        (root / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        sums = "".join(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(root).as_posix()}\n"
                       for path in sorted(root.rglob("*")) if path.is_file() and path.name != "SHA256SUMS")
        (root / "SHA256SUMS").write_text(sums, encoding="ascii")
        subprocess.run(["sh", str(root / "install-user.sh")], env=env, check=True, capture_output=True, text=True)
        self.assertNotEqual((home / ".local/share/ak-configurator/current/ak-configurator").read_bytes(), first_binary)
        user_data = home / ".local/share/config/ak-configurator/preferences.json"
        user_data.parent.mkdir(parents=True)
        user_data.write_text("{}", encoding="utf-8")
        subprocess.run(["sh", str(root / "uninstall-user.sh")], input="n\n", env=env,
                       check=True, capture_output=True, text=True)
        self.assertFalse((home / ".local/bin/ak-configurator").exists())
        self.assertTrue((home / ".local/share/ak-configurator/current/ak-configurator").exists())
        self.assertTrue(user_data.is_file())
        subprocess.run(["sh", str(home / ".local/share/ak-configurator/current/uninstall-user.sh")], input="y\n", env=env,
                       check=True, capture_output=True, text=True)
        self.assertFalse((home / ".local/share/ak-configurator/current").exists())
        self.assertTrue(user_data.is_file())

    def test_linux_install_refuses_unmanaged_launcher_without_touching_it(self):
        root = self.directory / "linux-unmanaged"
        root.mkdir()
        binary = root / "ak-configurator"
        binary.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        binary.chmod(0o755)
        manifest = {"format": "ak-configurator-desktop-candidate-1", "application_version": VERSION,
                    "platform": "linux-x86_64", "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
        (root / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        (root / "install-user.sh").write_bytes((ROOT / "scripts/ak820-release/linux/install-user.sh").read_bytes())
        (root / "uninstall-user.sh").write_bytes((ROOT / "scripts/ak820-release/linux/uninstall-user.sh").read_bytes())
        (root / "SHA256SUMS").write_text("".join(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(root).as_posix()}\n"
                                                 for path in sorted(root.iterdir())), encoding="ascii")
        home = self.directory / "unmanaged-home"
        launcher_dir = home / ".local/bin"
        launcher_dir.mkdir(parents=True)
        outside = self.directory / "outside-launcher"
        outside.write_text("keep", encoding="utf-8")
        launcher = launcher_dir / "ak-configurator"
        launcher.symlink_to(outside)
        env = os.environ.copy()
        env.update(HOME=str(home), XDG_DATA_HOME=str(home / ".local/share"), XDG_BIN_HOME=str(launcher_dir))
        result = subprocess.run(["sh", str(root / "install-user.sh")], env=env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(launcher.resolve(), outside)
        self.assertFalse((home / ".local/share/ak-configurator/current").exists())

    def test_installed_smoke_extractor_rejects_windows_drive_ads_and_links(self):
        destination = self.directory / "extract-target"
        destination.mkdir()
        for member in ("C:outside", "file:stream"):
            archive = self.directory / f"bad-{member.replace(':', '-')}.zip"
            with zipfile.ZipFile(archive, "w") as package:
                package.writestr(member, "bad")
            with self.assertRaisesRegex(ValueError, "Unsafe package path"):
                extract(archive, destination)
        link_archive = self.directory / "linked.tar.gz"
        with tarfile.open(link_archive, "w:gz") as package:
            link = tarfile.TarInfo("bundle/link")
            link.type = tarfile.SYMTYPE
            link.linkname = "../../outside"
            package.addfile(link)
        with self.assertRaisesRegex(ValueError, "Unsupported archive member type"):
            extract(link_archive, destination)

    def test_rejects_missing_platform(self):
        (self.directory / f"ak-configurator-{VERSION}-macos-arm64.app.zip").unlink()
        with self.assertRaisesRegex(ValueError, "Missing candidate"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_missing_gui_runtime_smoke_evidence(self):
        (self.directory / "runtime-smoke-linux-x86_64.json").unlink()
        with self.assertRaisesRegex(ValueError, "Missing or malformed native GUI runtime smoke"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_tampered_gui_runtime_smoke_executable_binding(self):
        path = self.directory / "runtime-smoke-windows-x86_64.json"
        report = json.loads(path.read_text(encoding="utf-8"))
        report["executable_sha256"] = "0" * 64
        path.write_text(json.dumps(report), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "GUI runtime smoke identity"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_missing_startup_feedback_evidence(self):
        path = self.directory / "runtime-smoke-linux-x86_64.json"
        report = json.loads(path.read_text(encoding="utf-8"))
        report["runtime_smoke"].pop("startup_timings")
        path.write_text(json.dumps(report), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "Startup timing evidence"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_editor_initialization_before_splash(self):
        path = self.directory / "runtime-smoke-windows-x86_64.json"
        report = json.loads(path.read_text(encoding="utf-8"))
        phases = report["runtime_smoke"]["startup_timings"]["phases"]
        phases[1]["phase"], phases[2]["phase"] = phases[2]["phase"], phases[1]["phase"]
        path.write_text(json.dumps(report), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "Editor initialized before startup feedback"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_wrong_platform_manifest(self):
        archive = self.directory / f"ak-configurator-{VERSION}-windows-x86_64.zip"
        def mutate(files):
            manifest = json.loads(files["manifest.json"])
            manifest["platform"] = "linux-arm64"
            files["manifest.json"] = json.dumps(manifest).encode()
        replace_archive(archive, mutate)
        with self.assertRaisesRegex(ValueError, "Wrong platform"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_version_mismatch(self):
        archive = self.directory / f"ak-configurator-{VERSION}-windows-x86_64.zip"
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
        archive = self.directory / f"ak-configurator-{VERSION}-windows-x86_64.zip"
        def mutate(files):
            binary = executable("linux-arm64")
            files["ak-configurator.exe"] = binary
            manifest = json.loads(files["manifest.json"])
            manifest["binary_sha256"] = hashlib.sha256(binary).hexdigest()
            files["manifest.json"] = json.dumps(manifest).encode()
        replace_archive(archive, mutate)
        with self.assertRaisesRegex(ValueError, "does not match"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_missing_package_member(self):
        archive = self.directory / f"ak-configurator-{VERSION}-windows-x86_64.zip"
        replace_archive(archive, lambda files: files.pop("INSTALL.en.md"))
        with self.assertRaisesRegex(ValueError, "missing or unexpected members"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_changed_bundled_firmware_provenance(self):
        archive = self.directory / f"ak-configurator-{VERSION}-windows-x86_64.zip"
        replace_archive(archive, lambda files: files.update({"firmware-provenance.json": b"{}"}))
        with self.assertRaisesRegex(ValueError, "Bundled source file differs"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_corrupt_internal_checksum_manifest(self):
        archive = self.directory / f"ak-configurator-{VERSION}-windows-x86_64.zip"
        with zipfile.ZipFile(archive) as source:
            entries = [(info.filename, source.read(info)) for info in source.infolist()]
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as target:
            for name, data in entries:
                target.writestr(name, b"" if name.endswith("/SHA256SUMS") else data)
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        archive.with_name(archive.name + ".sha256").write_text(f"{digest}  {archive.name}\n", encoding="ascii")
        with self.assertRaisesRegex(ValueError, "does not cover|Malformed entry"):
            verify(self.directory, VERSION, COMMIT)

    def test_rejects_runtime_self_check_for_wrong_firmware_profile(self):
        archive = self.directory / f"ak-configurator-{VERSION}-windows-x86_64.zip"
        def mutate(files):
            runtime = json.loads(files["runtime-self-check.json"])
            runtime["profiles"][0]["ihx_sha256"] = "0" * 64
            files["runtime-self-check.json"] = json.dumps(runtime).encode()
        replace_archive(archive, mutate)
        with self.assertRaisesRegex(ValueError, "Runner self-check identity"):
            verify(self.directory, VERSION, COMMIT)


if __name__ == "__main__":
    unittest.main()
