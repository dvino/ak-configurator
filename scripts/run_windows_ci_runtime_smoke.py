#!/usr/bin/env python3
"""CI-only software OpenGL fixture; never add its DLL to a candidate package."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import threading
import time
import urllib.request

from test_ak820_runtime_smoke import digest, expected_arch, expected_os, run

VERSION = "26.2.5"
SOURCE_COMMIT = "cb4cc458eff7c4b4b31d2623edd954d66201b88f"
PINS = {
    "aarch64": ("arm64", "e7df75398d50e07a68bbffa5cff2ece4c85e5f8ef8bda497f560dd04fe60cbbd", 0xaa64),
    "x86_64": ("x64", "d1b713ca2a7fba47a50c058099f13c435ddb76a16a422c334e4d787f9755a31d", 0x8664),
}
ARCHIVE_LIMIT = 32 * 1024 * 1024
DLL_LIMIT = 128 * 1024 * 1024
EXTRACT_TIMEOUT = 30


def download(url, path, expected):
    total = 0
    checksum = hashlib.sha256()
    with urllib.request.urlopen(url, timeout=60) as response, path.open("xb") as output:
        while block := response.read(1024 * 1024):
            total += len(block)
            if total > ARCHIVE_LIMIT:
                raise ValueError("Mesa archive exceeds CI size limit")
            checksum.update(block)
            output.write(block)
    if checksum.hexdigest() != expected:
        raise ValueError("Mesa archive SHA-256 mismatch")


def verify_pe(path, machine):
    with path.open("rb") as stream:
        header = stream.read(64)
        if len(header) != 64 or header[:2] != b"MZ":
            raise ValueError("CI graphics input has no PE header")
        offset = struct.unpack_from("<I", header, 0x3c)[0]
        if offset > path.stat().st_size - 6:
            raise ValueError("CI graphics input has invalid PE offset")
        stream.seek(offset)
        if stream.read(6) != b"PE\0\0" + machine.to_bytes(2, "little"):
            raise ValueError("CI graphics input architecture does not match native host")


def extract_driver(archive, destination, machine, seven_zip):
    # Exact root member streamed to a fixed destination: no archive path, link,
    # directory or executable is ever extracted into the filesystem.
    proc = subprocess.Popen([seven_zip, "e", "-so", str(archive), "opengl32.dll"],
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    errors = []

    def consume():
        try:
            total = 0
            with destination.open("xb") as output:
                while block := proc.stdout.read(1024 * 1024):
                    total += len(block)
                    if total > DLL_LIMIT:
                        raise ValueError("Mesa DLL exceeds CI size limit")
                    output.write(block)
        except (OSError, ValueError) as error:
            errors.append(error)
            if proc.poll() is None:
                proc.kill()

    # Drain with a bounded writer while the main thread enforces the deadline
    # over the entire child lifetime, including a stall before EOF on stdout.
    reader = threading.Thread(target=consume, daemon=True)
    deadline = time.monotonic() + EXTRACT_TIMEOUT
    reader.start()
    try:
        returncode = proc.wait(timeout=EXTRACT_TIMEOUT)
        reader.join(timeout=max(0, deadline - time.monotonic()))
        if reader.is_alive():
            raise ValueError("Mesa extraction stdout exceeded its deadline")
        if errors:
            raise errors[0]
        if returncode != 0:
            raise ValueError("Could not extract pinned Mesa root opengl32.dll")
    except subprocess.TimeoutExpired as error:
        raise ValueError(f"Mesa extraction exceeded {EXTRACT_TIMEOUT:g}s deadline") from error
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait(timeout=5)
        reader.join(timeout=5)
        if not reader.is_alive():
            proc.stdout.close()
    verify_pe(destination, machine)


def execute(binary, report, arch, runner_temp, seven_zip):
    binary, report = binary.resolve(strict=True), report.absolute()
    distribution, archive_sha, machine = PINS[arch]
    asset = f"mesa-llvmpipe-{distribution}-{VERSION}.7z"
    url = f"https://github.com/mmozeiko/build-mesa/releases/download/{VERSION}/{asset}"
    provenance = {"scope": "ci_only_software_opengl", "requested_driver": "Mesa llvmpipe",
                  "application_renderer": "production Glow/WGL",
                  "distribution": "mmozeiko/build-mesa", "version": VERSION, "source_commit": SOURCE_COMMIT,
                  "archive_url": url, "archive_sha256": archive_sha,
                  "native_arch": arch, "hardware_gpu_validated": False,
                  "packaged_driver": False, "driver_selection": "adjacent_wgl_dll",
                  "driver_load_independently_verified": False}
    report.parent.mkdir(parents=True, exist_ok=True)
    try:
        verify_pe(binary, machine)
        original_sha = digest(binary)
        with tempfile.TemporaryDirectory(prefix="ak820-ci-gl-", dir=runner_temp) as temporary:
            root = Path(temporary)
            archive = root / asset
            download(url, archive, archive_sha)
            application = root / "application"
            application.mkdir()
            driver = application / "opengl32.dll"
            extract_driver(archive, driver, machine, seven_zip)
            copy = application / binary.name
            shutil.copyfile(binary, copy)
            if digest(copy) != original_sha or digest(binary) != original_sha:
                raise ValueError("CI temporary executable differs from production executable")
            provenance.update(driver_sha256=digest(driver), original_executable_sha256=original_sha,
                              copied_executable_sha256=digest(copy))
            ok = run(copy, report)
            result = json.loads(report.read_text(encoding="utf-8"))
            if result.get("executable_sha256") != original_sha or digest(binary) != original_sha:
                raise ValueError("Runtime report executable binding changed")
            result["ci_graphics_fixture"] = provenance
            report.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            return ok
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        result = {"format": "ak820-runtime-smoke-harness-1", "status": "failed", "error": str(error),
                  "ci_graphics_fixture": provenance}
        report.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    arch = expected_arch()
    runner_temp = os.environ.get("RUNNER_TEMP")
    if expected_os() != "windows" or arch not in PINS or not runner_temp or not Path(runner_temp).is_dir():
        parser.error("Native Windows CI and an existing RUNNER_TEMP are required")
    seven_zip = shutil.which("7z") or str(Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "7-Zip/7z.exe")
    if not Path(seven_zip).is_file():
        parser.error("CI 7-Zip executable is required")
    if not execute(args.binary, args.report, arch, runner_temp, seven_zip):
        parser.exit(1, f"Windows CI runtime smoke failed; report retained at {args.report}\n")


if __name__ == "__main__":
    main()
