import hashlib
import json
import unittest
from pathlib import Path


ASSETS = Path(__file__).resolve().parents[2] / 'assets/ak820'
DIGESTS = {
    'release-catalog.json': '9a033d944db3282120df5b68c0c0c2a3c3a370d103e9482b95c66d673a43882d',
    'capability-catalog.json': 'a1b489026fced8135d424069bb1ed80f6e975468804a2a2cec5aae72047dda91',
    'firmware-matrix.json': '12132c84d7779d7433c535c89eaf1a99ffc2f662ec2c8d93d2807d8afb2ad7cb',
    'module-descriptions.json': '52ee9cfc278ac3524b0da5f6757279cf479a17ccf84dcdf987b63e84b1b671a4',
}
PROFILES = {'main', 'game', 'mmo', 'typing', 'code', 'pointer', 'onehand', 'creative', 'shortcuts', 'multios'}


class Beta5CatalogTest(unittest.TestCase):
    def test_published_catalog_contract(self):
        documents = {}
        for name, digest in DIGESTS.items():
            raw = (ASSETS / ('beta5-' + name)).read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(), digest, name)
            documents[name] = json.loads(raw)

        release = documents['release-catalog.json']
        capability = documents['capability-catalog.json']
        matrix = documents['firmware-matrix.json']
        descriptions = documents['module-descriptions.json']
        self.assertEqual(release['tag'], 'v0.6.0-beta.5')
        for name in DIGESTS.keys() - {'release-catalog.json'}:
            self.assertEqual(release['assets'][name]['sha256'], DIGESTS[name])
        for item in [release, capability, *capability['images']]:
            for flag, expected in [('flashing_authorized', True), ('release_approved', True),
                                   ('hardware_validated', False), ('settings_writes_authorized', False)]:
                self.assertIs(item[flag], expected)

        def indexed(items, key):
            result = {item[key]: item for item in items}
            self.assertEqual(len(items), len(result))
            self.assertEqual(set(result), PROFILES)
            return result

        authorized = indexed(release['flashing_authorized_images'], 'profile')
        images = indexed(capability['images'], 'profile')
        profiles = indexed(matrix['profiles'], 'id')
        historical = indexed(matrix['historical_profiles'], 'id')
        indexed(descriptions['profiles'], 'profile')
        self.assertEqual(set(release['catalog_profiles']), PROFILES)
        self.assertEqual(set(release['installable_profiles']), PROFILES)
        for name in PROFILES:
            image = images[name]
            profile = profiles[name]
            ihx = authorized[name]['ihx_sha256']
            self.assertEqual(len(ihx), 64)
            self.assertEqual(image['build_identity']['profile'], name)
            self.assertEqual(profile['identity']['profile'], name)
            self.assertEqual(profile['profile'], name)
            self.assertEqual(ihx, image['build_identity']['ihx_sha256'])
            self.assertEqual(ihx, profile['identity']['ihx_sha256'])
            self.assertEqual(ihx, profile['settings_identity']['ihx_sha256'])
            self.assertEqual(authorized[name]['code_sha256'], profile['settings_identity']['code_sha256'])
            self.assertEqual(image['build_identity']['map_sha256'], profile['identity']['map_sha256'])
            self.assertEqual(image['settings_schema'], profile['settings_identity']['nvm_schema'])
            self.assertEqual(sorted(image['runtime_qmk_settings']),
                             sorted(profile['capabilities']['runtime_qmk_settings_ids']))

        # Beta.4 and beta.4.1 share these exact image identities. The
        # independently pinned beta.4.1 release binds each profile to its IHX.
        beta4_raw = (ASSETS / 'beta4.1-release-catalog.json').read_bytes()
        self.assertEqual(hashlib.sha256(beta4_raw).hexdigest(),
                         '5ec2dbddfb2d66d2afa13a408f0365d2474d8f94866f700d65d124eeb13f05c1')
        beta4 = indexed(json.loads(beta4_raw)['flashing_authorized_images'], 'profile')
        old = {beta4[name]['ihx_sha256'] for name in PROFILES}
        new = {authorized[name]['ihx_sha256'] for name in PROFILES}
        self.assertEqual(len(old), 10)
        self.assertEqual(len(new), 10)
        self.assertTrue(old.isdisjoint(new))
        self.assertEqual(old, {historical[name]['identity']['ihx_sha256'] for name in PROFILES})
        transitions = matrix['settings_transitions']
        pairs = {(item['source'], item['target']) for item in transitions}
        self.assertEqual(len(transitions), 200)
        self.assertEqual(len(pairs), 200)
        self.assertEqual(pairs, {(source, target) for source in old | new for target in new})

        # No beta.3 or unknown identity can enter that closed pair set.
        beta3 = json.loads((ASSETS / 'beta3-capability-catalog.json').read_bytes())
        beta3_hashes = {item['build_identity']['ihx_sha256'] for item in beta3['images']}
        self.assertTrue(beta3_hashes.isdisjoint(old | new))
        for source in beta3_hashes | {'0' * 64}:
            self.assertTrue(all((source, target) not in pairs for target in new))


if __name__ == '__main__':
    unittest.main()
