#!/usr/bin/env python3
"""Package a desktop candidate; never publish or operate a device."""

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
import re
import tempfile
from urllib.parse import quote, unquote, urlsplit
from ak820_package_metadata import public_inputs, provenance, provenance_bytes, runtime_identity
from windows_signing import (CERT_THUMBPRINT, SIGNED_PATHS, canonical_report, pinned_certificate,
                             sign_file, sign_report, thumbprint)

ROOT = Path(__file__).resolve().parents[1]
PLATFORMS = {
    "linux-x86_64": ("elf", 62),
    "linux-arm64": ("elf", 183),
    "windows-x86_64": ("pe", 0x8664),
    "windows-arm64": ("pe", 0xAA64),
    "macos-x86_64": ("macho", 0x01000007),
    "macos-arm64": ("macho", 0x0100000C),
}
DOCUMENTS = {
    "INSTALL.en.md": "docs/INSTALL.md",
    "INSTALL.ru.md": "docs/ru/INSTALL.md",
    "DESKTOP.en.md": "docs/DESKTOP.md",
    "DESKTOP.ru.md": "docs/ru/DESKTOP.md",
    "CHANGELOG.en.md": "CHANGELOG.md",
    "CHANGELOG.ru.md": "docs/ru/CHANGELOG.md",
    "RGB-PLAN.en.md": "docs/RGB-PLAN.md",
    "RGB-PLAN.ru.md": "docs/ru/RGB-PLAN.md",
    "THIRD-PARTY-AUDIO.en.md": "docs/THIRD-PARTY-AUDIO.md",
    "THIRD-PARTY-AUDIO.ru.md": "docs/ru/THIRD-PARTY-AUDIO.md",
    "STATUS.en.md": "docs/STATUS.md",
    "STATUS.ru.md": "docs/ru/STATUS.md",
    "REVIEW.en.md": "docs/REVIEW.md",
    "REVIEW.ru.md": "docs/ru/REVIEW.md",
    "MCP.en.md": "docs/MCP.md",
    "MCP.ru.md": "docs/ru/MCP.md",
}


def package_document(source, commit):
    """Keep delivered guide links local and mark omitted source documents."""
    source = ROOT / source
    destinations = {(ROOT / path).resolve(): name for name, path in DOCUMENTS.items()}
    destinations[(ROOT / "vendor/cava-dependencies/REBUILD.md").resolve()] = "audio-sources/REBUILD.md"

    def rewrite(match):
        target = urlsplit(match[2])
        if target.scheme or target.netloc or not target.path:
            return match[0]
        path = (source.parent / unquote(target.path)).resolve()
        relative = path.relative_to(ROOT)
        if not path.exists():
            raise ValueError(f"Broken documentation link in {source.name}: {match[2]}")
        link = destinations.get(path)
        if link is None:
            return match[1][:-1] + " (source document not included in this package)"
        if target.query:
            link += "?" + target.query
        if target.fragment:
            link += "#" + target.fragment
        return match[1] + link + match[3]

    return re.sub(r"(\[[^\]\n]*\]\()([^\s)]+)(\))", rewrite,
                  source.read_text(encoding="utf-8")).encode("utf-8")


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


