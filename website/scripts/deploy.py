"""Deploy only managed files over SFTP, checking the pinned SSH host key."""
import os
import posixpath
import tempfile
from pathlib import Path
import paramiko

root = Path('website/build')
remote = os.environ['IONOS_PATH'].rstrip('/') or '/'
if not remote.startswith('/') or '..' in remote.split('/'):
    raise ValueError('IONOS_PATH must be an absolute directory')
with tempfile.NamedTemporaryFile(mode='w') as known:
    known.write(os.environ['IONOS_KNOWN_HOSTS'] + '\n')
    known.flush()
    with paramiko.SSHClient() as client:
        client.load_host_keys(known.name)
        client.set_missing_host_key_policy(paramiko.RejectPolicy())
        client.connect(os.environ['IONOS_HOST'], port=22, username=os.environ['IONOS_USER'], password=os.environ['IONOS_PASSWORD'], look_for_keys=False, allow_agent=False, timeout=30)
        with client.open_sftp() as sftp:
            sftp.stat(remote)  # Never create a guessed deployment root.
            files = sorted(p for p in root.rglob('*') if p.is_file())
            files.sort(key=lambda p: p.name == 'index.html')  # Switch homepage last.
            for path in files:
                dest = posixpath.join(remote, path.relative_to(root).as_posix())
                directory = posixpath.dirname(dest)
                if directory != remote:
                    raise ValueError('Unexpected nested deployment file')
                temporary = dest + '.deploying'
                sftp.put(str(path), temporary)
                sftp.posix_rename(temporary, dest)
print('Website uploaded with homepage switched last; unmanaged server files retained.')
