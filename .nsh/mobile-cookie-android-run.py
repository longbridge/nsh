"""Temporary Android verification through the real packaged Rust root."""
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import threading
import time

root = Path(__file__).resolve().parents[1]
area = root / '.nsh/mobile-cookie-android'
app = area / 'app'
proof = area / 'proof'
(app / 'src').mkdir(parents=True, exist_ok=True)
proof.mkdir(parents=True, exist_ok=True)
(app / 'Cargo.toml').write_text(f'''[package]
name = "mobile-cookie-android"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
nsh-core = {{ path = {json.dumps(str(root / 'crates/nsh-core'))} }}
log = "0.4"
''')
(app / 'nsh.json').write_text(json.dumps({'name': 'Android cookies', 'id': 'com.nsh.cookie.android', 'rust': '.', 'root': 'App', 'android': {'usesCleartextTraffic': True}}))
shutil.copyfile(root / '.nsh/mobile-cookie-android.rs', app / 'src/lib.rs')
shutil.copyfile(root / 'Cargo.lock', app / 'Cargo.lock')
(proof / 'commit.txt').write_text(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True))

def run(command, name, env=None):
    with (proof / (name + '.log')).open('w') as log:
        result = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=2400)
    print((proof / (name + '.log')).read_text()[-20000:], flush=True)
    result.check_returncode()

run(['cargo', 'build', '--locked', '-p', 'nsh-cli'], 'cli-build')
run(['cargo', 'xtask', 'android', '--abi', 'x86_64'], 'android-build')
subprocess.run(['adb', 'logcat', '-c'], check=True)
run([str(root / 'target/debug/nsh'), 'package', str(app), '--target', 'android', '--abi', 'x86_64', '--run', '-v'], 'package')
logs = subprocess.Popen(['adb', 'logcat', '-v', 'brief', '-s', 'nsh', 'AndroidRuntime'], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
lines = queue.Queue()
def collect():
    for line in logs.stdout:
        lines.put(line)
threading.Thread(target=collect, daemon=True).start()
success = False
try:
    deadline = time.monotonic() + 120
    with (proof / 'native.log').open('w') as log:
        while time.monotonic() < deadline:
            try:
                line = lines.get(timeout=max(0.1, deadline - time.monotonic()))
            except queue.Empty:
                break
            log.write(line)
            log.flush()
            print(line, end='', flush=True)
            if 'WEBVIEW_SMOKE_PASS' in line:
                success = True
                break
            if 'FAIL:' in line or 'FATAL EXCEPTION:' in line:
                break
    with (proof / 'screenshot.png').open('wb') as image:
        subprocess.run(['adb', 'exec-out', 'screencap', '-p'], stdout=image, check=True)
    (proof / 'device.txt').write_text(subprocess.check_output(['adb', 'shell', 'getprop', 'ro.build.fingerprint'], text=True))
    (proof / 'webview.txt').write_text(subprocess.check_output(['adb', 'shell', 'dumpsys', 'webviewupdate'], text=True))
    (proof / 'result.json').write_text(json.dumps({'success': success}))
    if not success:
        raise RuntimeError('Android Cookie scenario did not report success')
finally:
    logs.terminate()
    logs.wait(timeout=10)
    subprocess.run(['adb', 'shell', 'am', 'force-stop', 'com.nsh.cookie.android'], check=False)
