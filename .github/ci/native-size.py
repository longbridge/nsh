#!/usr/bin/env python3
"""Record the native binaries' sizes and fail when one exceeds its budget.

Usage: native-size.py DIST MATRIX_NAME [BUDGETS]

Reports every file of 1 MB or more under DIST (the `cargo xtask dist`
output) in the job summary. A budgeted file that is missing or larger than
its budget (.github/ci/native-size.json) fails the step.
"""
import json
import os
import sys
from pathlib import Path

dist = Path(sys.argv[1])
name = sys.argv[2]
budgets_path = Path(sys.argv[3]) if len(sys.argv) > 3 else Path(__file__).with_name("native-size.json")
budgets = json.loads(budgets_path.read_text()).get(name, {})

sizes = {
    path.relative_to(dist).as_posix(): path.stat().st_size
    for path in dist.rglob("*")
    if path.is_file() and path.stat().st_size >= 1_000_000
}

rows, failures = [], []
for file in sorted(set(sizes) | set(budgets)):
    size, budget = sizes.get(file), budgets.get(file)
    if size is None:
        failures.append(f"{file}: missing (budgeted {budget:,})")
        rows.append(f"| {file} | missing | {budget:,} | ❌ |")
        continue
    status = "" if budget is None else ("❌" if size > budget else "✅")
    if budget is not None and size > budget:
        failures.append(f"{file}: {size:,} bytes exceeds budget {budget:,}")
    rows.append(f"| {file} | {size:,} | {'' if budget is None else f'{budget:,}'} | {status} |")

report = "\n".join([f"### Native sizes ({name})", "", "| File | Bytes | Budget | |", "|---|---:|---:|---|", *rows, ""])
print(report)
summary = os.environ.get("GITHUB_STEP_SUMMARY")
if summary:
    with open(summary, "a", encoding="utf-8") as out:
        out.write(report + "\n")
if not budgets:
    print(f"::warning::no native size budget for {name}")
for failure in failures:
    print(f"::error::{failure}")
sys.exit(1 if failures else 0)
