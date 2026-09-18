"""Exercise real signed installation/rollback with an isolated service-controller stand-in.
No operating-system service is stopped, started or installed by these tests.
"""
import importlib.util
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('update_service',ROOT/'scripts/update-service.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

class ServiceUpdateTests(unittest.TestCase):
    def exercise(self,healthy):
        with tempfile.TemporaryDirectory(prefix='service-update-test-',dir=ROOT/'.runtime') as temporary:
            folder=Path(temporary).resolve()
            name='remvora-agent.exe' if sys.platform == 'win32' else 'remvora-agent'
            target=folder/name;shutil.copy2(ROOT/'target/debug'/name,target)
            before=target.read_bytes()
            artifact=folder/'release';artifact.write_bytes(before+b'\nSIGNED_INSTALL_TEST_NOT_EXECUTED\n')
            key=folder/'signing-key.pem'
            subprocess.run(['openssl','genpkey','-algorithm','EC','-pkeyopt','ec_paramgen_curve:P-256','-out',str(key)],check=True,capture_output=True);key.chmod(0o600)
            release=folder/'signed'
            subprocess.run([sys.executable,str(ROOT/'scripts/sign-release.py'),'--artifact',str(artifact),'--key',str(key),'--version','99.0.0','--os',{'darwin':'macos','linux':'linux','win32':'windows'}[sys.platform],'--arch','aarch64' if __import__('platform').machine().lower() in ['arm64','aarch64'] else 'x86_64','--output',str(release)],check=True,capture_output=True)
            state=folder/'controller'
            commands={action:[sys.executable,'-c','from pathlib import Path; import sys; Path(sys.argv[1]).write_text(sys.argv[2])',str(state),action] for action in ['start','stop']}
            commands['check']=[sys.executable,'-c',f'raise SystemExit({0 if healthy else 1})']
            argv=['update-service','--manager','systemd','--server','https://localhost/','--installed',str(target),'--manifest',str(release/'manifest.json'),'--signature',str(release/'manifest.sig'),'--artifact',str(artifact),'--trusted-key',str(release/'signer-public.der')]
            with patch.object(sys,'argv',argv),patch.object(module,'service_commands',return_value=commands),patch.object(module.time,'sleep',return_value=None):
                if healthy:module.main()
                else:
                    with self.assertRaises(RuntimeError):module.main()
            self.assertEqual(target.read_bytes(),artifact.read_bytes() if healthy else before)
            self.assertEqual(state.read_text(),'start')
            if healthy:self.assertEqual(next(folder.glob('.remvora-*.previous')).read_bytes(),before)
    def test_success_retains_previous(self):self.exercise(True)
    def test_failed_service_start_restores_previous(self):self.exercise(False)

if __name__=='__main__':unittest.main()
