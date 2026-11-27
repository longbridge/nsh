#!/usr/bin/env python3
"""Reruns the runner's link with only rust-lld (#1644): no gcc, no clang, no
-dev packages. lld-only.py <collect2.txt> <rt dir> <out> [editor] [plot]

collect2.txt is the collect2 line `cc -v` printed for the runner's link.
<rt dir> holds the files nsh would ship: glibc's Scrt1.o, crti.o, crtn.o
and libc_nonshared.a. System libraries are linked by their runtime names
(libgtk-4.so.1, found on any machine that can run the app), never through
the libgtk-4.so symlinks of -dev packages. LINKER: the lld to run."""
import ctypes.util, os, shlex, subprocess, sys

cmd_file, rt, out, *parts = sys.argv[1:]
args = shlex.split(open(cmd_file).read().strip())[1:]
SYS = ["/usr/lib", "/usr/lib64", "/lib", "/lib64", "/usr/lib/x86_64-linux-gnu", "/lib/x86_64-linux-gnu"]
ALL = ["editor", "plot"]


def runtime(name):
    """The runtime file of `-l<name>`: the one its SONAME names."""
    for d in SYS:
        path = os.path.join(d, f"lib{name}.so")
        if os.path.exists(path) and os.path.getsize(path) > 1024 or os.path.islink(path):
            so = subprocess.run(["objdump", "-p", os.path.realpath(path)], capture_output=True, text=True).stdout
            soname = next(l.split()[1] for l in so.splitlines() if "SONAME" in l)
            return os.path.join(os.path.dirname(os.path.realpath(path)), soname)
    found = ctypes.util.find_library(name)  # no -dev symlink: the loader's cache
    if not found:
        sys.exit(f"no runtime library for -l{name}")
    for d in SYS:
        if os.path.exists(os.path.join(d, found)):
            return os.path.join(d, found)
    sys.exit(f"no runtime library for -l{name}")


kept, skip = [], False
for a in args:
    if skip:
        skip = False
        if a != "-o":
            continue
    if a.startswith("-fuse-ld") or a in ("-plugin", "-o") or a.startswith("-plugin-opt") or a.startswith("-L/usr") or a.startswith("-L/lib"):
        skip = a in ("-plugin", "-o")
        continue
    base = os.path.basename(a)
    if base in ("crtbeginS.o", "crtendS.o"):
        continue  # gcc's: Rust needs neither
    if base in ("Scrt1.o", "crti.o", "crtn.o"):
        kept.append(os.path.join(rt, base))
        continue
    if any(base == f"libnsh_windows_{p}.rlib" or base.endswith(f"libnsh_windows_{p}.a") for p in ALL if p not in parts):
        continue
    if a.startswith("-l"):
        name = a[2:]
        if name == "gcc":
            continue  # compiler_builtins has what Rust needs
        if name == "c":
            kept += [runtime("c"), os.path.join(rt, "libc_nonshared.a")]
            continue
        kept.append(runtime(name))
        continue
    kept.append(a)
for p in ALL:
    if p not in parts:
        kept.append(f"--defsym=nsh_part_{p}=nsh_part_none")
kept += ["-o", out]
lld = os.environ.get("LINKER", "rust-lld")
cmd = [lld, "-flavor", "gnu", *kept] if lld.endswith("rust-lld") else [lld, *kept]
sys.exit(subprocess.run(cmd, env={"PATH": "/nonexistent"}).returncode)
