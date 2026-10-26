#!/usr/bin/env python3
"""Verify six complete unsigned packages before creating a draft release."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import tomllib
import zipfile

from package_ak820_candidate import PLATFORMS, check_architecture

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = tuple(sorted(PLATFORMS))
STATIC_FILES = {
    "INSTALL.en.md": "docs/INSTALL.md",
    "INSTALL.ru.md": "docs/ru/INSTALL.md",
    "RGB-PLAN.en.md": "docs/RGB-PLAN.md",
    "RGB-PLAN.ru.md": "docs/ru/RGB-PLAN.md",
    "firmware-provenance.json": "assets/ak820/beta2-provenance.json",
    "LICENSE": "LICENSE",
    "FONT-LICENSES.txt": "assets/FONT-LICENSES.txt",
    "LUCIDE-LICENSE.txt": "assets/LUCIDE-LICENSE.txt",
    "eframe-LICENSE-MIT": "vendor/eframe/LICENSE-MIT",
    "eframe-LICENSE-APACHE": "vendor/eframe/LICENSE-APACHE",
}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def expected_static_files():
    return {name: (ROOT / path).read_bytes() for name, path in STATIC_FILES.items()}


def verify_internal_checksums(contents):
    raw = contents.get("SHA256SUMS")
    if raw is None:
        raise ValueError("Candidate package is missing internal SHA256SUMS")
    try:
        lines = raw.decode("ascii").splitlines()
    except UnicodeDecodeError as exc:
        raise ValueError("Internal SHA256SUMS is not ASCII") from exc
    parsed = {}
    for line in lines:
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9._-]+)", line)
        if not match:
            raise ValueError("Malformed entry in internal SHA256SUMS")
        digest, filename = match.groups()
        if filename in parsed:
            raise ValueError("Duplicate entry in internal SHA256SUMS")
        parsed[filename] = digest
    required = set(contents) - {"SHA256SUMS"}
    if set(parsed) != required:
        raise ValueError("Internal SHA256SUMS does not cover the exact package contents")
    for filename, digest in parsed.items():
        if sha256(contents[filename]) != digest:
            raise ValueError(f"Internal checksum mismatch for {filename}")


def verify(directory: Path, version: str, commit: str):
    if tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"] != version:
        raise ValueError("Requested application version differs from checked-out Cargo.toml")
    checked_out_commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if checked_out_commit != commit:
        raise ValueError("Verifier checkout does not match the validated source commit")
    expected_static = expected_static_files()
    firmware = json.loads(expected_static["firmware-provenance.json"])
    result = {"format": "ak-configurator-release-candidate-1", "application_version": version,
              "source_commit": commit, "status": "unsigned desktop trial; hardware and platform qualification remain separate",
              "platforms": {}}
    for platform in EXPECTED:
        name = f"ak-configurator-{version}-{platform}.zip"
        archive = directory / name
        checksum = directory / f"{name}.sha256"
        if not archive.is_file() or not checksum.is_file():
            raise ValueError(f"Missing candidate archive or checksum for {platform}")
        archive_hash = sha256(archive.read_bytes())
        if checksum.read_text(encoding="ascii").strip() != f"{archive_hash}  {name}":
            raise ValueError(f"Checksum mismatch for {platform}")
        with zipfile.ZipFile(archive) as package:
            root = f"ak-configurator-{version}-{platform}/"
            infos = package.infolist()
            names = [item.filename for item in infos]
            if len(names) != len(set(names)):
                raise ValueError(f"Duplicate ZIP member for {platform}")
            for member in names:
                parts = PurePosixPath(member).parts
                if "\\" in member or member.startswith("/") or any(part in ("", ".", "..") for part in parts):
                    raise ValueError(f"Unsafe ZIP member path for {platform}")
                if not member.startswith(root) or len(parts) != 2:
                    raise ValueError(f"Unexpected ZIP member path for {platform}")
            binary_name = "ak-configurator.exe" if platform.startswith("windows-") else "ak-configurator"
            expected_names = set(STATIC_FILES) | {binary_name, "manifest.json", "SHA256SUMS"}
            if set(names) != {root + filename for filename in expected_names}:
                raise ValueError(f"Candidate package has missing or unexpected members for {platform}")
            contents = {PurePosixPath(member).name: package.read(member) for member in names}
        for filename, expected in expected_static.items():
            if contents.get(filename) != expected:
                raise ValueError(f"Bundled source file differs from checked-out source: {filename}")
        verify_internal_checksums(contents)
        try:
            manifest = json.loads(contents["manifest.json"])
        except (KeyError, json.JSONDecodeError) as exc:
            raise ValueError(f"Malformed candidate manifest for {platform}") from exc
        if manifest.get("format") != "ak-configurator-desktop-candidate-1":
            raise ValueError(f"Unexpected manifest format for {platform}")
        if manifest.get("platform") != platform:
            raise ValueError(f"Wrong platform in manifest for {platform}")
        if manifest.get("application_version") != version:
            raise ValueError(f"Wrong application version for {platform}")
        if manifest.get("source_commit") != commit or manifest.get("source_dirty") is not False:
            raise ValueError(f"Wrong or dirty source provenance for {platform}")
        if manifest.get("source_repository") != "https://github.com/dvino/ak-configurator":
            raise ValueError(f"Wrong source repository in manifest for {platform}")
        if manifest.get("firmware_release") != firmware.get("release"):
            raise ValueError(f"Wrong firmware release provenance for {platform}")
        if manifest.get("firmware_commit") != firmware.get("tag_commit"):
            raise ValueError(f"Wrong firmware commit provenance for {platform}")
        if manifest.get("firmware_tag_object") != firmware.get("tag_object"):
            raise ValueError(f"Wrong firmware tag provenance for {platform}")
        if manifest.get("firmware_provenance_sha256") != sha256(expected_static["firmware-provenance.json"]):
            raise ValueError(f"Firmware provenance digest mismatch for {platform}")
        binary = contents[binary_name]
        if manifest.get("binary_sha256") != sha256(binary):
            raise ValueError(f"Binary digest mismatch for {platform}")
        check_architecture(binary, platform)
        result["platforms"][platform] = {"archive": name, "sha256": archive_hash}
    if set(result["platforms"]) != set(EXPECTED):
        raise ValueError("Candidate platform set is incomplete")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    args = parser.parse_args()
    try:
        report = verify(args.directory, args.version, args.source_commit)
        args.manifest.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    except (OSError, ValueError, subprocess.CalledProcessError, zipfile.BadZipFile) as exc:
        parser.exit(1, f"Release candidate rejected: {exc}\n")


if __name__ == "__main__":
    main()
