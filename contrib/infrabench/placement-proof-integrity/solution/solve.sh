#!/bin/bash
set -euo pipefail

CAMPAIGN_ROOT="${CAMPAIGN_ROOT:-/app/campaign}"
VERIFY_PROOF="${VERIFY_PROOF:-/app/tools/verify_proof.py}"
export CAMPAIGN_ROOT

python3 - <<'PY'
from pathlib import Path
import hashlib
import yaml

root = Path(__import__("os").environ["CAMPAIGN_ROOT"])
transfer = yaml.safe_load((root / "transfers/run-01.yaml").read_text())
content = transfer["content"]
observed = hashlib.sha256(content.encode()).hexdigest()
if observed != transfer["sha256"]:
    raise SystemExit(
        f"trusted transfer package hash mismatch: declared={transfer['sha256']} observed={observed}"
    )

destination = root / transfer["path"]
destination.parent.mkdir(parents=True, exist_ok=True)
destination.write_text(content)
PY

python3 "$VERIFY_PROOF" "$CAMPAIGN_ROOT"