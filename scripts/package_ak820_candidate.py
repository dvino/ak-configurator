#!/usr/bin/env python3
"""Package an unsigned desktop candidate; never publish or operate a device."""

import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PLATFORMS = {
    "linux-x86_64": ("elf", 62),
    "linux-arm64": ("elf", 183),
    "windows-x86_64": ("pe", 0x8664),
    "windows-arm64": ("pe", 0xAA64),
    "macos-x86_64": ("macho", 0x01000007),
    "macos-arm64": ("macho", 0x0100000C),
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def check_architecture(data, platform):
    kind, machine = PLATFORMS[platform]
    if kind == "elf":
        valid = len(data) >= 64 and data[:6] == b"\x7fELF\x02\x01"
        valid = valid and struct.unpack_from("<H", data, 18)[0] == machine
    elif kind == "pe":
        valid = len(data) >= 64 and data[:2] == b"MZ"
        offset = struct.unpack_from("<I", data, 0x3C)[0] if valid else 0
        valid = valid and offset <= len(data) - 26 and data[offset:offset + 4] == b"PE\0\0"
        valid = valid and struct.unpack_from("<H", data, offset + 4)[0] == machine
        valid = valid and struct.unpack_from("<H", data, offset + 24)[0] == 0x20B
    else:
        valid = len(data) >= 32 and data[:4] == b"\xcf\xfa\xed\xfe"
        valid = valid and struct.unpack_from("<I", data, 4)[0] == machine
    if not valid:
        raise ValueError(f"Binary format/architecture does not match {platform}")


def build_package(binary, platform, output, allow_dirty=False):
    data = Path(binary).read_bytes()
    check_architecture(data, platform)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT).strip())
    if dirty and not allow_dirty:
        raise ValueError("Source tree is dirty; use --allow-dirty only for a labeled local trial")
    provenance = (ROOT / "assets/ak820/beta2-provenance.json").read_bytes()
    firmware = json.loads(provenance)
    for asset in firmware["assets"]:
        if asset["name"].endswith(".json"):
            bundled = (ROOT / "assets/ak820" / ("beta2-" + asset["name"])).read_bytes()
            if len(bundled) != asset["size"] or digest(bundled) != asset["sha256"]:
                raise ValueError(f"Bundled firmware catalog differs: {asset['name']}")
    executable = "ak-configurator.exe" if platform.startswith("windows-") else "ak-configurator"
    files = {
        executable: data,
        "INSTALL.en.md": (ROOT / "docs/INSTALL.md").read_bytes(),
        "INSTALL.ru.md": (ROOT / "docs/ru/INSTALL.md").read_bytes(),
        "RGB-PLAN.en.md": (ROOT / "docs/RGB-PLAN.md").read_bytes(),
        "RGB-PLAN.ru.md": (ROOT / "docs/ru/RGB-PLAN.md").read_bytes(),
        "firmware-provenance.json": provenance,
        "LICENSE": (ROOT / "LICENSE").read_bytes(),
        "FONT-LICENSES.txt": (ROOT / "assets/FONT-LICENSES.txt").read_bytes(),
        "LUCIDE-LICENSE.txt": (ROOT / "assets/LUCIDE-LICENSE.txt").read_bytes(),
        "eframe-LICENSE-MIT": (ROOT / "vendor/eframe/LICENSE-MIT").read_bytes(),
        "eframe-LICENSE-APACHE": (ROOT / "vendor/eframe/LICENSE-APACHE").read_bytes(),
    }
    manifest = {
        "format": "ak-configurator-desktop-candidate-1",
        "application_version": version,
        "platform": platform,
        "source_commit": commit,
        "source_repository": "https://github.com/dvino/ak-configurator",
        "source_dirty": dirty,
        "status": "unsigned desktop trial; hardware and platform qualification remain separate",
        "firmware_release": firmware["release"],
        "firmware_commit": firmware["tag_commit"],
        "firmware_provenance_sha256": digest(provenance),
        "binary_sha256": digest(data),
    }
    files["manifest.json"] = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()
    files["SHA256SUMS"] = "".join(f"{digest(value)}  {name}\n" for name, value in sorted(files.items())).encode()
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    name = f"ak-configurator-{version}-{platform}"
    archive = output / f"{name}.zip"
    checksum = output / f"{name}.zip.sha256"
    if archive.exists() or checksum.exists():
        raise ValueError("Candidate output already exists; choose an empty output directory")
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as package:
        for filename, value in sorted(files.items()):
            info = zipfile.ZipInfo(f"{name}/{filename}", date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = (0o100755 if filename == executable else 0o100644) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            package.writestr(info, value)
    checksum.write_text(f"{digest(archive.read_bytes())}  {archive.name}\n", encoding="ascii")
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--platform", choices=PLATFORMS, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--allow-dirty", action="store_true")
    args = parser.parse_args()
    try:
        print(build_package(args.binary, args.platform, args.output_dir, args.allow_dirty))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Packaging stopped: {error}\n")


if __name__ == "__main__":
    main()
