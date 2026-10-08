#!/bin/bash
set -euo pipefail

python3 - <<'PY'
from pathlib import Path
import hashlib
import yaml

root = Path("/app/campaign")
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

python3 /app/tools/verify_proof.py /app/campaign
