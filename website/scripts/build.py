"""Build static release content from GitHub's API response; no runtime API dependency."""
import html
import json
import re
import shutil
import sys
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]

def build(release, output):
    if release.get('draft') or release.get('prerelease'):
        raise ValueError('Only published stable releases may be deployed')
    tag = release['tag_name']
    if not re.fullmatch(r'v?\d+\.\d+\.\d+', tag):
        raise ValueError('Expected a stable semantic version')
    def safe_url(url):
        parsed = urlparse(url)
        if parsed.scheme != 'https' or parsed.netloc != 'github.com' or not parsed.path.startswith('/pepperstm/velopopuli/releases/'):
            raise ValueError('Unexpected release URL')
        return html.escape(url, quote=True)
    values = {'TAG': html.escape(tag), 'VERSION': html.escape(tag.removeprefix('v')),
              'RELEASE_URL': safe_url(release['html_url'])}
    for platform, suffix in [('MACOS', '_universal.dmg'), ('WINDOWS', '_x64-setup.exe'), ('LINUX', '_amd64.AppImage')]:
        assets = [a for a in release['assets'] if a['name'].startswith('VeloPopuli_') and a['name'].endswith(suffix)]
        if len(assets) != 1:
            raise ValueError(f'Expected one {platform} installer; refusing incomplete deployment')
        values[platform + '_URL'] = safe_url(assets[0]['browser_download_url'])
    # Escape the complete Markdown body as text: release HTML can never execute.
    values['RELEASE_NOTES'] = '<pre style="white-space:pre-wrap;font:inherit;line-height:1.7">' + html.escape(release.get('body') or 'See the full release notes on GitHub.') + '</pre>'
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    shutil.copytree(ROOT / 'source', output, dirs_exist_ok=True)
    template = (ROOT / 'source/index.html').read_text()
    for key, value in values.items():
        template = template.replace('{{' + key + '}}', value)
    if re.search(r'\{\{[A-Z_]+\}\}', template):
        raise ValueError('Unresolved template variable')
    (output / 'index.html').write_text(template)
    (output / 'release.json').write_text(json.dumps({'tag': tag, 'url': release['html_url']}))

if __name__ == '__main__':
    build(json.loads(Path(sys.argv[1]).read_text()), sys.argv[2])
