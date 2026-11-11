"""Branch-only, real-engine validation of the reviewed WebView capture APIs."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading

root = Path(__file__).resolve().parents[1]
area = root / ".nsh/webview-review"
proof = area / "proof"
crate = area / "crate"
for path in [proof, crate / "src"]:
    path.mkdir(parents=True, exist_ok=True)
platform = {"linux": "linux", "darwin": "macos", "win32": "windows"}[sys.platform]
files = subprocess.check_output(["git", "ls-files", "--", "crates", "Cargo.toml", "Cargo.lock"], cwd=root, text=True).splitlines()
metadata = {"commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
            "platform": platform,
            "hashes": {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in files if (root / name).is_file()}}
(proof / "sources.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
features = ', features = ["webview"]' if platform == "linux" else ""
(crate / "Cargo.toml").write_text(f'''[package]
name = "webview-cookie-preflight"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
nsh-core = {{ path = {json.dumps(str(root / "crates/nsh-core"))} }}
nsh-platform-{platform} = {{ path = {json.dumps(str(root / ("crates/nsh-platform-" + platform)))}, default-features = false{features} }}
env_logger = "0.11"
''', encoding="utf-8")
shutil.copyfile(root / "Cargo.lock", crate / "Cargo.lock")
shutil.copyfile(root / ".nsh/cookie-proof-inputs/webview-review.rs", crate / "src/main.rs")
fixture = subprocess.Popen([sys.executable, "-u", str(root / ".nsh/cookie-proof-inputs/webview-review-fixture.py")], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
try:
    first = fixture.stdout.readline().strip()
    if not first.startswith("FIXTURE_URL="):
        raise RuntimeError("fixture startup: " + first)
    def record():
        with (proof / "fixture.log").open("w", encoding="utf-8") as log:
            log.write(first + "\n")
            for line in fixture.stdout:
                log.write(line)
                log.flush()
    threading.Thread(target=record, daemon=True).start()
    env = os.environ.copy()
    env.update(SMOKE_URL=first.split("=", 1)[1], SMOKE_OUTPUT=str(proof), RUST_BACKTRACE="1")
    unit = ["cargo", "test", "--locked", "-p", "nsh-platform-" + platform, "--no-default-features", "--lib", "native::"]
    if platform == "linux":
        unit[unit.index("--lib"):unit.index("--lib")] = ["--features", "webview"]
    native = ["cargo", "run", "--manifest-path", str(crate / "Cargo.toml"), "--target-dir", str(root / "target")]
    prefix = []
    if platform == "linux":
        env.pop("WAYLAND_DISPLAY", None)
        env.update(GDK_BACKEND="x11", GSK_RENDERER="gl", WEBKIT_DISABLE_DMABUF_RENDERER="1",
                   XDG_DATA_HOME=str(area / "data"), XDG_CACHE_HOME=str(area / "cache"))
        prefix = ["dbus-run-session", "--", "xvfb-run", "-a"]
    results = {}
    for name, command in [("unit", unit), ("native", native)]:
        with (proof / (name + ".log")).open("w", encoding="utf-8") as log:
            completed = subprocess.run(prefix + command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=2400)
        results[name] = completed.returncode
        text = (proof / (name + ".log")).read_text(encoding="utf-8", errors="replace")
        print(text[-18000:], flush=True)
        if name == "native":
            results["checks"] = [line for line in text.splitlines() if line.startswith("PASS ")]
            results["success_marker"] = "WEBVIEW_SMOKE_PASS" in text
    results["commit"] = metadata["commit"]
    (proof / "result.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
    if results["unit"] or results["native"] or not results["success_marker"]:
        raise SystemExit(1)
finally:
    fixture.terminate()
    try:
        fixture.wait(timeout=10)
    except subprocess.TimeoutExpired:
        fixture.kill()
        fixture.wait()
