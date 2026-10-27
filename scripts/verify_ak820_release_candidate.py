#!/usr/bin/env python3
"""Verify six complete unsigned candidate packages before artifact retention."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
import tomllib
import zipfile
import io

from package_ak820_candidate import DOCUMENTS, PLATFORMS, check_architecture, package_document

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = tuple(sorted(PLATFORMS))
STATIC_FILES = {
    "audio-sources/build_cava_helper.py": "scripts/build_cava_helper.py",
    "CAVA-LICENSE": "vendor/cava/LICENSE",
    "CAVA-PROVENANCE.json": "vendor/cava-provenance.json",
    "firmware-provenance.json": "assets/ak820/beta3-provenance.json",
    "LICENSE": "LICENSE",
    "FONT-LICENSES.txt": "assets/FONT-LICENSES.txt",
    "LUCIDE-LICENSE.txt": "assets/LUCIDE-LICENSE.txt",
    "eframe-LICENSE-MIT": "vendor/eframe/LICENSE-MIT",
    "eframe-LICENSE-APACHE": "vendor/eframe/LICENSE-APACHE",
}
TARGETS = {
    "linux-x86_64": ("linux", "x86_64", "x86_64-unknown-linux-gnu"),
    "linux-arm64": ("linux", "arm64", "aarch64-unknown-linux-gnu"),
    "windows-x86_64": ("windows", "x86_64", "x86_64-pc-windows-msvc"),
    "windows-arm64": ("windows", "arm64", "aarch64-pc-windows-msvc"),
    "macos-x86_64": ("macos", "x86_64", "x86_64-apple-darwin"),
    "macos-arm64": ("macos", "arm64", "aarch64-apple-darwin"),
}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def expected_static_files(platform, commit):
    files = {name: (ROOT / path).read_bytes() for name, path in STATIC_FILES.items()}
    files.update({name: package_document(source, commit) for name, source in DOCUMENTS.items()})
    os_name = TARGETS[platform][0]
    for kind in ("install", "uninstall"):
        suffix = ".ps1" if os_name == "windows" else ".sh"
        filename = f"{kind}-user{suffix}"
        files[filename] = (ROOT / "scripts/ak820-release" / os_name / filename).read_bytes()
        if os_name == "macos":
            files[f"AK-Configurator.app/Contents/Resources/{filename}"] = files[filename]
    for asset in sorted((ROOT / "assets/ak820").glob("beta3-*.json")):
        files[f"offline/{asset.name.removeprefix('beta3-')}"] = asset.read_bytes()
    for asset in sorted((ROOT / "assets/ak820/firmware-beta3").rglob("*")):
        if asset.is_file():
            relative = asset.relative_to(ROOT / "assets/ak820/firmware-beta3").as_posix()
            files[f"offline/firmware-beta3/{relative}"] = asset.read_bytes()
    for dependency in sorted((ROOT / "vendor/cava-dependencies").iterdir()):
        if dependency.is_file():
            files["audio-sources/" + dependency.name] = dependency.read_bytes()
    if os_name == "macos":
        for filename, value in list(files.items()):
            if filename.startswith(("audio-sources/", "CAVA-", "THIRD-PARTY-AUDIO")):
                files["AK-Configurator.app/Contents/Resources/" + filename] = value
    return files


def expected_runtime_identity(platform):
    provenance = json.loads((ROOT / "assets/ak820/beta3-provenance.json").read_text(encoding="utf-8"))
    defaults = json.loads((ROOT / "tests/fixtures/ak820-beta3-defaults.json").read_text(encoding="utf-8"))
    catalog = (ROOT / "assets/ak820/beta3-capability-catalog.json").read_bytes()
    profiles = []
    for profile in ("main", "game", "mmo", "typing", "code", "pointer", "onehand", "creative", "shortcuts", "multios"):
        folder = ROOT / "assets/ak820/firmware-beta3" / profile
        profiles.append({
            "profile": profile,
            "ihx_sha256": sha256((folder / "application.ihx").read_bytes()),
            "map_sha256": sha256((folder / "application.map").read_bytes()),
            "default_settings_sha256": defaults[profile],
        })
    profiles.sort(key=lambda item: item["profile"])
    return {
        "firmware_release": provenance["release"],
        "catalog_sha256": sha256(catalog),
        "profiles": profiles,
    }


def normalize_runtime_profiles(report):
    normalized = dict(report)
    profiles = normalized.get("profiles")
    if isinstance(profiles, list):
        normalized["profiles"] = sorted(
            profiles,
            key=lambda item: item.get("profile", "") if isinstance(item, dict) else "",
        )
    return normalized



def verify_windows_audio_sources(raw):
    try:
        with tarfile.open(fileobj=io.BytesIO(raw), mode="r:gz") as archive:
            names = [member.name for member in archive.getmembers() if member.isfile()]
            required = (("fftw3/source/", "/api/fftw3.h"), ("pthreads/source/", "/pthread.h"),
                        ("fftw3/vcpkg-port/", "/portfile.cmake"), ("pthreads/vcpkg-port/", "/portfile.cmake"),
                        ("licenses-and-build-metadata/", "/fftw3/copyright"),
                        ("licenses-and-build-metadata/", "/pthreads/copyright"))
            if not all(any(name.startswith(prefix) and name.endswith(suffix) for name in names)
                       for prefix, suffix in required):
                raise ValueError("Windows corresponding FFTW/pthreads sources or licenses missing")
    except tarfile.TarError as exc:
        raise ValueError("Malformed Windows corresponding dependency source archive") from exc


def verify_cava_source(raw):
    expected = {source.relative_to(ROOT / "vendor/cava").as_posix(): source.read_bytes()
                for source in (ROOT / "vendor/cava").rglob("*") if source.is_file()}
    try:
        with tarfile.open(fileobj=io.BytesIO(raw), mode="r:gz") as source_archive:
            members = source_archive.getmembers()
            names = [member.name for member in members]
            if len(names) != len(set(names)) or set(names) != {"cava/" + name for name in expected}:
                raise ValueError("Bundled CAVA corresponding source differs from checkout")
            for member in members:
                if not member.isfile() or source_archive.extractfile(member).read() != expected[member.name.removeprefix("cava/")]:
                    raise ValueError("Bundled CAVA corresponding source differs from checkout")
    except tarfile.TarError as exc:
        raise ValueError("Malformed CAVA source archive") from exc


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
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9._/-]+)", line)
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
    common_static = expected_static_files("linux-x86_64", commit)
    firmware = json.loads(common_static["firmware-provenance.json"])
    result = {"format": "ak-configurator-release-candidate-1", "application_version": version,
              "source_commit": commit, "status": "unsigned desktop trial; hardware and platform qualification remain separate",
              "platforms": {}}
    for platform in EXPECTED:
        expected_static = expected_static_files(platform, commit)
        suffix = ".app.zip" if platform.startswith("macos-") else (".tar.gz" if platform.startswith("linux-") else ".zip")
        name = f"ak-configurator-{version}-{platform}{suffix}"
        archive = directory / name
        checksum = directory / f"{name}.sha256"
        if not archive.is_file() or not checksum.is_file():
            raise ValueError(f"Missing candidate archive or checksum for {platform}")
        archive_hash = sha256(archive.read_bytes())
        if checksum.read_text(encoding="ascii").strip() != f"{archive_hash}  {name}":
            raise ValueError(f"Checksum mismatch for {platform}")
        if platform.startswith("linux-"):
            package = tarfile.open(archive, "r:gz")
            infos = package.getmembers()
            names = [item.name for item in infos]
            read_member = lambda member: package.extractfile(member).read()
        else:
            package = zipfile.ZipFile(archive)
            infos = package.infolist()
            names = [item.filename for item in infos]
            read_member = package.read
        with package:
            root = f"ak-configurator-{version}-{platform}/"
            if len(names) != len(set(names)):
                raise ValueError(f"Duplicate archive member for {platform}")
            for member in names:
                parts = PurePosixPath(member).parts
                if "\\" in member or member.startswith("/") or any(part in ("", ".", "..") for part in parts):
                    raise ValueError(f"Unsafe ZIP member path for {platform}")
                if not member.startswith(root):
                    raise ValueError(f"Unexpected ZIP member path for {platform}")
            binary_name = "ak-configurator.exe" if platform.startswith("windows-") else "ak-configurator"
            helper_platform = TARGETS[platform][0]
            helper_suffix = ".ps1" if helper_platform == "windows" else ".sh"
            helper_names = {f"{kind}-user{helper_suffix}" for kind in ("install", "uninstall")}
            cava_name = ("AK-Configurator.app/Contents/MacOS/" if platform.startswith("macos-") else "") + "audio/cava" + (".exe" if platform.startswith("windows-") else "")
            expected_names = {cava_name, "audio-sources/cava-patched-source.tar.gz"} | set(expected_static) | helper_names | {binary_name, "manifest.json", "SHA256SUMS", "runtime-self-check.json"}
            if platform.startswith("windows-"):
                expected_names.add("audio-sources/windows-built-dependencies.tar.gz")
            app_binary = "AK-Configurator.app/Contents/MacOS/ak-configurator"
            if platform.startswith("macos-"):
                expected_names.remove(binary_name)
                expected_names |= {app_binary, "AK-Configurator.app/Contents/Info.plist", "AK-Configurator.app/Contents/Resources/audio-sources/cava-patched-source.tar.gz"}
            if {member[len(root):] for member in names} != expected_names:
                raise ValueError(f"Candidate package has missing or unexpected members for {platform}")
            contents = {member[len(root):]: read_member(member) for member in names}
        for filename, expected in expected_static.items():
            if contents.get(filename) != expected:
                raise ValueError(f"Bundled source file differs from checked-out source: {filename}")
        verify_internal_checksums(contents)
        verify_cava_source(contents["audio-sources/cava-patched-source.tar.gz"])
        if platform.startswith("windows-"):
            verify_windows_audio_sources(contents["audio-sources/windows-built-dependencies.tar.gz"])
        if platform.startswith("macos-") and contents["AK-Configurator.app/Contents/Resources/audio-sources/cava-patched-source.tar.gz"] != contents["audio-sources/cava-patched-source.tar.gz"]:
            raise ValueError("Installed macOS CAVA source archive differs")
        try:
            runtime = json.loads(contents["runtime-self-check.json"])
        except (KeyError, json.JSONDecodeError) as exc:
            raise ValueError(f"Missing or malformed runner self-check for {platform}") from exc
        if not isinstance(runtime, dict):
            raise ValueError(f"Runner self-check is not a JSON object for {platform}")
        expected_runtime = expected_runtime_identity(platform)
        normalized_runtime = normalize_runtime_profiles(runtime)
        if (runtime.get("format") != "ak-configurator-self-check-1" or runtime.get("platform") != platform
                or runtime.get("application_version") != version or runtime.get("status") != "passed"
                or runtime.get("hardware_access") is not False or runtime.get("network_access") is not False
                or runtime.get("firmware_release") != expected_runtime["firmware_release"]
                or runtime.get("catalog_sha256") != expected_runtime["catalog_sha256"]
                or normalized_runtime.get("profiles") != expected_runtime["profiles"]):
            raise ValueError(f"Runner self-check identity or result is invalid for {platform}")
        smoke_path = directory / f"install-smoke-{platform}.json"
        try:
            smoke = json.loads(smoke_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            raise ValueError(f"Missing or malformed native installed-package smoke result for {platform}") from exc
        if not isinstance(smoke, dict) or not isinstance(smoke.get("installed_self_check"), dict):
            raise ValueError(f"Native installed-package smoke result is not a JSON object for {platform}")
        if (smoke.get("format") != "ak-configurator-installed-smoke-1" or smoke.get("platform") != platform
                or smoke.get("user_data_preserved") is not True or smoke.get("uninstalled") is not True
                or smoke.get("archive_sha256") != archive_hash
                or smoke.get("audio_helper_version") != "cava 1.0.0"
                or smoke.get("upgrade_verified") is not True
                or normalize_runtime_profiles(smoke.get("installed_self_check", {})) != normalized_runtime):
            raise ValueError(f"Native installed-package smoke result is invalid for {platform}")
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
        binary = contents[app_binary if platform.startswith("macos-") else binary_name]
        if manifest.get("binary_sha256") != sha256(binary):
            raise ValueError(f"Binary digest mismatch for {platform}")
        if smoke.get("audio_helper_sha256") != sha256(contents[cava_name]):
            raise ValueError(f"Installed helper digest mismatch for {platform}")
        if smoke.get("binary_sha256") != manifest.get("binary_sha256"):
            raise ValueError(f"Installed smoke binary digest mismatch for {platform}")
        check_architecture(binary, platform)
        check_architecture(contents[cava_name], platform)
        cava_identity = json.loads((ROOT / "vendor/cava-provenance.json").read_text())
        if manifest.get("audio_helper") != {**cava_identity, "path": cava_name, "sha256": sha256(contents[cava_name])}:
            raise ValueError(f"CAVA helper identity mismatch for {platform}")
        result["platforms"][platform] = {"archive": name, "sha256": archive_hash,
                                         "runtime_self_check": runtime, "installation_smoke": smoke}
    if set(result["platforms"]) != set(EXPECTED):
        raise ValueError("Candidate platform set is incomplete")
    index = {
        "schema_version": 1,
        "tag": f"v{version}",
        "version": version,
        "prerelease": "-" in version,
        "assets": [],
    }
    for platform in EXPECTED:
        archive = result["platforms"][platform]["archive"]
        os_name, arch, target = TARGETS[platform]
        index["assets"].append({
            "name": archive, "target": target, "platform": os_name, "arch": arch,
            "url": f"https://github.com/dvino/ak-configurator/releases/download/v{version}/{archive}",
            "sha256": result["platforms"][platform]["sha256"],
            "size": (directory / archive).stat().st_size,
        })
    result["app_update_index"] = index
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--update-index", type=Path)
    args = parser.parse_args()
    try:
        report = verify(args.directory, args.version, args.source_commit)
        args.manifest.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        if args.update_index:
            args.update_index.write_text(json.dumps(report["app_update_index"], indent=2, sort_keys=True) + "\n", encoding="utf-8")
    except (OSError, ValueError, subprocess.CalledProcessError, zipfile.BadZipFile) as exc:
        parser.exit(1, f"Release candidate rejected: {exc}\n")


if __name__ == "__main__":
    main()
