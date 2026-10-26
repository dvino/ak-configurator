#!/usr/bin/env python3
"""Install, run the device-free self-check, and uninstall a candidate on its native runner."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import stat
import tempfile
import tarfile
import zipfile
import uuid


def run(command, *, env, input_text=None):
    result = subprocess.run(command, env=env, input=input_text, text=True, capture_output=True, timeout=180)
    if result.returncode:
        raise RuntimeError(
            f"Command failed ({result.returncode}): {command!r}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def extract(archive, destination):
    if archive.suffixes[-2:] == [".tar", ".gz"]:
        with tarfile.open(archive, "r:gz") as package:
            members = package.getmembers()
            for member in members:
                name = member.name
                parts = PurePosixPath(name).parts
                if name.startswith("/") or "\\" in name or ":" in name or any(p in ("", ".", "..") for p in parts):
                    raise ValueError(f"Unsafe package path: {name}")
                if not (member.isfile() or member.isdir()):
                    raise ValueError(f"Unsupported archive member type: {name}")
            for member in members:
                target = destination.joinpath(*PurePosixPath(member.name).parts)
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    stream = package.extractfile(member)
                    if stream is None:
                        raise ValueError(f"Cannot read archive member: {member.name}")
                    target.write_bytes(stream.read())
                    target.chmod(member.mode & 0o777)
    else:
        with zipfile.ZipFile(archive) as package:
            infos = package.infolist()
            for info in infos:
                name = info.filename
                parts = PurePosixPath(name).parts
                if name.startswith("/") or "\\" in name or ":" in name or any(p in ("", ".", "..") for p in parts):
                    raise ValueError(f"Unsafe package path: {name}")
                mode = (info.external_attr >> 16) & 0xffff
                if stat.S_ISLNK(mode) or not (info.is_dir() or stat.S_IFMT(mode) in (0, stat.S_IFREG, stat.S_IFDIR)):
                    raise ValueError(f"Unsupported archive member type: {name}")
            for info in infos:
                target = destination.joinpath(*PurePosixPath(info.filename).parts)
                if info.is_dir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(package.read(info))
                    mode = (info.external_attr >> 16) & 0o777
                    if mode:
                        target.chmod(mode)


def verify_check(path, platform):
    result = json.loads(path.read_text(encoding="utf-8"))
    if result.get("format") != "ak-configurator-self-check-1" or result.get("platform") != platform or result.get("status") != "passed":
        raise ValueError("Installed executable did not produce a passing self-check for this target")
    if result.get("hardware_access") is not False or result.get("network_access") is not False:
        raise ValueError("Installed executable self-check reports prohibited device or network access")
    return result


def exercise(archive, platform, report_path):
    with tempfile.TemporaryDirectory(prefix="ak-configurator-install-smoke-") as temporary:
        root = Path(temporary)
        extract(archive, root)
        bundle = next(root.iterdir())
        env = os.environ.copy()
        check_path = root / "installed-self-check.json"
        if platform.startswith("linux-"):
            home = root / "home"
            xdg_data, xdg_bin = home / ".local/share", home / ".local/bin"
            env.update(XDG_DATA_HOME=str(xdg_data), XDG_BIN_HOME=str(xdg_bin), XDG_CONFIG_HOME=str(home / ".config"))
            run(["sh", str(bundle / "install-user.sh")], env=env)
            installed = xdg_data / "ak-configurator/current/ak-configurator"
            uninstall = xdg_data / "ak-configurator/current/uninstall-user.sh"
            data = Path(env["XDG_CONFIG_HOME"]) / "ak-configurator" / "app_settings.json"
        elif platform.startswith("macos-"):
            home = root / "home"
            app_root = root / "mac-applications"
            env["AK_CONFIGURATOR_APPLICATIONS_DIR"] = str(app_root)
            run(["sh", str(bundle / "install-user.sh")], env=env)
            installed = app_root / "AK-Configurator.app/Contents/MacOS/ak-configurator"
            uninstall = app_root / "AK-Configurator.app/Contents/Resources/uninstall-user.sh"
            sentinel_dir = Path.home() / "Library/Application Support/ak-configurator" / (".install-smoke-" + uuid.uuid4().hex)
            data = sentinel_dir / "app_settings.json"
        else:
            home = root / "home"
            local, roaming = home / "AppData/Local", home / "AppData/Roaming"
            env.update(LOCALAPPDATA=str(local), APPDATA=str(roaming))
            run(["powershell", "-NoProfile", "-File", str(bundle / "install-user.ps1")], env=env)
            installed = local / "Programs/AK Configurator/ak-configurator.exe"
            uninstall = local / "Programs/AK Configurator/uninstall-user.ps1"
            data = roaming / "ak-configurator/app_settings.json"
        if not installed.is_file() or not uninstall.is_file():
            raise ValueError("Installer did not place the executable and uninstaller at their documented paths")
        expected_binary_hash = json.loads((bundle / "manifest.json").read_text(encoding="utf-8"))["binary_sha256"]
        if hashlib.sha256(installed.read_bytes()).hexdigest() != expected_binary_hash:
            raise ValueError("Installed executable digest differs from the verified archive manifest")
        helper = installed.parent / "audio" / ("cava.exe" if platform.startswith("windows-") else "cava")
        expected_helper_hash = json.loads((bundle / "manifest.json").read_text(encoding="utf-8"))["audio_helper"]["sha256"]
        if not helper.is_file() or hashlib.sha256(helper.read_bytes()).hexdigest() != expected_helper_hash:
            raise ValueError("Installed CAVA helper differs from package identity")
        helper_version = run([str(helper), "-v"], env=env).stdout.strip()
        if helper_version != "cava 1.0.0":
            raise ValueError("Installed CAVA helper did not execute its device-free version probe")
        legal_root = installed.parent.parent / "Resources" if platform.startswith("macos-") else installed.parent
        if not (legal_root / "CAVA-LICENSE").is_file() or not (legal_root / "audio-sources/cava-patched-source.tar.gz").is_file():
            raise ValueError("Installer did not preserve CAVA license and corresponding source")
        data.parent.mkdir(parents=True, exist_ok=True)
        data.write_text("preserve", encoding="utf-8")
        installer = bundle / ("install-user.ps1" if platform.startswith("windows-") else "install-user.sh")
        if platform.startswith("windows-"):
            run(["powershell", "-NoProfile", "-File", str(installer)], env=env)
        else:
            run(["sh", str(installer)], env=env)
        if not installed.is_file() or hashlib.sha256(installed.read_bytes()).hexdigest() != expected_binary_hash:
            raise ValueError("Updated executable digest differs from the verified archive manifest")
        if data.read_text(encoding="utf-8") != "preserve":
            raise ValueError("Update removed user data outside the application installation")
        run([str(installed), "--self-check", "--output", str(check_path)], env=env)
        check = verify_check(check_path, platform)
        if platform.startswith("windows-"):
            run(["powershell", "-NoProfile", "-File", str(uninstall)], env=env, input_text="y\n")
        else:
            run(["sh", str(uninstall)], env=env, input_text="y\n")
        if not data.is_file() or data.read_text(encoding="utf-8") != "preserve":
            raise ValueError("Uninstall removed user data outside the application installation")
        if platform.startswith("linux-") and (xdg_data / "ak-configurator/current").exists():
            raise ValueError("Linux uninstaller left its owned application directory installed")
        if platform.startswith("macos-") and (app_root / "AK-Configurator.app").exists():
            raise ValueError("macOS uninstaller left its owned application installed")
        if platform.startswith("windows-") and (local / "Programs/AK Configurator").exists():
            raise ValueError("Windows uninstaller left its owned application installed")
        if platform.startswith("macos-"):
            shutil.rmtree(sentinel_dir)
        report = {"format": "ak-configurator-installed-smoke-1", "platform": platform,
                  "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
                  "binary_sha256": expected_binary_hash,
                  "audio_helper_sha256": expected_helper_hash, "audio_helper_version": helper_version,
                  "installed_self_check": check, "user_data_preserved": True,
                  "upgrade_verified": True, "uninstalled": True}
        report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--platform", required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    exercise(args.archive, args.platform, args.report)


if __name__ == "__main__":
    main()
