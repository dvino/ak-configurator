"""Pinned public beta.5 inputs for offline candidate packaging and verification."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROFILES = ("main", "game", "mmo", "typing", "code", "pointer", "onehand", "creative", "shortcuts", "multios")
RELEASE = "v0.6.0-beta.5"
PINS = {
    "release-catalog.json": (31614, "9a033d944db3282120df5b68c0c0c2a3c3a370d103e9482b95c66d673a43882d"),
    "capability-catalog.json": (74704, "a1b489026fced8135d424069bb1ed80f6e975468804a2a2cec5aae72047dda91"),
    "firmware-matrix.json": (1750531, "12132c84d7779d7433c535c89eaf1a99ffc2f662ec2c8d93d2807d8afb2ad7cb"),
    "module-descriptions.json": (61695, "52ee9cfc278ac3524b0da5f6757279cf479a17ccf84dcdf987b63e84b1b671a4"),
}

def sha256(data):
    return hashlib.sha256(data).hexdigest()

def public_inputs(root=ROOT):
    base = root / "assets/ak820"
    files = {}
    for name, (size, digest) in PINS.items():
        data = (base / ("beta5-" + name)).read_bytes()
        if len(data) != size or sha256(data) != digest:
            raise ValueError(f"Bundled firmware catalog differs: {name}")
        files["offline/" + name] = data
    catalog = json.loads(files["offline/release-catalog.json"])
    if catalog.get("tag") != RELEASE or set(catalog.get("catalog_profiles", [])) != set(PROFILES) or len(catalog["catalog_profiles"]) != len(PROFILES):
        raise ValueError("Unexpected release catalog profiles")
    directory = base / "firmware-beta5"
    expected = {f"{profile}/application.{ext}" for profile in PROFILES for ext in ("ihx", "map")}
    actual = {p.relative_to(directory).as_posix() for p in directory.rglob("*") if p.is_file()}
    if actual != expected:
        raise ValueError("Unexpected bundled firmware asset set")
    for name in sorted(expected):
        data = (directory / name).read_bytes()
        profile, filename = name.split("/")
        entry = catalog["assets"].get(f"profiles/{profile}/firmware/{filename}")
        if not isinstance(entry, dict) or entry.get("size") != len(data) or entry.get("sha256") != sha256(data):
            raise ValueError(f"Bundled public firmware asset differs: {name}")
        files["offline/firmware-beta5/" + name] = data
    return files

def provenance():
    return {
        "format": "ak-configurator-public-firmware-provenance-1",
        "source": "App-owned record of accepted GitHub release API metadata and pinned public release assets; not an official firmware manifest.",
        "release": RELEASE,
        "release_url": "https://github.com/dvino/ak820/releases/tag/v0.6.0-beta.5",
        "tag_object": "1d960eabb5fd9abf078e6815b0c5c01bfb13a803",
        "tag_commit": "dfd0770f9ef64bf5c7ab6d40e9f9e7b4354cd2cd",
        "assets": {name: {"size": size, "sha256": digest} for name, (size, digest) in PINS.items()},
    }

def provenance_bytes():
    return (json.dumps(provenance(), indent=2, sort_keys=True) + "\n").encode()

def runtime_identity():
    files = public_inputs()
    # App-derived installer defaults, including the official migration's V2
    # settings record. This is not an official fixture or erased-QS C defaults.
    defaults = json.loads((ROOT / "tests/fixtures/ak820-beta5-installer-defaults.json").read_text(encoding="utf-8"))
    if set(defaults) != set(PROFILES) or any(v != "a5d91c5619527fc5b0ebb9fd1117aa8259f17e1ac9212a063599711a4b129a46" for v in defaults.values()):
        raise ValueError("Invalid derived installer defaults fixture")
    profiles = []
    for profile in sorted(PROFILES):
        folder = f"offline/firmware-beta5/{profile}/application."
        profiles.append({"profile": profile, "ihx_sha256": sha256(files[folder + "ihx"]),
                         "map_sha256": sha256(files[folder + "map"]),
                         "default_settings_sha256": defaults[profile]})
    return {"firmware_release": RELEASE, "catalog_sha256": sha256(files["offline/release-catalog.json"]), "profiles": profiles}
