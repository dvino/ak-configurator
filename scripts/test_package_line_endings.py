#!/usr/bin/env python3
"""Check raw package inputs under actual Git checkout conversion settings."""
import ast
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def git(*args, cwd=ROOT):
    return subprocess.run(
        ["git", *args], cwd=cwd, check=True, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, env={**os.environ, "GIT_CONFIG_NOSYSTEM": "1",
                                     "GIT_CONFIG_GLOBAL": os.devnull},
    ).stdout


def static_sources():
    tree = ast.parse((ROOT / "scripts/verify_ak820_release_candidate.py").read_text(encoding="utf-8"))
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(
            isinstance(target, ast.Name) and target.id == "STATIC_FILES" for target in node.targets
        ):
            return set(ast.literal_eval(node.value).values())
    raise AssertionError("STATIC_FILES not found")


class PackageLineEndings(unittest.TestCase):
    def test_checkout_bytes_are_platform_independent(self):
        # Guides are read as text and re-encoded by package_document; these are
        # all the remaining raw static files and platform install helpers.
        sources = static_sources() | {
            path.relative_to(ROOT).as_posix()
            for path in (ROOT / "scripts/ak820-release").glob("*/*") if path.is_file()
        }
        self.assertEqual(len([p for p in sources if p.startswith("scripts/ak820-release/")]), 6)
        opaque = {
            "assets/ak820/fixture.txt": b"firmware\r\nsecond line\r\n",
            "vendor/cava/fixture.txt": b"cava\r\nsecond line\r\n",
            "vendor/cava-dependencies/fixture.txt": b"dependency\r\nsecond line\r\n",
            "assets/fixture.ico": b"\x00\x00\x01\x00\r\n\xff",
        }
        with tempfile.TemporaryDirectory(prefix="ak-package-eol-") as tmp:
            repo = Path(tmp) / "source"
            repo.mkdir()
            git("init", "-q", cwd=repo)
            (repo / ".gitattributes").write_bytes((ROOT / ".gitattributes").read_bytes())
            expected = dict(opaque)
            for path in sources:
                expected[path] = git("show", f"HEAD:{path}")
            for name, data in expected.items():
                dest = repo / name
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes(data)
            git("-c", "core.autocrlf=false", "add", ".", cwd=repo)
            git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                "commit", "-qm", "fixture", cwd=repo)
            for setting in ("true", "false"):
                checkout = Path(tmp) / f"checkout-{setting}"
                git("clone", "-q", "-c", f"core.autocrlf={setting}", str(repo), str(checkout), cwd=Path(tmp))
                for name, data in expected.items():
                    with self.subTest(autocrlf=setting, path=name):
                        self.assertEqual((checkout / name).read_bytes(), data)


if __name__ == "__main__":
    unittest.main()
