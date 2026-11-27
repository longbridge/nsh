#!/usr/bin/env python3
"""Relinks the Linux runner from its saved objects with only the given parts,
as `nsh package` would (#1644): relink.py <linkcmd.txt> <out> [editor] [plot]

linkcmd.txt is the `cc` line `rustc --print link-args -C save-temps` printed.
A part left out of the link resolves its gate to `nsh_part_none` (--defsym).
LINKER=ld.lld reruns the line as lld directly, without cc (see --lld)."""
import shlex, subprocess, sys

cmd_file, out, *parts = sys.argv[1:]
line = open(cmd_file).read().strip()
args = shlex.split(line)
while args and "=" in args[0] and not args[0].startswith("-"):
    args.pop(0)  # the environment rustc prints first (LC_ALL, PATH, VSLANG)
cc, args = args[0], args[1:]
ALL = ["editor", "plot"]
kept, skip = [], False
for a in args:
    if skip:
        skip = False
        continue
    if a == "-o":
        skip = True
        continue
    name = a.rsplit("/", 1)[-1]
    if any(name == f"libnsh_windows_{p}.rlib" or name.startswith(f"libnsh_windows_{p}-") for p in ALL if p not in parts):
        continue
    kept.append(a)
for p in ALL:
    if p not in parts:
        kept.append(f"-Wl,--defsym=nsh_part_{p}=nsh_part_none")
kept += ["-o", out]
sys.exit(subprocess.run([cc, *kept]).returncode)
