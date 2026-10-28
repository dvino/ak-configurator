#!/usr/bin/env python3
"""Run the native GUI offline smoke mode and retain bounded evidence."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile

FORMAT = "ak820-runtime-smoke-1"
HARNESS_FORMAT = "ak820-runtime-smoke-harness-1"
COMMON_CASES = ("window_launch", "rgb_navigation", "settings_navigation", "render_after_navigation")
STARTUP_PHASES = ("window_creator", "splash_first_frame_submitted", "editor_initialization_started",
                  "editor_fonts_configured", "editor_initialization_finished", "editor_first_frame_submitted")


def validate_startup(value):
    if (not isinstance(value, dict) or value.get("unit") != "milliseconds_from_process_entry"
            or value.get("display_fence_measured") is not False):
        raise ValueError("Startup timing evidence is missing or mislabels display presentation")
    phases = value.get("phases")
    if not isinstance(phases, list):
        raise ValueError("Startup phase timing list is missing")
    observed = {}
    previous = -1.0
    for item in phases:
        if not isinstance(item, dict):
            raise ValueError("Invalid startup phase record")
        name, elapsed = item.get("phase"), item.get("elapsed_ms")
        if (not isinstance(name, str) or name in observed or not isinstance(elapsed, (int, float))
                or isinstance(elapsed, bool) or not math.isfinite(elapsed) or elapsed < 0 or elapsed < previous):
            raise ValueError("Startup phases are invalid, repeated or out of order")
        observed[name] = elapsed
        previous = elapsed
    names = list(observed)
    if any(name not in observed for name in STARTUP_PHASES):
        raise ValueError("Required startup feedback phases are missing")
    if [name for name in names if name in STARTUP_PHASES] != list(STARTUP_PHASES):
        raise ValueError("Editor initialized before startup feedback was submitted")


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def expected_os():
    return {"Windows": "windows", "Linux": "linux", "Darwin": "macos"}.get(platform.system())


def expected_arch():
    return {"x86_64": "x86_64", "AMD64": "x86_64", "aarch64": "aarch64", "arm64": "aarch64",
            "ARM64": "aarch64"}.get(platform.machine())


def validate(raw, version, os_name, arch):
    if not isinstance(raw, dict) or raw.get("format") != FORMAT:
        raise ValueError("Executable did not emit a valid runtime smoke JSON object")
    if (raw.get("application_version") != version or raw.get("os") != os_name or raw.get("arch") != arch
            or raw.get("status") != "passed"):
        raise ValueError("Runtime smoke identity or overall status did not pass")
    cases = raw.get("cases")
    if not isinstance(cases, list):
        raise ValueError("Runtime smoke cases are missing")
    statuses = {}
    for case in cases:
        if isinstance(case, dict) and isinstance(case.get("name"), str):
            statuses[case["name"]] = case.get("status")
    expected = [*COMMON_CASES, "native_tray"]
    for name in expected:
        if statuses.get(name) != "passed":
            raise ValueError(f"Required runtime smoke case did not pass: {name}")
    frames = raw.get("rendered_frames")
    if not isinstance(frames, int) or isinstance(frames, bool) or frames < 12:
        raise ValueError("Runtime smoke rendered-frame count is missing or too small")
    if raw.get("live_hid_allowed") is not False or raw.get("physical_behavior_validated") is not False:
        raise ValueError("Runtime smoke report does not explicitly exclude live hardware qualification")
    if raw.get("isolated_settings") is not True:
        raise ValueError("Runtime smoke did not confirm isolated temporary settings")
    validate_startup(raw.get("startup_timings"))


def run(binary: Path, report: Path, timeout: float = 35):
    binary = binary.absolute()
    os_name, arch = expected_os(), expected_arch()
    version = None
    parsed = None
    error = None
    stderr = ""
    return_code = None
    binary_hash = None
    try:
        if os_name not in ("linux", "windows", "macos") or arch not in ("x86_64", "aarch64"):
            raise ValueError(f"Unsupported runtime smoke host: {platform.system()} {platform.machine()}")
        if not binary.is_file():
            raise ValueError("Runtime smoke executable does not exist or is not a file")
        binary_hash = digest(binary)
        # The executable emits application_version in its one-line JSON report;
        # validate its stable semver-like identity below without an environment override.
        with tempfile.TemporaryDirectory(prefix="ak820-runtime-smoke-") as cwd:
            proc = subprocess.run([str(binary), "--runtime-smoke"], cwd=cwd, capture_output=True,
                                  text=True, encoding="utf-8", errors="replace", timeout=timeout, check=False)
        return_code, stderr = proc.returncode, proc.stderr[-8192:]
        lines = [line for line in proc.stdout.splitlines() if line.strip()]
        if len(lines) != 1:
            raise ValueError(f"Expected one JSON line on stdout, received {len(lines)}")
        if len(lines[0].encode("utf-8")) > 65536:
            raise ValueError("Runtime smoke JSON exceeds the 64 KiB report limit")
        parsed = json.loads(lines[0])
        version = parsed.get("application_version") if isinstance(parsed, dict) else None
        if proc.returncode != 0:
            raise ValueError(f"Runtime smoke executable exited with status {proc.returncode}")
        if not isinstance(version, str) or not version:
            raise ValueError("Runtime smoke application version is missing")
        validate(parsed, version, os_name, arch)
    except subprocess.TimeoutExpired as exc:
        stderr = (exc.stderr or b"").decode("utf-8", "replace")[-8192:] if isinstance(exc.stderr, bytes) else (exc.stderr or "")[-8192:]
        error = f"Runtime smoke exceeded {timeout:g}s timeout"
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        error = str(exc)
    result = {"format": HARNESS_FORMAT, "status": "passed" if error is None else "failed",
              "os": os_name, "arch": arch, "executable_sha256": binary_hash,
              "application_version": version, "exit_code": return_code,
              "runtime_smoke": parsed, "stderr_tail": stderr, "error": error}
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return error is None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=35)
    args = parser.parse_args()
    if args.timeout <= 0 or args.timeout > 35:
        parser.error("--timeout must be greater than 0 and at most 35 seconds")
    try:
        ok = run(args.binary, args.report, args.timeout)
    except OSError as exc:
        parser.exit(1, f"Could not retain runtime smoke report: {exc}\n")
    if not ok:
        parser.exit(1, f"Runtime smoke failed; report retained at {args.report}\n")


if __name__ == "__main__":
    main()
