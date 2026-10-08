#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
from pathlib import Path

import yaml

ROOT = Path("/app/campaign")
EXPECTED = {
    "frozen/plan.yaml": "e11743937f69441bf480610274e1deaee7ccd8f7cb5ad9ffce6ad67bcf6840ed",
    "benchmark-proof.yaml": "c82f7d1dfa55a98fbcfc4393238dea25e619fed6ffec064e43a1ce76d5d45fbb",
    "transfers/run-01.yaml": "ff20e705e5a0f7a4c1c2697ab627fbabd35df65ca249c01cc4cb0437f311abc3",
}
EXPECTED_BUNDLE_SHA = "2e0299f5505ad593fe7fc1549e7980d234dbed1b6349d2c3660b6aeb88c7a39b"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(name: str, passed: bool, scored: bool = True) -> dict:
    return {"name": name, "passed": bool(passed), "scored": scored}


checks = []

for rel, expected in EXPECTED.items():
    path = ROOT / rel
    checks.append(check(f"immutable control unchanged: {rel}", path.is_file() and digest(path) == expected))

control_lines = (ROOT / "CONTROL.sha256").read_text().splitlines()
control_ok = all(
    len(line.split()) == 2 and digest(ROOT / line.split()[1]) == line.split()[0]
    for line in control_lines
)
checks.append(check("CONTROL.sha256 validates frozen controls", control_ok))

proof = yaml.safe_load((ROOT / "benchmark-proof.yaml").read_text())
bundle_path = ROOT / proof["bundle"]["path"]

checks.append(check("raw bundle is a regular file", bundle_path.is_file() and not bundle_path.is_symlink(), scored=False))
checks.append(check("raw bundle matches proof hash", bundle_path.is_file() and digest(bundle_path) == EXPECTED_BUNDLE_SHA))

bundle = yaml.safe_load(bundle_path.read_text()) if bundle_path.is_file() else {}
checks.append(check("source plan binding preserved", bundle.get("source_plan_id") == "meshfit-plan-001"))
checks.append(check("source commit binding preserved", bundle.get("source_commit") == "0123456789abcdef0123456789abcdef01234567"))
checks.append(check("executing binary identity preserved", bundle.get("binary_sha256") == "a" * 64))
checks.append(check("capture identity preserved", bundle.get("captured_at") == "unix_ms:1700000000000"))
checks.append(check("measurements match frozen claim", bundle.get("measurements") == proof.get("claim")))

scored = [item for item in checks if item["scored"]]
reward = sum(1 for item in scored if item["passed"]) / len(scored)
if any(not item["passed"] for item in checks if not item["scored"]):
    reward = 0.0

Path("/logs/verifier/reward.json").write_text(
    json.dumps({"reward": reward, "checks": checks}, indent=2) + "\n"
)
Path("/logs/verifier/reward.txt").write_text(f"{reward}\n")

if reward < 1.0:
    raise SystemExit(1)
