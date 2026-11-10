"""Run the actual desktop WebView controls against an owned local fixture."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading

root = Path(__file__).resolve().parents[2]
artifacts = root / ".nsh" / "webview-preflight"
logs = artifacts / "logs"
proof = artifacts / "proof"
output = artifacts / "output"
smoke = artifacts / "crate"
for directory in (logs, proof, output, smoke / "src"):
    directory.mkdir(parents=True, exist_ok=True)

source_paths = subprocess.check_output(
    ["git", "ls-files", "--", "crates", "Cargo.toml", "Cargo.lock", "tests/compat"],
    cwd=root, text=True,
).splitlines()
hashes = {
    path: hashlib.sha256((root / path).read_bytes()).hexdigest()
    for path in source_paths
    if (root / path).is_file()
}
metadata = {
    "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
    "platform": sys.platform,
    "rustc": subprocess.check_output(["rustc", "-Vv"], cwd=root, text=True),
    "source_sha256": hashes,
}
(proof / "source-hashes.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")

platform = {"linux": "linux", "darwin": "macos", "win32": "windows"}[sys.platform]
features = ', features = ["webview"]' if platform == "linux" else ""
manifest = f'''[package]
name = "webview-preflight"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
nsh-core = {{ path = {json.dumps(str(root / "crates/nsh-core"))} }}
nsh-platform-{platform} = {{ path = {json.dumps(str(root / ("crates/nsh-platform-" + platform)))}, default-features = false{features} }}
serde_json = "1"
image = {{ version = "0.25", default-features = false, features = ["png"] }}
'''
(smoke / "Cargo.toml").write_text(manifest, encoding="utf-8")
shutil.copyfile(root / "Cargo.lock", smoke / "Cargo.lock")
shutil.copyfile(root / ".github/ci/webview-preflight.rs", smoke / "src/main.rs")

fixture = subprocess.Popen(
    [sys.executable, "-u", str(root / ".github/ci/webview-fixture.py")],
    cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
)
try:
    line = fixture.stdout.readline().strip()
    if not line.startswith("FIXTURE_URL="):
        raise RuntimeError("fixture failed to start: " + line)
    url = line.split("=", 1)[1]

    def record_fixture():
        with (logs / "fixture.log").open("w", encoding="utf-8") as log:
            log.write(line + "\n")
            for entry in fixture.stdout:
                log.write(entry)
                log.flush()

    recorder = threading.Thread(target=record_fixture, daemon=True)
    recorder.start()
    env = os.environ.copy()
    env.update(SMOKE_URL=url + "/page", SMOKE_OUTPUT=str(output), RUST_BACKTRACE="1")
    command = ["cargo", "run", "--manifest-path", str(smoke / "Cargo.toml"), "--target-dir", str(root / "target")]
    unit_command = ["cargo", "test", "--locked", "-p", "nsh-platform-" + platform,
                    "--features", "dev,webview" if platform == "linux" else "dev", "--lib"]
    if platform == "linux":
        env.pop("WAYLAND_DISPLAY", None)
        env.update(GDK_BACKEND="x11", WEBKIT_DISABLE_DMABUF_RENDERER="1", GSK_RENDERER="gl",
                   XDG_DATA_HOME=str(output / "profile"), XDG_CACHE_HOME=str(output / "cache"))
        command = ["dbus-run-session", "--", "xvfb-run", "-a", "-s", "-screen 0 1280x1024x24", *command]
        unit_command = ["dbus-run-session", "--", "xvfb-run", "-a", "-s", "-screen 0 1280x1024x24", *unit_command]
    elif platform == "windows":
        env.setdefault("LIBCLANG_PATH", r"C:\Program Files\LLVM\bin")
        env.setdefault("RUST_MIN_STACK", "268435456")
    print("Native verification:", platform, metadata["commit"], flush=True)
    with (logs / "unit.log").open("w", encoding="utf-8") as log:
        unit = subprocess.run(unit_command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=2400)
    unit_text = (logs / "unit.log").read_text(encoding="utf-8", errors="replace")
    print(unit_text[-24000:], flush=True)
    with (logs / "native.log").open("w", encoding="utf-8") as log:
        completed = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=2400)
    text = (logs / "native.log").read_text(encoding="utf-8", errors="replace")
    print(text[-24000:], flush=True)
    for image in output.glob("*.png"):
        shutil.copyfile(image, proof / image.name)
    checks = [entry for entry in text.splitlines() if entry.startswith("PASS ")]
    status = {"commit": metadata["commit"], "platform": platform, "exit_code": completed.returncode,
              "unit_exit_code": unit.returncode,
              "checks": checks, "success_marker": "WEBVIEW_SMOKE_PASS" in text}
    (proof / "result.json").write_text(json.dumps(status, indent=2), encoding="utf-8")
    if unit.returncode or completed.returncode or not status["success_marker"]:
        raise SystemExit(unit.returncode or completed.returncode or 1)
finally:
    fixture.terminate()
    try:
        fixture.wait(timeout=10)
    except subprocess.TimeoutExpired:
        fixture.kill()
        fixture.wait()
