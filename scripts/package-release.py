#!/usr/bin/env python3
"""Create a local native archive. No upload, network operation or signing identity is involved."""
import argparse
import hashlib
from pathlib import Path
import platform
import tarfile
import zipfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--version', required=True)
args = parser.parse_args()
if not args.version or any(c not in '0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.+-' for c in args.version):
    parser.error('Invalid release version')
root = Path(__file__).resolve().parent.parent
system = {'Darwin':'macos','Windows':'windows','Linux':'linux'}[platform.system()]
arch = {'arm64':'aarch64','aarch64':'aarch64','x86_64':'x86_64','AMD64':'x86_64'}[platform.machine()]
binary = root/'target/release'/('remvora-agent.exe' if system == 'windows' else 'remvora-agent')
if not binary.is_file():
    parser.error('Build the native release executable first')
files = [(binary,binary.name)] + [(root/name,name) for name in ['README.md','LICENSE','SECURITY.md','docs/protocol-v1.md','docs/release-and-update.md','docs/remote-capabilities.md','scripts/update-service.py','scripts/install-linux-user.py']]
files += [(path,str(path.relative_to(root))) for path in (root/'deploy').glob('*.example')]
output = root/'.artifacts'; output.mkdir(exist_ok=True)
name = f'remvora-agent-{args.version}-{system}-{arch}'
archive = output/(name+('.zip' if system == 'windows' else '.tar.gz'))
if archive.exists():
    parser.error('Archive already exists; choose a new release version')
if system == 'windows':
    with zipfile.ZipFile(archive,'x',zipfile.ZIP_DEFLATED) as target:
        for path,relative in files: target.write(path,name+'/'+relative)
else:
    with tarfile.open(archive,'x:gz') as target:
        for path,relative in files: target.add(path,arcname=name+'/'+relative,recursive=False)
with archive.open('rb') as source: digest=hashlib.file_digest(source,'sha256').hexdigest()
(archive.parent/(archive.name+'.sha256')).write_text(digest+'  '+archive.name+'\n')
print('Unsigned native archive created:',archive)
