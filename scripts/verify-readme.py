#!/usr/bin/env python3
from pathlib import Path
import re, sys

root = Path(__file__).resolve().parents[1]
readme = (root / "README.md").read_text(encoding="utf-8")

refs = set(re.findall(r'(?:src=|!\[[^\]]*\]\()["\']?([^"\' )>]+)', readme))
local = [r for r in refs if not re.match(r'^[a-z]+://', r) and not r.startswith('#')]

missing = []
for ref in local:
    ref = ref.split('#', 1)[0]
    if not ref:
        continue
    p = (root / ref).resolve()
    if not p.exists():
        missing.append(ref)

if missing:
    print("Missing local README assets:")
    for m in sorted(set(missing)):
        print(" -", m)
    sys.exit(1)

print(f"README local assets OK ({len(local)} references checked)")
