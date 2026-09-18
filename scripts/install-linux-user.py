#!/usr/bin/env python3
"""Install a user-scoped Remvora graphical-session unit; never changes the kiosk or escalates."""
import argparse
import json
import os
from pathlib import Path
import subprocess
from urllib.parse import urlsplit

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--server',required=True)
p.add_argument('--binary',type=Path,required=True)
p.add_argument('--state',type=Path,required=True)
p.add_argument('--ca-certificate',type=Path)
p.add_argument('--file-root',type=Path)
for flag in ['terminal','desktop','clipboard','audio','reboot']:
    p.add_argument('--allow-'+flag,action='store_true')
p.add_argument('--enable',action='store_true',help='Enable and start only after successful enrollment and activation')
a=p.parse_args()
url=urlsplit(a.server)
if url.scheme!='https' or not url.hostname or url.username or url.password or any(c.isspace() for c in a.server):p.error('A credential-free HTTPS server URL is required')
if not a.binary.is_absolute() or not a.binary.is_file() or a.binary.is_symlink():p.error('Use an absolute regular executable path')
if not a.state.is_absolute() or a.state.is_symlink():p.error('Use an absolute private state directory')
if (a.allow_clipboard or a.allow_audio or a.file_root) and not a.allow_desktop:p.error('Clipboard, files and audio require desktop capability')
a.state.mkdir(parents=True,exist_ok=True,mode=0o700);a.state.chmod(0o700)
command=[str(a.binary.resolve()),'--server',a.server,'--state',str(a.state.resolve())]
for flag in ['terminal','desktop','clipboard','audio','reboot']:
    if getattr(a,'allow_'+flag):command+=['--allow-'+flag]
for flag,value in [('ca-certificate',a.ca_certificate),('file-root',a.file_root)]:
    if value:
        if not value.is_absolute() or value.is_symlink() or not value.exists():p.error('Local capability paths must exist and be absolute, without symlinks')
        command+=['--'+flag,str(value.resolve())]
command+=['run']
# systemd interprets percent specifiers even in quotes; double them in every external value.
quote=lambda value:json.dumps(value).replace('%','%%')
unit='[Unit]\nDescription=Remvora device agent (user session)\nAfter=graphical-session.target\n\n[Service]\nType=simple\nExecStart='+' '.join(map(quote,command))+'\nRestart=on-failure\nRestartSec=5\nNoNewPrivileges=true\nUMask=0077\n'
for key,default in [('DISPLAY',':0'),('WAYLAND_DISPLAY','wayland-0'),('XDG_SESSION_TYPE','wayland'),('XDG_RUNTIME_DIR',f'/run/user/{os.getuid()}'),('DBUS_SESSION_BUS_ADDRESS',f'unix:path=/run/user/{os.getuid()}/bus')]:
    value=os.environ.get(key,default)
    if '\n' in value or '\r' in value:p.error('Invalid session environment')
    unit+='Environment='+quote(key+'='+value)+'\n'
unit+='\n[Install]\nWantedBy=default.target\n'
directory=Path.home()/'.config/systemd/user';directory.mkdir(parents=True,exist_ok=True)
path=directory/'remvora-agent.service'
if path.exists():p.error('Existing Remvora unit found; review and update it manually to preserve local settings')
path.write_text(unit);path.chmod(0o600)
subprocess.run(['systemctl','--user','daemon-reload'],check=True)
if a.enable:subprocess.run(['systemctl','--user','enable','--now','remvora-agent.service'],check=True)
print('Installed:',path)
print('Enroll/activate under this same account before starting the service. Desktop requires the graphical session and its OS permissions.')
