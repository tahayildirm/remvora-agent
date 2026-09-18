#!/usr/bin/env python3
"""Explicit operator-driven signed update with service restart and failed-start rollback.
Run as the existing service owner/administrator; this tool never elevates privileges.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


def service_commands(kind):
    if kind == 'systemd':
        return {action: ['/usr/bin/systemctl', action, 'remvora-agent.service'] for action in ['stop','start']} | {'check':['/usr/bin/systemctl','is-active','--quiet','remvora-agent.service']}
    if kind == 'launchd':
        label=f'gui/{os.getuid()}/com.remvora.agent'
        # KeepAlive must be disabled before stopping so it cannot race executable replacement.
        return {'stop':['/bin/launchctl','bootout',label], 'start':['/bin/launchctl','bootstrap',f'gui/{os.getuid()}',str(Path.home()/'Library/LaunchAgents/com.remvora.agent.plist')], 'check':['/bin/launchctl','print',label]}
    powershell=str(Path(os.environ.get('SystemRoot',r'C:\Windows'))/'System32/WindowsPowerShell/v1.0/powershell.exe')
    return {'stop':[powershell,'-NoProfile','-NonInteractive','-Command',"Stop-Service -Name RemvoraAgent -ErrorAction Stop"], 'start':[powershell,'-NoProfile','-NonInteractive','-Command',"Start-Service -Name RemvoraAgent -ErrorAction Stop"], 'check':[powershell,'-NoProfile','-NonInteractive','-Command',"if ((Get-Service RemvoraAgent).Status -ne 'Running') { exit 1 }"]}


def healthy(commands, kind):
    for _ in range(10):
        result=subprocess.run(commands['check'],capture_output=True,text=True,timeout=15)
        if result.returncode == 0 and (kind != 'launchd' or 'state = running' in result.stdout):
            return True
        time.sleep(1)
    return False


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manager',choices=['systemd','launchd','windows'],required=True)
    for name in ['installed','manifest','signature','artifact','trusted-key']:
        parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--server',required=True)
    args=parser.parse_args()
    installed=args.installed.absolute()
    if installed.is_symlink() or installed.parent.resolve()!=installed.parent:
        parser.error('Installed executable and its parent must not be symlinks')
    commands=service_commands(args.manager)
    with tempfile.TemporaryDirectory(prefix='.remvora-update-',dir=installed.parent) as temporary:
        helper=Path(temporary)/installed.name
        shutil.copy2(installed,helper)
        common=[str(helper),'--server',args.server,'--state',str(Path(temporary)/'state')]
        release=['--manifest',str(args.manifest),'--signature',str(args.signature),'--artifact',str(args.artifact),'--trusted-key',str(args.trusted_key)]
        # Validate before service interruption. The installer repeats validation on the destination copy.
        subprocess.run(common+['stage-update']+release,check=True,timeout=120)
        subprocess.run(commands['stop'],check=True,timeout=30)
        backup=None
        try:
            result=subprocess.run(common+['install-update']+release+['--target',str(installed)],check=True,capture_output=True,text=True,timeout=120)
            report=json.loads(result.stdout.strip().splitlines()[-1]);backup=Path(report['backup'])
            subprocess.run(commands['start'],check=True,timeout=30)
            if not healthy(commands,args.manager):
                raise RuntimeError('Updated service did not become healthy')
        except BaseException:
            if backup is not None and backup.exists():
                subprocess.run(commands['stop'],check=False,timeout=30,capture_output=True)
                os.replace(backup,installed)
            subprocess.run(commands['start'],check=False,timeout=30,capture_output=True)
            raise
        print('Signed update installed and service restarted. Previous binary:',backup)


if __name__=='__main__':
    main()