def build_package(binary, platform, output, allow_dirty=False, runtime_check=None, cava_helper=None,
                  signing_thumbprint=None, signtool=None, timestamp_url=None):
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
    provenance_data = provenance_bytes()
    firmware = provenance()
    offline_files = public_inputs()
    executable = "ak-configurator.exe" if platform.startswith("windows-") else "ak-configurator"
    app_bundle = platform.startswith("macos-")
    payload_binary = "AK-Configurator.app/Contents/MacOS/ak-configurator" if app_bundle else executable
    cava_name = ("AK-Configurator.app/Contents/MacOS/" if app_bundle else "") + "audio/cava" + (".exe" if platform.startswith("windows-") else "")
    files = {
        cava_name: cava_data,
        "audio-sources/build_cava_helper.py": (ROOT / "scripts/build_cava_helper.py").read_bytes(),
        "CAVA-LICENSE": (ROOT / "vendor/cava/LICENSE").read_bytes(),
        "CAVA-PROVENANCE.json": (ROOT / "vendor/cava-provenance.json").read_bytes(),
        payload_binary: data,
        "firmware-provenance.json": provenance_data,
        "LICENSE": (ROOT / "LICENSE").read_bytes(),
        "FONT-LICENSES.txt": (ROOT / "assets/FONT-LICENSES.txt").read_bytes(),
        "LUCIDE-LICENSE.txt": (ROOT / "assets/LUCIDE-LICENSE.txt").read_bytes(),
        "eframe-LICENSE-MIT": (ROOT / "vendor/eframe/LICENSE-MIT").read_bytes(),
        "eframe-LICENSE-APACHE": (ROOT / "vendor/eframe/LICENSE-APACHE").read_bytes(),
    }
    files.update({name: package_document(source, commit) for name, source in DOCUMENTS.items()})
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
    if signing_thumbprint or signtool or timestamp_url:
        if not platform.startswith('windows-') or thumbprint(signing_thumbprint or '') != CERT_THUMBPRINT or not signtool or not timestamp_url:
            raise ValueError('Windows signing requires the pinned development certificate, SignTool and timestamp URL')
        pinned_certificate(ROOT)
        signing = {}
        with tempfile.TemporaryDirectory(prefix='ak-sign-payload-') as temporary:
            for name in SIGNED_PATHS:
                path = Path(temporary).joinpath(*name.split('/'))
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(files[name])
                info = sign_file(path, signing_thumbprint, signtool, timestamp_url)
                files[name] = path.read_bytes()
                signing[name] = {'unsigned_sha256': info['unsigned_sha256'], 'signed_sha256': info['signed_sha256']}
            files['windows-signing.json'] = canonical_report(signing)
            files['windows-signing.p7s'] = sign_report(files['windows-signing.json'], signing_thumbprint)
        data, cava_data = files[payload_binary], files[cava_name]
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
        if not isinstance(check, dict) or check.get("format") != "ak-configurator-self-check-1" or check.get("platform") != platform or check.get("status") != "passed" or check.get("hardware_access") is not False or check.get("network_access") is not False:
            raise ValueError("Runtime self-check does not prove a passing, offline, device-free execution for this target")
        if (check.get("application_version") != version
                or check.get("firmware_release") != firmware["release"]
                or check.get("catalog_sha256") != digest(offline_files["offline/release-catalog.json"])):
            raise ValueError("Runtime self-check belongs to a different application or firmware catalog")
        profiles = check.get("profiles")
        if (not isinstance(profiles, list)
                or any(not isinstance(p, dict) or not isinstance(p.get("profile"), str) for p in profiles)
                or sorted(profiles, key=lambda p: p["profile"]) != runtime_identity()["profiles"]):
            raise ValueError("Runtime self-check profile identities differ")
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
        "status": ("local-development signed desktop trial; public trust and hardware qualification remain separate"
                   if signing_thumbprint else "unsigned desktop trial; hardware and platform qualification remain separate"),
        "firmware_release": firmware["release"],
        "firmware_commit": firmware["tag_commit"],
        "firmware_tag_object": firmware["tag_object"],
        "firmware_provenance_sha256": digest(provenance_data),
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
    parser.add_argument('--windows-signing-thumbprint')
    parser.add_argument('--signtool', type=Path)
    parser.add_argument('--timestamp-url')
    args = parser.parse_args()
    try:
        print(build_package(args.binary, args.platform, args.output_dir, args.allow_dirty, args.runtime_check, args.cava_helper,
                            args.windows_signing_thumbprint, args.signtool, args.timestamp_url))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Packaging stopped: {error}\n")


if __name__ == "__main__":
    main()
