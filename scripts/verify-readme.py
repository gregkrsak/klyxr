#!/usr/bin/env python3
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parents[1]
readme = (root / "README.md").read_text(encoding="utf-8")

refs = set(re.findall(r'(?:src=|!\[[^\]]*\]\()["\']?([^"\' )>]+)', readme))
local = [r for r in refs if not re.match(r'^[a-z]+://', r) and not r.startswith('#')]

missing = []
for ref in local:
    ref = ref.split('#', 1)[0]
    if not ref:
        continue
    path = (root / ref).resolve()
    if not path.exists():
        missing.append(ref)

policy_errors = []

for line_number, line in enumerate(readme.splitlines(), start=1):
    identifiers = re.findall(r'\b(?:BED|BD|KED|KD)-\d+\b', line)
    if identifiers:
        policy_errors.append(
            f"line {line_number}: internal decision identifier(s) "
            f"{', '.join(sorted(set(identifiers)))}"
        )

    if re.search(r'\bthis branch\b', line, flags=re.IGNORECASE):
        policy_errors.append(
            f"line {line_number}: branch-lifecycle prose belongs in an issue, "
            "pull request, or implementation ledger"
        )

    if re.search(
        r'\bawaits\b.{0,80}\b(?:review|acceptance)\b',
        line,
        flags=re.IGNORECASE,
    ):
        policy_errors.append(
            f"line {line_number}: pending-review prose must not be published "
            "as current project documentation"
        )

public_footer = """<p align="center">
  <strong>Systems programming you can reason about.</strong>
</p>

<p align="center">
  <code>bowfin build</code> ·
  <code>bowfin test</code> ·
  <code>bowfin verify</code> ·
  <code>bowfin audit</code>
</p>

<p align="center">
  <sub>Own what moves. Constrain what exists. Prove what matters. Make every escape hatch visible.</sub>
</p>

<!-- README-END -->"""

if not readme.rstrip().endswith(public_footer):
    policy_errors.append(
        "the public footer and README-END marker must remain the final README section"
    )

if missing:
    print("Missing local README assets:")
    for ref in sorted(set(missing)):
        print(" -", ref)

if policy_errors:
    print("Root README public-documentation policy violations:")
    for error in policy_errors:
        print(" -", error)
    print(
        "Move decision history to docs/ai/LANGUAGE_DECISIONS.md, exact "
        "implementation scope to compiler/README.md, and milestone evidence "
        "to docs/engineering/."
    )

if missing or policy_errors:
    sys.exit(1)

print(
    f"README local assets OK ({len(local)} references checked); "
    "public-documentation boundary OK"
)
