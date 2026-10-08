#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import sys
from pathlib import Path

import yaml


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def safe_path(root: Path, relative: str) -> Path:
    rel = Path(relative)
    if rel.is_absolute() or ".." in rel.parts:
        raise ValueError(f"unsafe proof path: {relative}")
    resolved = (root / rel).resolve(strict=True)
    root_resolved = root.resolve(strict=True)
    if root_resolved not in (resolved, *resolved.parents):
        raise ValueError(f"proof path escapes audit root: {relative}")
    if resolved.is_symlink():
        raise ValueError(f"proof path must not be a symlink: {relative}")
    return resolved


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "/app/campaign")
    proof = yaml.safe_load((root / "benchmark-proof.yaml").read_text())

    # This task fixture is intentionally a task-local repair contract. It is
    # not the public MeshFit BenchmarkProofReceipt schema.
    if proof.get("schema") != "meshfit.infrabench-placement-proof/v1":
        raise ValueError("unsupported proof schema")

    plan = safe_path(root, proof["plan"]["path"])
    bundle = safe_path(root, proof["bundle"]["path"])

    if sha256(plan) != proof["plan"]["sha256"]:
        raise ValueError("frozen plan hash mismatch")
    if sha256(bundle) != proof["bundle"]["sha256"]:
        raise ValueError("raw benchmark bundle hash mismatch")

    raw = yaml.safe_load(bundle.read_text())
    expected = proof["bundle"]
    for key in (
        "benchmark_id",
        "source_plan_id",
        "source_commit",
        "binary_sha256",
        "captured_at",
    ):
        expected_value = (
            proof["benchmark_id"] if key == "benchmark_id" else expected[key]
        )
        if raw.get(key) != expected_value:
            raise ValueError(f"bundle {key} does not match proof")

    measurements = raw.get("measurements", {})
    claim = proof.get("claim", {})
    if measurements != claim:
        raise ValueError("raw measurements do not match frozen claim")

    print("VERIFIED: decision -> evidence -> claim chain is intact")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"VERIFY FAIL: {exc}", file=sys.stderr)
        raise SystemExit(2)
