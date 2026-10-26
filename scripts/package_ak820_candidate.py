#!/usr/bin/env python3
"""Package an unsigned desktop candidate; never publish or operate a device."""

import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tarfile
import tomllib
import zipfile
import io
import plistlib

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


def build_package(binary, platform, output, allow_dirty=False, runtime_check=None, cava_helper=None):
    if cava_helper is None:
        raise ValueError("Bundled CAVA helper is required")
    cava_data = Path(cava_helper).read_bytes()
    check_architecture(cava_data, platform)
    cava_identity = json.loads((ROOT / "vendor/cava-provenance.json").read_text())
    data = Path(binary).read_bytes()
    check_architecture(data, platform)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT).strip())
    if dirty and not allow_dirty:
        raise ValueError("Source tree is dirty; use --allow-dirty only for a labeled local trial")
    provenance = (ROOT / "assets/ak820/beta2-provenance.json").read_bytes()
    release_manifest = json.loads((ROOT / "assets/ak820/beta2-release-manifest.json").read_text(encoding="utf-8"))
    offline_files = {}
    for asset in sorted((ROOT / "assets/ak820").glob("beta2-*.json")):
        relative = asset.name.removeprefix("beta2-")
        offline_files[f"offline/{relative}"] = asset.read_bytes()
    for asset in sorted((ROOT / "assets/ak820/firmware-beta2").rglob("*")):
        if asset.is_file():
            relative = asset.relative_to(ROOT / "assets/ak820/firmware-beta2").as_posix()
            offline_files[f"offline/firmware-beta2/{relative}"] = asset.read_bytes()
            profile, basename = relative.split("/", 1)
            manifest_name = f"profiles/{profile}/firmware/{basename}"
            identity = release_manifest["files"].get(manifest_name)
            if not identity or len(offline_files[f"offline/firmware-beta2/{relative}"]) != identity["size"] or digest(offline_files[f"offline/firmware-beta2/{relative}"]) != identity["sha256"]:
                raise ValueError(f"Bundled public firmware asset differs from release manifest: {manifest_name}")
    firmware = json.loads(provenance)
    for asset in firmware["assets"]:
        if asset["name"].endswith(".json"):
            bundled = (ROOT / "assets/ak820" / ("beta2-" + asset["name"])).read_bytes()
            if len(bundled) != asset["size"] or digest(bundled) != asset["sha256"]:
                raise ValueError(f"Bundled firmware catalog differs: {asset['name']}")
    executable = "ak-configurator.exe" if platform.startswith("windows-") else "ak-configurator"
    app_bundle = platform.startswith("macos-")
    payload_binary = "AK-Configurator.app/Contents/MacOS/ak-configurator" if app_bundle else executable
    cava_name = ("AK-Configurator.app/Contents/MacOS/" if app_bundle else "") + "audio/cava" + (".exe" if platform.startswith("windows-") else "")
    files = {
        cava_name: cava_data,
        "audio-sources/build_cava_helper.py": (ROOT / "scripts/build_cava_helper.py").read_bytes(),
        "CAVA-LICENSE": (ROOT / "vendor/cava/LICENSE").read_bytes(),
        "CAVA-PROVENANCE.json": (ROOT / "vendor/cava-provenance.json").read_bytes(),
        "THIRD-PARTY-AUDIO.en.md": (ROOT / "docs/THIRD-PARTY-AUDIO.md").read_bytes(),
        "THIRD-PARTY-AUDIO.ru.md": (ROOT / "docs/ru/THIRD-PARTY-AUDIO.md").read_bytes(),
        payload_binary: data,
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
    helper_dir = "windows" if platform.startswith("windows-") else ("macos" if app_bundle else "linux")
    files["install-user.ps1" if helper_dir == "windows" else "install-user.sh"] = (ROOT / "scripts/ak820-release" / helper_dir / ("install-user.ps1" if helper_dir == "windows" else "install-user.sh")).read_bytes()
    files["uninstall-user.ps1" if helper_dir == "windows" else "uninstall-user.sh"] = (ROOT / "scripts/ak820-release" / helper_dir / ("uninstall-user.ps1" if helper_dir == "windows" else "uninstall-user.sh")).read_bytes()
    if app_bundle:
        files["AK-Configurator.app/Contents/Resources/install-user.sh"] = files["install-user.sh"]
        files["AK-Configurator.app/Contents/Resources/uninstall-user.sh"] = files["uninstall-user.sh"]
    if platform.startswith("windows-"):
        windows_source = Path(cava_helper).parent / "cava-windows-dependency-sources.tar.gz"
        if not windows_source.is_file():
            raise ValueError("Windows helper corresponding dependency source is required")
        files["audio-sources/windows-built-dependencies.tar.gz"] = windows_source.read_bytes()
    files.update(offline_files)
    for dependency in sorted((ROOT / "vendor/cava-dependencies").iterdir()):
        if dependency.is_file():
            files["audio-sources/" + dependency.name] = dependency.read_bytes()
    source_buffer = io.BytesIO()
    with tarfile.open(fileobj=source_buffer, mode="w:gz") as source_archive:
        for source in sorted((ROOT / "vendor/cava").rglob("*")):
            if source.is_file():
                value = source.read_bytes()
                entry = tarfile.TarInfo("cava/" + source.relative_to(ROOT / "vendor/cava").as_posix())
                entry.size = len(value)
                entry.mode = 0o644
                source_archive.addfile(entry, io.BytesIO(value))
    files["audio-sources/cava-patched-source.tar.gz"] = source_buffer.getvalue()
    if app_bundle:
        for filename, value in list(files.items()):
            if filename.startswith(("audio-sources/", "CAVA-", "THIRD-PARTY-AUDIO")):
                files["AK-Configurator.app/Contents/Resources/" + filename] = value
    if runtime_check is not None:
        check = json.loads(Path(runtime_check).read_text(encoding="utf-8"))
        if check.get("format") != "ak-configurator-self-check-1" or check.get("platform") != platform or check.get("status") != "passed" or check.get("hardware_access") is not False or check.get("network_access") is not False:
            raise ValueError("Runtime self-check does not prove a passing, offline, device-free execution for this target")
        files["runtime-self-check.json"] = (json.dumps(check, indent=2, sort_keys=True) + "\n").encode()
    if app_bundle:
        numeric_version = ".".join(version.split("-", 1)[0].split("+")[0].split(".")[:3])
        plist = {
            "CFBundleExecutable": "ak-configurator",
            "CFBundleIdentifier": "com.dvino.ak-configurator",
            "CFBundleName": "AK Configurator",
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": numeric_version,
            "CFBundleVersion": numeric_version,
            "NSAudioCaptureUsageDescription": "Audio visualization uses system playback audio locally while Music is enabled.",
            "LSMinimumSystemVersion": "11.0" if platform == "macos-arm64" else "10.15",
        }
        files["AK-Configurator.app/Contents/Info.plist"] = plistlib.dumps(plist, fmt=plistlib.FMT_XML, sort_keys=True)
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
        "firmware_tag_object": firmware["tag_object"],
        "firmware_provenance_sha256": digest(provenance),
        "binary_sha256": digest(data),
        "audio_helper": {**cava_identity, "path": cava_name, "sha256": digest(cava_data)},
    }
    files["manifest.json"] = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()
    files["SHA256SUMS"] = "".join(f"{digest(value)}  {name}\n" for name, value in sorted(files.items())).encode()
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    name = f"ak-configurator-{version}-{platform}"
    suffix = ".app.zip" if app_bundle else (".tar.gz" if platform.startswith("linux-") else ".zip")
    archive = output / f"{name}{suffix}"
    checksum = output / f"{name}{suffix}.sha256"
    if archive.exists() or checksum.exists():
        raise ValueError("Candidate output already exists; choose an empty output directory")
    if platform.startswith("linux-"):
        with tarfile.open(archive, "x:gz", format=tarfile.PAX_FORMAT) as package:
            for filename, value in sorted(files.items()):
                info = tarfile.TarInfo(f"{name}/{filename}")
                info.size = len(value)
                info.mtime = 0
                info.mode = 0o755 if filename in (executable, cava_name) else 0o644
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                package.addfile(info, io.BytesIO(value))
    else:
        with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as package:
            for filename, value in sorted(files.items()):
                info = zipfile.ZipInfo(f"{name}/{filename}", date_time=(1980, 1, 1, 0, 0, 0))
                info.create_system = 3
                info.external_attr = (0o100755 if filename in (payload_binary, cava_name) else 0o100644) << 16
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
    parser.add_argument("--runtime-check", type=Path, help="JSON produced by the built target's --self-check")
    parser.add_argument("--cava-helper", type=Path, required=True)
    args = parser.parse_args()
    try:
        print(build_package(args.binary, args.platform, args.output_dir, args.allow_dirty, args.runtime_check, args.cava_helper))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Packaging stopped: {error}\n")


if __name__ == "__main__":
    main()
