"""Opt-in Linux LayerFS trial. Run from the repository root; see docs/dev/layerfs.md."""

import argparse
import io
import json
import pathlib
import shutil
import subprocess
import tarfile
import uuid

SOURCE = pathlib.Path('.nsh/layerfs/source')
REVISION = '2aaf2552ce21154928b9f9c83582e41a637f5910'
STATE = pathlib.Path('.nsh/layerfs')


def run(*args, capture=False):
    return subprocess.run(args, check=True, text=True, capture_output=capture).stdout


def prepare():
    STATE.mkdir(parents=True, exist_ok=True)
    if not SOURCE.exists():
        run('git', 'clone', '--filter=blob:none', '--no-checkout', '--depth', '1', '--branch', 'v1.0.0',
            'https://github.com/Ephemeral-AI-Lab/layerfs.git', str(SOURCE))
        run('git', '-C', str(SOURCE), 'sparse-checkout', 'set', '--no-cone',
            '/.cargo/', '/core/Cargo.toml', '/core/Cargo.lock',
            '/core/crates/', '/core/vendor/', '/sdk/js/')
        run('git', '-C', str(SOURCE), 'checkout', '--detach', REVISION)
    revision = run('git', '-C', str(SOURCE), 'rev-parse', 'HEAD', capture=True).strip()
    if revision != REVISION:
        raise RuntimeError(f'LayerFS source must be {REVISION}, found {revision}')
    run('git', '-C', str(SOURCE), 'diff', '--exit-code', 'HEAD', '--', '.cargo', 'core', 'sdk/js')
    run('cargo', '+1.85.1', '--config', str(SOURCE / '.cargo/config.toml'),
        'build', '--manifest-path', str(SOURCE / 'core/Cargo.toml'),
        '--target-dir', 'target', '--locked', '--release',
        '-p', 'layerfs-node', '--lib', '-p', 'layerfs-daemon', '--bins')
    shutil.copyfile('target/release/liblayerfs_node.so', SOURCE / 'sdk/js/native/layerfs.node')
    run('docker', 'build', '--tag', 'nsh-layerfs:1.0.0', '.github/ci/layerfs')
    image = run('docker', 'image', 'inspect', '--format', '{{.Id}}',
                'nsh-layerfs:1.0.0', capture=True).strip()
    (STATE / 'deployment.json').write_text(json.dumps({'revision': revision, 'image': image}, indent=2))


def smoke():
    deployment = json.loads((STATE / 'deployment.json').read_text())
    name = 'nsh-' + uuid.uuid4().hex[:12]
    state = STATE / name
    source = state / 'input'
    source.mkdir(parents=True)
    # Use a named, committed input, excluding ignored caches and credentials.
    # Init itself intentionally captures everything in that input, including .git.
    archive = subprocess.check_output(['git', 'archive', 'HEAD'])
    with tarfile.open(fileobj=io.BytesIO(archive)) as tree:
        tree.extractall(source, filter='data')
    run('git', 'init', '--initial-branch=main', str(source))
    run('git', '-C', str(source), 'add', '.')
    run('git', '-C', str(source), '-c', 'user.name=NSH LayerFS trial',
        '-c', 'user.email=layerfs@example.invalid',
        '-c', 'maintenance.auto=false', '-c', 'gc.auto=0', 'commit', '-qm', 'Trial input')
    run('git', '-C', str(source), 'fsck', '--full')
    status = run('git', '-C', str(source), 'status', '--porcelain', capture=True)
    if status:
        raise RuntimeError(f'Trial input must be clean: {status}')
    state = state.resolve()
    volume = 'nsh-layerfs-' + name
    run('docker', 'volume', 'create', '--label', 'nsh.layerfs=trial', volume)
    settings = {**deployment, 'project': name, 'volume': volume,
                'gitCommit': run('git', 'rev-parse', 'HEAD', capture=True).strip()}
    (state / 'settings.json').write_text(json.dumps(settings, indent=2))
    # The Linux SDK opens the same plain local volume as the Sandbox. Host
    # networking lets it reach the daemon's loopback-only published port.
    run('docker', 'run', '--rm', '--network', 'host',
        '--mount', 'type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock',
        '--mount', f'type=volume,source={volume},target=/layerfs-store',
        '--mount', f'type=bind,source={state},target=/layerfs-state',
        '--mount', f'type=bind,source={source.resolve()},target=/layerfs-input,readonly',
        '--mount', f'type=bind,source={SOURCE.resolve() / "sdk/js"},target=/layerfs-sdk,readonly',
        '--mount', f'type=bind,source={pathlib.Path("target/release/layerfs-daemon").resolve()},target=/layerfs-daemon,readonly',
        '--mount', f'type=bind,source={pathlib.Path(".github/ci/layerfs/smoke.mjs").resolve()},target=/smoke.mjs,readonly',
        deployment['image'], 'node', '/smoke.mjs')
    print(f'Receipts: {state}/result.json', flush=True)
    print(f'Store retained in Docker volume: {volume}', flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['prepare', 'smoke'])
    args = parser.parse_args()
    {'prepare': prepare, 'smoke': smoke}[args.command]()
