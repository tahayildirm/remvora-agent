#!/usr/bin/env python3
"""Build a local macOS PKG; optional identities sign locally. No notarization/upload is performed."""
import argparse
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--version',required=True)
p.add_argument('--application-identity')
p.add_argument('--installer-identity')
a=p.parse_args()
if not re.fullmatch(r'\d+\.\d+\.\d+',a.version):p.error('Version must be major.minor.patch')
if bool(a.application_identity)!=bool(a.installer_identity):p.error('Supply both signing identities, or neither')
root=Path(__file__).resolve().parent.parent
output=root/'.artifacts';output.mkdir(exist_ok=True)
package=output/f'remvora-agent-{a.version}-macos.pkg'
if package.exists():p.error('Refusing to overwrite an existing package')
with tempfile.TemporaryDirectory(prefix='pkg-',dir=output) as temporary:
    stage=Path(temporary);binary=stage/'remvora-agent'
    shutil.copy2(root/'target/release/remvora-agent',binary)
    if a.application_identity:
        subprocess.run(['/usr/bin/codesign','--force','--options','runtime','--sign',a.application_identity,str(binary)],check=True)
        subprocess.run(['/usr/bin/codesign','--verify','--strict',str(binary)],check=True)
    shutil.copy2(root/'LICENSE',stage/'LICENSE')
    command=['/usr/bin/pkgbuild','--root',str(stage),'--identifier','com.remvora.agent','--version',a.version,'--install-location','/usr/local/lib/remvora']
    if a.installer_identity:command+=['--sign',a.installer_identity]
    subprocess.run(command+[str(package)],check=True)
print('Local PKG created:',package)
