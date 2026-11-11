"""Temporary iOS simulator validation of the actual Rust Cookie entrypoints."""
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import threading

root = Path(__file__).resolve().parents[1]
area = root / '.nsh/mobile-cookie-ios'
proof = area / 'proof'
crate = area / 'crate'
(crate / 'src').mkdir(parents=True, exist_ok=True)
proof.mkdir(parents=True, exist_ok=True)

def run(command, name, env=None):
    with (proof / (name + '.log')).open('w') as log:
        result = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=1800)
    print((proof / (name + '.log')).read_text()[-18000:], flush=True)
    result.check_returncode()

(crate / 'Cargo.toml').write_text(f'''[package]
name = "mobile-cookie-ios"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
nsh-core = {{ path = {json.dumps(str(root / 'crates/nsh-core'))} }}
nsh-platform-ios = {{ path = {json.dumps(str(root / 'crates/nsh-platform-ios'))}, default-features = false }}
env_logger = "0.11"
''')
shutil.copyfile(root / 'Cargo.lock', crate / 'Cargo.lock')
shutil.copyfile(root / '.nsh/mobile-cookie-ios.rs', crate / 'src/main.rs')
(proof / 'commit.txt').write_text(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True))
target = 'aarch64-apple-ios-sim' if os.uname().machine == 'arm64' else 'x86_64-apple-ios'
run(['cargo', 'clippy', '--locked', '-p', 'nsh-platform-ios', '--no-default-features', '--target', target, '--', '-D', 'warnings'], 'clippy')
run(['cargo', 'build', '--manifest-path', str(crate / 'Cargo.toml'), '--target', target, '--message-format=json'], 'build')
artifacts = [json.loads(line) for line in (proof / 'build.log').read_text().splitlines() if line.startswith('{')]
executable = next(item['executable'] for item in artifacts if item.get('reason') == 'compiler-artifact' and item.get('target', {}).get('name') == 'mobile-cookie-ios' and item.get('executable'))
app = area / 'MobileCookie.app'
app.mkdir(exist_ok=True)
shutil.copy2(executable, app / 'MobileCookie')
with (app / 'Info.plist').open('wb') as file:
    plistlib.dump({'CFBundleIdentifier': 'com.nsh.mobile-cookie', 'CFBundleName': 'MobileCookie', 'CFBundleExecutable': 'MobileCookie', 'CFBundlePackageType': 'APPL', 'CFBundleVersion': '1', 'CFBundleShortVersionString': '1.0', 'MinimumOSVersion': '15.0', 'UIDeviceFamily': [1, 2], 'UILaunchScreen': {}, 'UIApplicationSceneManifest': {'UIApplicationSupportsMultipleScenes': False, 'UISceneConfigurations': {}}, 'NSAppTransportSecurity': {'NSAllowsArbitraryLoads': True, 'NSAllowsArbitraryLoadsInWebContent': True}}, file)
run(['codesign', '--force', '--sign', '-', str(app)], 'codesign')
runtimes = json.loads(subprocess.check_output(['xcrun', 'simctl', 'list', 'runtimes', '-j']))['runtimes']
runtime = next(r['identifier'] for r in reversed(runtimes) if r.get('isAvailable') and 'iOS' in r['name'])
devices = json.loads(subprocess.check_output(['xcrun', 'simctl', 'list', 'devices', 'available', '-j']))['devices']
source = next(d['udid'] for d in devices[runtime] if d['name'].startswith('iPhone') and d['state'] == 'Shutdown')
udid = subprocess.check_output(['xcrun', 'simctl', 'clone', source, 'NSH Cookie Verification'], text=True).strip()
fixture = None
try:
    run(['xcrun', 'simctl', 'boot', udid], 'boot')
    run(['xcrun', 'simctl', 'bootstatus', udid, '-b'], 'bootstatus')
    run(['xcrun', 'simctl', 'install', udid, str(app)], 'install')
    fixture = subprocess.Popen([sys.executable, '-u', str(root / '.nsh/cookie-proof-inputs/webview-review-fixture.py')], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    first = fixture.stdout.readline().strip()
    if not first.startswith('FIXTURE_URL='):
        raise RuntimeError(first)
    def record():
        with (proof / 'fixture.log').open('w') as log:
            log.write(first + '\n')
            for line in fixture.stdout:
                log.write(line)
                log.flush()
    threading.Thread(target=record, daemon=True).start()
    container = subprocess.check_output(['xcrun', 'simctl', 'get_app_container', udid, 'com.nsh.mobile-cookie', 'data'], text=True).strip()
    env = os.environ.copy()
    env.update(SIMCTL_CHILD_SMOKE_URL=first.split('=', 1)[1], SIMCTL_CHILD_SMOKE_OUTPUT=container + '/Documents/proof', SIMCTL_CHILD_RUST_BACKTRACE='1')
    run(['xcrun', 'simctl', 'launch', '--console', '--terminate-running-process', udid, 'com.nsh.mobile-cookie'], 'native', env)
    native = (proof / 'native.log').read_text()
    if 'WEBVIEW_SMOKE_PASS' not in native:
        raise RuntimeError('iOS Cookie smoke did not report success')
    shutil.copytree(Path(container) / 'Documents/proof', proof / 'app', dirs_exist_ok=True)
    (proof / 'result.json').write_text(json.dumps({'success': True, 'runtime': runtime, 'target': target, 'checks': [line for line in native.splitlines() if line.startswith('PASS ')]}, indent=2))
finally:
    if fixture:
        fixture.terminate()
        fixture.wait(timeout=10)
    subprocess.run(['xcrun', 'simctl', 'shutdown', udid], check=False)
    subprocess.run(['xcrun', 'simctl', 'delete', udid], check=False)
