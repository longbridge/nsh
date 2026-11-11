"""Cargo runner for the focused cookie tests in the Android emulator."""
import subprocess
import sys
import uuid

remote = '/data/local/tmp/nsh-cookie-' + uuid.uuid4().hex
subprocess.run(['adb', 'push', sys.argv[1], remote], check=True)
try:
    subprocess.run(['adb', 'shell', 'chmod', '700', remote], check=True)
    result = subprocess.run(['adb', 'shell', remote, *sys.argv[2:]])
finally:
    subprocess.run(['adb', 'shell', 'rm', '-f', remote], check=True)
sys.exit(result.returncode)
