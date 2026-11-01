import copy
import tempfile
import unittest
from pathlib import Path
from build import build

BASE = 'https://github.com/pepperstm/velopopuli/releases/'
class ReleaseBuildTests(unittest.TestCase):
    def setUp(self):
        self.release = {'tag_name':'v1.2.3', 'html_url':BASE+'tag/v1.2.3', 'body':'<script>unsafe()</script>', 'assets':[
            {'name':'VeloPopuli_1.2.3'+suffix, 'browser_download_url':BASE+'download/v1.2.3/VeloPopuli_1.2.3'+suffix}
            for suffix in ('_universal.dmg','_x64-setup.exe','_amd64.AppImage')]}
    def test_release_content_and_historical_features(self):
        with tempfile.TemporaryDirectory() as d:
            build(self.release,d)
            page = Path(d,'index.html').read_text()
            self.assertIn('VeloPopuli 1.2.3 is here.',page)
            self.assertIn('Download v1.2.3',page)
            self.assertIn('NEW IN v0.3.5 / BLEEDING EDGE',page)
            self.assertIn('&lt;script&gt;unsafe()&lt;/script&gt;',page)
            self.assertNotIn('<script>unsafe()</script>',page)
            for asset in self.release['assets']:
                self.assertIn(asset['browser_download_url'],page)
    def test_reject_unsafe_or_incomplete_releases(self):
        for key,value in [('draft',True),('prerelease',True),('assets',[]),('tag_name','v1.2.3-beta'),('html_url','https://evil.example/')]:
            with self.subTest(key=key), tempfile.TemporaryDirectory() as d:
                release=copy.deepcopy(self.release);release[key]=value
                with self.assertRaises(ValueError):build(release,d)

if __name__ == '__main__':unittest.main()
