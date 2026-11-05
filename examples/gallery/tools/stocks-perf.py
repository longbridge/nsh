#!/usr/bin/env python3
"""Compare two prebuilt Gallery bundles on macOS, in alternating ABBA order.

Build each revision with `nsh build examples/gallery/App.tsx --release -o DIR`,
then pass its DIR/bundle.nshc. Both use the same release host and a fresh data
directory per launch. Keep the window visible on a 120 Hz display. Market ticks
retain their normal random seed; repeated runs account for that variation.

The HUD's frame timings measure main-thread work, not presentation latency.
Its p95 values cover overlapping 120-frame windows, not independent frames.
CPU is percent of one core; memory is physical footprint in decimal MB.
"""

import argparse
import ctypes
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
from perf import LINE


class Usage(ctypes.Structure):
    _fields_ = [("uuid", ctypes.c_uint8 * 16)] + [
        (name, ctypes.c_uint64) for name in
        ("user", "system", "wakeups", "interrupts", "pageins", "wired", "rss", "footprint", "start", "exit")
    ]


class Timebase(ctypes.Structure):
    _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--before", required=True, type=Path)
    parser.add_argument("--after", required=True, type=Path)
    parser.add_argument("--host", type=Path, default=Path(__file__).resolve().parents[3] / "target/release/nsh-macos-host")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--display", default="1")
    parser.add_argument("--seconds", type=int, default=20)
    parser.add_argument("--warmup", type=int, default=5)
    parser.add_argument("--blocks", type=int, default=2, help="ABBA blocks (two runs of each bundle per block)")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("this benchmark uses the macOS host and libproc")
    if args.seconds < 5 or args.warmup < 3 or args.blocks < 1:
        parser.error("use at least 5 seconds, 3 seconds of warmup and one ABBA block")
    for path in (args.before, args.after, args.host):
        if not path.is_file():
            parser.error(f"missing file: {path}")
    # A new result directory also guarantees fresh Gallery preferences.
    args.out.mkdir(parents=True, exist_ok=False)
    lib = ctypes.CDLL("/usr/lib/libproc.dylib")
    lib.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    lib.proc_pid_rusage.restype = ctypes.c_int
    system = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
    system.mach_timebase_info.argtypes = [ctypes.POINTER(Timebase)]
    base = Timebase()
    if system.mach_timebase_info(ctypes.byref(base)) or not base.denom:
        raise RuntimeError("cannot read Mach timebase")

    def usage(pid):
        value = Usage()
        if lib.proc_pid_rusage(pid, 0, ctypes.byref(value)):
            raise RuntimeError(f"cannot sample process {pid}")
        # libproc returns Mach ticks on Apple Silicon, not nanoseconds.
        return (value.user + value.system) * base.numer / base.denom

    rows = []
    for index, label in enumerate(["before", "after", "after", "before"] * args.blocks):
        log = []
        readings = []
        command = [str(args.host.resolve()), "--bundle", str(getattr(args, label).resolve()),
                   "--window-size", "1200x800", "--display", args.display, "--fps", "--trace",
                   "--data-dir", str((args.out / f"data-{index}").resolve())]
        start = time.monotonic()
        proc = subprocess.Popen(command, env=dict(os.environ, NSH_GALLERY_DEMO="Stocks"),
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

        def read():
            for line in proc.stdout:
                log.append(line)
                if time.monotonic() - start >= args.warmup + 1 and (match := LINE.search(line)):
                    readings.append(match.groupdict())

        reader = threading.Thread(target=read, daemon=True)
        reader.start()
        try:
            time.sleep(args.warmup)
            cpu_start = usage(proc.pid)
            measured = time.monotonic()
            time.sleep(args.seconds)
            cpu = (usage(proc.pid) - cpu_start) / 1e9 / (time.monotonic() - measured) * 100
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
            reader.join(timeout=5)
            (args.out / f"{index}-{label}.log").write_text("".join(log))
        if any("ERROR" in line for line in log) or len(readings) < args.seconds - 3:
            raise RuntimeError(f"incomplete run {index}; inspect its log")
        median = lambda key: statistics.median(float(row[key]) for row in readings)
        # Reject an idle, hidden or wrong page instead of reporting it as a win.
        if median("fps") < 30 or median("commits") < 10:
            raise RuntimeError(f"run {index} did not sustain the live Stocks workload")
        row = {"variant": label, "cpu_percent_one_core": cpu, "fps": median("fps"),
               "refresh_hz": median("hz"), "footprint_MB": median("rss"),
               "over_budget_percent": median("over"), "commits_per_second": median("commits"),
               "frame_window_p50_ms": statistics.median(float(r["frame"].split("/")[1]) for r in readings),
               "frame_window_p95_ms": statistics.median(float(r["frame"].split("/")[2]) for r in readings),
               "readings": readings, "command": command}
        rows.append(row)
        (args.out / "results.json").write_text(json.dumps(rows, indent=2))
        print(json.dumps({k: v for k, v in row.items() if k not in ("readings", "command")}), flush=True)


if __name__ == "__main__":
    main()
