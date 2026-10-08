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


def build_seed(instance: dict, sysinfo: dict, xml_path: Path) -> dict:
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
            "hourly_cost_usd": 0.0,
        },
        "local_fabric": [],
        "warnings": warnings,
    }


def convert(infragraph_yaml: Path, sysinfo_dir: Path, out_dir: Path) -> list[Path]:
    graph = yaml.safe_load(infragraph_yaml.read_text(encoding="utf-8")) or {}
    instances = graph.get("instances") or []
    if not instances:
        raise ValueError("InfraGraph YAML contains no instances")
    if out_dir.exists() and any(out_dir.iterdir()):
        raise ValueError(f"output directory is not empty: {out_dir}")
    out_dir.mkdir(parents=True, exist_ok=True)

    written: list[Path] = []
    discovery_files: list[str] = []
    for instance in instances:
        stem = str(instance.get("description") or "").strip()
        if not stem:
            raise ValueError(f"instance {instance.get('name')!r} has no description/source stem")
        sysinfo_path = sysinfo_dir / f"{stem}.json"
        xml_path = sysinfo_dir / f"{stem}.lstopo.xml"
        if not sysinfo_path.is_file() or not xml_path.is_file():
            raise ValueError(f"{instance.get('name')}: missing paired {stem}.json or {stem}.lstopo.xml")
        sysinfo = json.loads(sysinfo_path.read_text(encoding="utf-8"))
        seed = build_seed(instance, sysinfo, xml_path)
        filename = f"discovery-{safe_name(seed['node']['id'])}.yaml"
        path = out_dir / filename
        path.write_text(yaml.safe_dump(seed, sort_keys=False), encoding="utf-8")
        written.append(path)
        discovery_files.append(filename)

    manifest = {
        "discovery_files": discovery_files,
        "probes": [],
    }
    manifest_path = out_dir / "snapshot-manifest.yaml"
    manifest_path.write_text(yaml.safe_dump(manifest, sort_keys=False), encoding="utf-8")
    written.append(manifest_path)
    return written


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--infragraph-yaml", required=True, type=Path)
    parser.add_argument("--sysinfo-dir", required=True, type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    args = parser.parse_args()

    try:
        files = convert(args.infragraph_yaml, args.sysinfo_dir, args.out_dir)
    except Exception as exc:
        parser.error(str(exc))
    for path in files:
        print(path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
