#!/usr/bin/env python3
"""Check the pinned beta.4 candidate inputs; never grant device authority."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FILES = {
    'capability-catalog.json': 'f33698e6329a66a3e4ade069d1eda50dfc45882109112860e76cebe41cf165c2',
    'firmware-matrix.json': '8bf90526a51e9879870a61d7de1c59b1df38e3489b77a67e67496503f0307658',
}
PROFILES = {'main', 'game', 'mmo', 'typing', 'code', 'pointer', 'onehand', 'creative', 'shortcuts', 'multios'}


def check(directory):
    data = {}
    for name, digest in FILES.items():
        raw = (directory / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != digest:
            raise ValueError(f'{name}: SHA256 differs from pinned candidate')
        data[name] = json.loads(raw)
    catalog, matrix = data['capability-catalog.json'], data['firmware-matrix.json']
    if catalog['status'] != 'development-only' or any(catalog[k] is not False for k in ('release_approved', 'hardware_validated', 'flashing_authorized', 'settings_writes_authorized')):
        raise ValueError('catalog approval flags differ')
    def index(rows, name):
        result = {row['profile']: row for row in rows}
        if len(rows) != 10 or set(result) != PROFILES:
            raise ValueError(f'{name}: profile set differs')
        return result
    images = index(catalog['images'], 'catalog')
    current = index(matrix['profiles'], 'current')
    previous = index(matrix['historical_profiles'], 'historical')
    for image in images.values():
        if image['status'] != 'development' or any(image[k] is not False for k in ('release_approved', 'hardware_validated', 'flashing_authorized', 'settings_writes_authorized')):
            raise ValueError('image approval flags differ')
    if any(p['release_approved'] is not False or p['installable'] is not False for p in current.values()):
        raise ValueError('candidate matrix approval flags differ')
    published = index(json.loads((ROOT / 'assets/ak820/beta3-firmware-matrix.json').read_bytes())['profiles'], 'published beta.3')
    if any(previous[p]['settings_identity']['ihx_sha256'] != published[p]['settings_identity']['ihx_sha256'] for p in PROFILES):
        raise ValueError('historical identities differ from bundled beta.3')
    old = {p['settings_identity']['ihx_sha256'] for p in previous.values()}
    new = {p['settings_identity']['ihx_sha256'] for p in current.values()}
    if len(old) != 10 or len(new) != 10 or old & new:
        raise ValueError('ambiguous matrix identities')
    pairs = [(p['source'], p['target']) for p in matrix['settings_transitions']]
    expected = {(a, b) for a in old | new for b in new}
    if len(pairs) != 200 or set(pairs) != expected:
        raise ValueError('transition matrix differs')
    mismatch = [p for p in sorted(PROFILES) if images[p]['build_identity']['ihx_sha256'] != current[p]['settings_identity']['ihx_sha256']]
    historical = [p for p in mismatch if images[p]['build_identity']['ihx_sha256'] == previous[p]['settings_identity']['ihx_sha256']]
    if mismatch:
        raise ValueError(f'candidate capability identities mismatch: {len(mismatch)}/10; match historical beta.3: {len(historical)}/10 ({", ".join(mismatch)})')
    return 'Candidate metadata agrees; this check does not authorize installation or writes.'


if __name__ == '__main__':
    try:
        print(check(ROOT / 'assets/beta4-candidate'))
    except (ValueError, KeyError, OSError, json.JSONDecodeError) as error:
        raise SystemExit(f'BLOCKED: {error}')
