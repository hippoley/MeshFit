#!/usr/bin/env python3
"""Fail-closed Lambda ODC capacity/cost gate for MeshFit Reality Campaign 001.

This tool consumes a previously captured Lambda `GET /api/v1/instance-types`
response. It never calls the provider API and never handles API keys.

It selects exactly one provider instance type for each required campaign role,
requires the expected CPU architecture, computes the live same-region capacity
intersection, derives the concurrent hourly burn from the authenticated
response, and calculates the maximum GPU wall-clock time under the campaign
budget ceiling.

The output is deterministic JSON suitable for retaining as campaign evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


DEFAULT_ROLES = (
    ("node-a", "A6000", 2),
    ("node-b", "H100 PCIe", 1),
    ("node-c", "B200", 1),
)


class GateError(RuntimeError):
    pass


def _load(path: Path) -> tuple[dict[str, Any], str]:
    raw = path.read_bytes()
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise GateError(f"invalid JSON: {exc}") from exc
    if not isinstance(payload, dict) or not isinstance(payload.get("data"), dict):
        raise GateError("expected top-level object with object field 'data'")
    return payload, hashlib.sha256(raw).hexdigest()


def _regions(item: dict[str, Any]) -> list[str]:
    regions = item.get("regions_with_capacity_available")
    if not isinstance(regions, list):
        raise GateError("regions_with_capacity_available must be a list")
    names: list[str] = []
    for region in regions:
        if not isinstance(region, dict) or not isinstance(region.get("name"), str):
            raise GateError("region entry missing string name")
        names.append(region["name"])
    return sorted(set(names))


def _instance_type(item: dict[str, Any]) -> dict[str, Any]:
    value = item.get("instance_type")
    if not isinstance(value, dict):
        raise GateError("instance_type must be an object")
    required = (
        "name",
        "description",
        "gpu_description",
        "price_cents_per_hour",
        "specs",
        "architecture",
    )
    missing = [key for key in required if key not in value]
    if missing:
        raise GateError(f"instance_type missing fields: {', '.join(missing)}")
    if not isinstance(value["specs"], dict) or not isinstance(
        value["specs"].get("gpus"), int
    ):
        raise GateError("instance_type.specs.gpus must be an integer")
    if not isinstance(value["price_cents_per_hour"], int):
        raise GateError("price_cents_per_hour must be an integer")
    return value


def _select_role(
    data: dict[str, Any],
    *,
    role: str,
    pattern: str,
    gpus: int,
    architecture: str,
) -> dict[str, Any]:
    rx = re.compile(pattern, re.IGNORECASE)
    matches: list[tuple[str, dict[str, Any], list[str]]] = []

    for provider_key, item in data.items():
        if not isinstance(item, dict):
            continue
        itype = _instance_type(item)
        haystack = " ".join(
            str(itype.get(field, ""))
            for field in ("name", "description", "gpu_description")
        )
        if (
            rx.search(haystack)
            and itype["specs"]["gpus"] == gpus
            and itype["architecture"] == architecture
        ):
            matches.append((provider_key, itype, _regions(item)))

    if not matches:
        raise GateError(
            f"{role}: no instance type matched /{pattern}/ with "
            f"{gpus} GPU(s) and architecture={architecture}"
        )
    if len(matches) > 1:
        details = ", ".join(key for key, _, _ in matches)
        raise GateError(
            f"{role}: ambiguous instance type match for /{pattern}/: {details}; "
            "use a narrower --*-match pattern"
        )

    provider_key, itype, regions = matches[0]
    if not regions:
        raise GateError(f"{role}: matched {provider_key} but live region capacity is empty")

    return {
        "role": role,
        "provider_type_key": provider_key,
        "name": itype["name"],
        "description": itype["description"],
        "gpu_description": itype["gpu_description"],
        "gpus": itype["specs"]["gpus"],
        "architecture": itype["architecture"],
        "price_cents_per_hour": itype["price_cents_per_hour"],
        "regions_with_capacity_available": regions,
    }


def build_receipt(args: argparse.Namespace) -> dict[str, Any]:
    payload, source_sha256 = _load(args.instance_types_json)
    data = payload["data"]

    requested = (
        ("node-a", args.node_a_match, args.node_a_gpus),
        ("node-b", args.node_b_match, args.node_b_gpus),
        ("node-c", args.node_c_match, args.node_c_gpus),
    )

    selected = [
        _select_role(
            data,
            role=role,
            pattern=pattern,
            gpus=gpus,
            architecture=args.architecture,
        )
        for role, pattern, gpus in requested
    ]

    common = set(selected[0]["regions_with_capacity_available"])
    for role in selected[1:]:
        common.intersection_update(role["regions_with_capacity_available"])
    common_regions = sorted(common)
    if not common_regions:
        raise GateError("no live same-region capacity intersection across all campaign roles")

    burn_cents = sum(item["price_cents_per_hour"] for item in selected)
    if burn_cents <= 0:
        raise GateError("computed concurrent burn must be positive")

    budget_cents = round(args.budget_usd * 100)
    if budget_cents <= 0:
        raise GateError("budget must be positive")

    max_wall_clock_hours = budget_cents / burn_cents

    return {
        "schema": "meshfit.reality.lambda_capacity_gate.v1",
        "source": {
            "path": str(args.instance_types_json),
            "sha256": source_sha256,
            "provider_endpoint": "GET /api/v1/instance-types",
        },
        "requirements": {
            "architecture": args.architecture,
            "budget_usd": round(args.budget_usd, 2),
            "roles": [
                {"role": role, "match": pattern, "gpus": gpus}
                for role, pattern, gpus in requested
            ],
        },
        "selected_instance_types": selected,
        "common_regions": common_regions,
        "concurrent_burn_usd_per_hour": burn_cents / 100.0,
        "max_gpu_wall_clock_hours_under_budget": max_wall_clock_hours,
        "ready_to_launch_shape": True,
        "note": (
            "Capacity is first-come and can change after this receipt. "
            "Launch must still fail closed on provider capacity or private-path errors."
        ),
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Validate captured Lambda instance-types JSON for the frozen "
            "Reality Campaign 001 hardware shape."
        )
    )
    parser.add_argument("instance_types_json", type=Path)
    parser.add_argument("--architecture", default="x86_64")
    parser.add_argument("--budget-usd", type=float, default=50.0)

    for role, default_match, default_gpus in DEFAULT_ROLES:
        flag = role.replace("-", "_")
        parser.add_argument(
            f"--{role}-match",
            dest=f"{flag}_match",
            default=default_match,
            help=f"case-insensitive regex for {role} (default: {default_match!r})",
        )
        parser.add_argument(
            f"--{role}-gpus",
            dest=f"{flag}_gpus",
            type=int,
            default=default_gpus,
            help=f"required GPU count for {role} (default: {default_gpus})",
        )

    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        receipt = build_receipt(args)
    except (OSError, GateError, re.error) as exc:
        print(f"lambda capacity gate: FAIL: {exc}", file=sys.stderr)
        return 2

    json.dump(receipt, sys.stdout, indent=2, sort_keys=True)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
