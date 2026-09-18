#!/usr/bin/env python3
"""Create a P-256 signed offline release manifest. Keep the signing key outside the repository."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--artifact', type=Path, required=True)
parser.add_argument('--key', type=Path, required=True)
parser.add_argument('--version', required=True)
parser.add_argument('--os', required=True, choices=['windows','macos','linux'])
parser.add_argument('--arch', required=True, choices=['x86_64','aarch64'])
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=False)
with args.artifact.open('rb') as source:
    digest = hashlib.file_digest(source, 'sha256').hexdigest()
manifest = args.output/'manifest.json'
manifest.write_text(json.dumps(dict(version=args.version,os=args.os,arch=args.arch,sha256=digest),separators=(',',':')))
subprocess.run(['openssl','dgst','-sha256','-sign',str(args.key),'-out',str(args.output/'manifest.sig'),str(manifest)],check=True)
subprocess.run(['openssl','pkey','-in',str(args.key),'-pubout','-outform','DER','-out',str(args.output/'signer-public.der')],check=True)
print('Signed manifest created. Provision the trusted public key independently; never trust a key solely because it accompanies a download.')
