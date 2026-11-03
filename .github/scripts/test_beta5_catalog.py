import hashlib
import json
import unittest
from pathlib import Path


ASSETS = Path(__file__).resolve().parents[2] / 'assets/ak820'
DIGESTS = {
    'release-catalog.json': '53edcdd9261bd545dac031b0efb79b4faf6fe7c29c5332dc5e1bd63daacfee6d',
    'capability-catalog.json': '3479a75b1b9ed2b0db3f3779f0110dd3ba7db1a3990d16340fb1bd440d160b9a',
    'firmware-matrix.json': 'd96e04011086bd34897f7afc0fd0056ebd530252ecfdec8f131349010e95c8a0',
    'module-descriptions.json': 'f6074eafcedbfd0c605ee4f58aa1b458170a2f98942924deb158c555f6ce03a4',
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


if __name__ == '__main__':
    unittest.main()
