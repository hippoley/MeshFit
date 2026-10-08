#!/usr/bin/env python3
"""Convert MLCommons InfraGraph + paired sysinfo captures into MeshFit planning seeds.

This is deliberately an inventory bridge, not execution attestation.
It consumes the pairing contract used by mlperf-automations PR #1088:

  infragraph.yaml instance.description -> <stem>.json / <stem>.lstopo.xml

The output keeps unknown/runtime-sensitive fields untrusted:
- relative_compute = 0
- free_memory_gb = null
- hardware_identity.ram_mib = null
- local_fabric = []

Real Benchmark 001 execution must still re-discover each host and measure peer links.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import xml.etree.ElementTree as ET
from pathlib import Path

import yaml

CAPACITY_RE = re.compile(r"^\s*([0-9]+(?:\.[0-9]+)?)\s*([kmgt]?i?b?|bytes?)?\s*$", re.I)


def parse_capacity_gib(raw: object) -> float:
    text = str(raw or "").strip()
    match = CAPACITY_RE.match(text)
    if not match:
        raise ValueError(f"unsupported capacity value: {text!r}")
    value = float(match.group(1))
    unit = (match.group(2) or "b").lower()
    if unit in {"t", "tb", "tib"}:
        return value * 1024.0
    if unit in {"g", "gb", "gib"}:
        return value
    if unit in {"m", "mb", "mib"}:
        return value / 1024.0
    if unit in {"k", "kb", "kib"}:
        return value / (1024.0 * 1024.0)
    if unit in {"b", "byte", "bytes"}:
        return value / (1024.0 ** 3)
    raise ValueError(f"unsupported capacity unit: {unit!r}")


def lstopo_info(path: Path, key: str) -> str | None:
    root = ET.parse(path).getroot()
    for info in root.iter("info"):
        if info.get("name") == key:
            value = (info.get("value") or "").strip()
            if value:
                return value
    return None


def normalize_arch(raw: str) -> str:
    value = raw.strip().lower()
    aliases = {
        "amd64": "x86_64",
        "x86-64": "x86_64",
        "x86_64": "x86_64",
        "arm64": "aarch64",
        "aarch64": "aarch64",
    }
    return aliases.get(value, value)


def normalize_os(raw: str) -> str:
    value = raw.strip().lower()
    if any(token in value for token in ("linux", "ubuntu", "debian", "rhel", "centos", "rocky", "fedora")):
        return "linux"
    if "windows" in value:
        return "windows"
    if any(token in value for token in ("darwin", "macos", "mac os")):
        return "macos"
    return value.split()[0] if value else "unknown"


def backend_and_vendor(model: str, backend_hint: str) -> tuple[str, str]:
    m = model.lower()
    h = backend_hint.lower()
    if "cuda" in h or "nvidia" in m:
        return "cuda", "nvidia"
    if "rocm" in h or "amd" in m or re.search(r"\bmi\d", m):
        return "rocm", "amd"
    if "xpu" in h or "intel" in m:
        return "xpu", "intel"
    if "metal" in h or "apple" in m:
        return "metal", "apple"
    if "npu" in h:
        return "npu", "unknown"
    return "unknown", "unknown"


def clean_driver(raw: object) -> str | None:
    text = str(raw or "").strip()
    if not text:
        return None
    return re.sub(r"^driver\s+", "", text, flags=re.I)


def safe_name(raw: str) -> str:
    out = re.sub(r"[^A-Za-z0-9._-]+", "-", raw.strip()).strip("-")
    return out or "node"


def parse_hourly_cost_map(path: Path) -> dict[str, float]:
    raw = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(raw, dict) or not raw:
        raise ValueError("hourly cost map must be a non-empty mapping of instance.name -> USD/hour")

    costs: dict[str, float] = {}
    for key, raw_value in raw.items():
        node_id = str(key).strip()
        if not node_id:
            raise ValueError("hourly cost map contains an empty node id")
        try:
            value = float(raw_value)
        except (TypeError, ValueError) as exc:
            raise ValueError(f"{node_id}: invalid hourly cost {raw_value!r}") from exc
        if not math.isfinite(value) or value < 0.0:
            raise ValueError(f"{node_id}: hourly cost must be finite and non-negative")
        costs[node_id] = value
    return costs


def build_seed(
    instance: dict,
    sysinfo: dict,
    xml_path: Path,
    hourly_cost_usd: float,
) -> dict:
    node_id = str(instance.get("name") or "").strip()
    if not node_id:
        raise ValueError("InfraGraph instance has no name")

    architecture = lstopo_info(xml_path, "Architecture")
    if not architecture:
        raise ValueError(
            f"{node_id}: lstopo XML has no Architecture info; refusing to invent hardware identity"
        )

    os_name = lstopo_info(xml_path, "OSName") or str(sysinfo.get("operating_system") or "")
    if not os_name:
        raise ValueError(f"{node_id}: no operating-system fact available")

    cpu_model = str(sysinfo.get("host_processor_model_name") or "").strip() or None
    host_ram_gib = parse_capacity_gib(sysinfo.get("host_memory_capacity"))

    model = str(sysinfo.get("accelerator_model_name") or "").strip()
    count = int(sysinfo.get("accelerators_per_node") or 0)
    acc_mem_gib = parse_capacity_gib(sysinfo.get("accelerator_memory_capacity")) if count else 0.0
    backend, vendor = backend_and_vendor(model, str(sysinfo.get("inference_backend") or ""))
    driver = clean_driver(sysinfo.get("driver"))

    devices = []
    accelerators = []
    for idx in range(count):
        devices.append(
            {
                "vendor": vendor,
                "model": model,
                "backend": backend,
                "memory_mib": int(round(acc_mem_gib * 1024.0)),
                "driver_version": driver,
            }
        )
        accelerators.append(
            {
                "id": f"gpu{idx}",
                "backend": backend,
                "memory_gb": acc_mem_gib,
                "free_memory_gb": None,
                "relative_compute": 0.0,
            }
        )

    stem = str(instance.get("description") or "").strip()
    warnings = [
        "Imported from MLCommons InfraGraph/sysinfo as a planning seed; not execution attestation.",
        "hardware_identity.ram_mib is intentionally unset because MLCommons host_memory_capacity is presentation-rounded.",
        "accelerator free memory and relative_compute are intentionally unset/zero; measure them on the execution host.",
        "local_fabric is intentionally empty; MLCommons InfraGraph PR #1088 does not infer inter-node fabric edges.",
        "hourly_cost_usd is an explicit operator declaration; MLCommons sysinfo does not supply this cost fact.",
        "Run meshfit discover on the target host and meshfit probe for required peers before Benchmark 001 readiness.",
    ]
    if stem:
        warnings.append(f"MLCommons source stem: {stem}")

    return {
        "hardware_identity": {
            "architecture": normalize_arch(architecture),
            "operating_system": normalize_os(os_name),
            "cpu_model": cpu_model,
            "ram_mib": None,
            "devices": devices,
        },
        "node": {
            "id": node_id,
            "site": "mlcommons-import",
            "ram_gb": host_ram_gib,
            "accelerators": accelerators,
            "hourly_cost_usd": hourly_cost_usd,
        },
        "local_fabric": [],
        "warnings": warnings,
    }


def convert(
    infragraph_yaml: Path,
    sysinfo_dir: Path,
    out_dir: Path,
    hourly_costs: dict[str, float] | None = None,
    assume_zero_hourly_cost: bool = False,
) -> list[Path]:
    graph = yaml.safe_load(infragraph_yaml.read_text(encoding="utf-8")) or {}
    instances = graph.get("instances") or []
    if not instances:
        raise ValueError("InfraGraph YAML contains no instances")

    instance_names = {
        str(instance.get("name") or "").strip()
        for instance in instances
    }
    if "" in instance_names:
        raise ValueError("InfraGraph instance has no name")

    if assume_zero_hourly_cost:
        if hourly_costs is not None:
            raise ValueError("hourly cost map and zero-cost assumption are mutually exclusive")
        hourly_costs = {node_id: 0.0 for node_id in instance_names}
    elif hourly_costs is None:
        raise ValueError(
            "MLCommons sysinfo does not define node hourly cost; provide "
            "--hourly-cost-map or explicitly pass --assume-zero-hourly-cost"
        )

    missing_costs = sorted(instance_names - set(hourly_costs))
    extra_costs = sorted(set(hourly_costs) - instance_names)
    if missing_costs:
        raise ValueError(
            "hourly cost map is missing InfraGraph instance(s): "
            + ", ".join(missing_costs)
        )
    if extra_costs:
        raise ValueError(
            "hourly cost map contains unknown InfraGraph instance(s): "
            + ", ".join(extra_costs)
        )
    for node_id, value in hourly_costs.items():
        if not math.isfinite(value) or value < 0.0:
            raise ValueError(f"{node_id}: hourly cost must be finite and non-negative")

    if out_dir.exists():
        raise ValueError(f"output directory already exists: {out_dir}")

    stage = out_dir.with_name(out_dir.name + ".meshfit-tmp")
    if stage.exists():
        raise ValueError(f"staging directory already exists: {stage}")

    try:
        stage.mkdir(parents=True)
        discovery_files: list[str] = []
        seen_node_ids: set[str] = set()

        for instance in instances:
            stem = str(instance.get("description") or "").strip()
            if not stem:
                raise ValueError(f"instance {instance.get('name')!r} has no description/source stem")
            sysinfo_path = sysinfo_dir / f"{stem}.json"
            xml_path = sysinfo_dir / f"{stem}.lstopo.xml"
            if not sysinfo_path.is_file() or not xml_path.is_file():
                raise ValueError(
                    f"{instance.get('name')}: missing paired {stem}.json or {stem}.lstopo.xml"
                )
            sysinfo = json.loads(sysinfo_path.read_text(encoding="utf-8"))
            seed = build_seed(
                instance,
                sysinfo,
                xml_path,
                hourly_costs[str(instance.get("name") or "").strip()],
            )
            node_id = seed["node"]["id"]
            if node_id in seen_node_ids:
                raise ValueError(f"duplicate InfraGraph instance/node id: {node_id}")
            seen_node_ids.add(node_id)

            filename = f"discovery-{safe_name(node_id)}.yaml"
            if filename in discovery_files:
                raise ValueError(f"node ids collide after filename sanitization: {node_id}")
            path = stage / filename
            path.write_text(yaml.safe_dump(seed, sort_keys=False), encoding="utf-8")
            discovery_files.append(filename)

        manifest = {
            "discovery_files": discovery_files,
            "probes": [],
        }
        manifest_path = stage / "snapshot-manifest.yaml"
        manifest_path.write_text(yaml.safe_dump(manifest, sort_keys=False), encoding="utf-8")
        stage.rename(out_dir)
    except Exception:
        if stage.exists():
            import shutil
            shutil.rmtree(stage, ignore_errors=True)
        raise

    return [out_dir / name for name in discovery_files] + [out_dir / "snapshot-manifest.yaml"]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--infragraph-yaml", required=True, type=Path)
    parser.add_argument("--sysinfo-dir", required=True, type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    cost_group = parser.add_mutually_exclusive_group(required=True)
    cost_group.add_argument(
        "--hourly-cost-map",
        type=Path,
        help="YAML/JSON mapping of InfraGraph instance.name to declared marginal USD/hour",
    )
    cost_group.add_argument(
        "--assume-zero-hourly-cost",
        action="store_true",
        help="Explicitly declare zero marginal compute cost for every imported node",
    )
    args = parser.parse_args()

    try:
        hourly_costs = (
            parse_hourly_cost_map(args.hourly_cost_map)
            if args.hourly_cost_map is not None
            else None
        )
        files = convert(
            args.infragraph_yaml,
            args.sysinfo_dir,
            args.out_dir,
            hourly_costs=hourly_costs,
            assume_zero_hourly_cost=args.assume_zero_hourly_cost,
        )
    except Exception as exc:
        parser.error(str(exc))
    for path in files:
        print(path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
