"""Release boundary tests; no SDK, signing key or game assets required."""
import tempfile
import unittest
from pathlib import Path
import xml.etree.ElementTree as ET
from build_probe import ANDROID, manifest_for_build


class BuildManifestTests(unittest.TestCase):
    def test_debug_and_release_separate_probe_access(self):
        with tempfile.TemporaryDirectory() as directory:
            for variant in ('debug', 'release'):
                with self.subTest(variant=variant):
                    path = Path(directory)/'manifest.xml'
                    manifest_for_build(variant, 7, '0.1.0-preview.7', path)
                    root = ET.parse(path).getroot()
                    self.assertEqual(root.get(ANDROID+'versionCode'), '7')
                    self.assertEqual(root.get(ANDROID+'versionName'), '0.1.0-preview.7')
                    self.assertEqual(root.get('package'), 'org.nekopack.probe')
                    app = root.find('application')
                    expected = str(variant == 'debug').lower()
                    self.assertEqual(app.get(ANDROID+'debuggable'), expected)
                    activities = {a.get(ANDROID+'name'): a for a in app.findall('activity')}
                    self.assertEqual(activities['.MainActivity'].get(ANDROID+'exported'), expected)
                    self.assertEqual(activities['.LibraryActivity'].get(ANDROID+'exported'), 'true')
                    self.assertEqual(app.get(ANDROID+'allowBackup'), 'false')


if __name__ == '__main__':
    unittest.main()
