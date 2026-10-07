#!/usr/bin/env python3
"""Emit a small, auditable GPU compute proxy for MeshFit Reality Campaign 001.

This is not an inference benchmark. It measures one fixed dense GEMM shape so
real discovered accelerators can receive comparable, non-zero relative_compute
scores before Benchmark 001 baseline selection.
"""

import argparse
import json
import platform
import socket
import statistics
import time
from datetime import datetime, timezone

try:
    import torch
except ImportError as exc:
    raise SystemExit("PyTorch is required: install a CUDA-enabled torch build") from exc


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--device", type=int, default=0)
    parser.add_argument("--dtype", choices=("bf16", "fp16"), default="bf16")
    parser.add_argument("--size", type=int, default=8192)
    parser.add_argument("--warmup", type=int, default=5)
    parser.add_argument("--repeats", type=int, default=20)
    return parser.parse_args()


def selected_dtype(name: str):
    if name == "bf16":
        if not torch.cuda.is_bf16_supported():
            raise SystemExit(
                "BF16 is not supported on this CUDA device; do not mix precisions "
                "inside one Reality Campaign. Re-run every node with one common dtype."
            )
        return torch.bfloat16
    return torch.float16


def main() -> None:
    args = parse_args()
    if not torch.cuda.is_available():
        raise SystemExit("CUDA is required for this compute proxy")
    if args.size <= 0 or args.warmup < 0 or args.repeats < 3:
        raise SystemExit("--size must be >0, --warmup >=0, and --repeats >=3")

    torch.cuda.set_device(args.device)
    dtype = selected_dtype(args.dtype)
    device = torch.device(f"cuda:{args.device}")
    props = torch.cuda.get_device_properties(device)

    # Deterministic inputs are unnecessary for throughput, but a fixed seed makes
    # allocation/data generation reproducible across repeated operator runs.
    torch.manual_seed(1)
    a = torch.randn((args.size, args.size), device=device, dtype=dtype)
    b = torch.randn((args.size, args.size), device=device, dtype=dtype)

    for _ in range(args.warmup):
        torch.mm(a, b)
    torch.cuda.synchronize(device)

    samples_ms = []
    for _ in range(args.repeats):
        start = torch.cuda.Event(enable_timing=True)
        end = torch.cuda.Event(enable_timing=True)
        start.record()
        torch.mm(a, b)
        end.record()
        end.synchronize()
        samples_ms.append(float(start.elapsed_time(end)))

    median_ms = statistics.median(samples_ms)
    operations = 2.0 * (args.size ** 3)
    median_tflops = operations / (median_ms / 1000.0) / 1e12

    output = {
        "schema": "meshfit.compute-proxy/v1",
        "purpose": "baseline-relative-compute-only",
        "captured_at": datetime.now(timezone.utc).isoformat(),
        "host": socket.gethostname(),
        "platform": platform.platform(),
        "torch_version": torch.__version__,
        "cuda_version": torch.version.cuda,
        "device_index": args.device,
        "device_name": props.name,
        "compute_capability": f"{props.major}.{props.minor}",
        "total_memory_bytes": props.total_memory,
        "dtype": args.dtype,
        "matrix_size": args.size,
        "warmup": args.warmup,
        "repeats": args.repeats,
        "median_ms": median_ms,
        "median_tflops": median_tflops,
        "relative_compute": median_tflops,
        "samples_ms": samples_ms,
        "notes": [
            "Use the same dtype, matrix_size, warmup, and repeats on every compared GPU.",
            "This score is a baseline ordering proxy, not a MeshFit performance claim.",
            "Retain this JSON beside the real discovery/snapshot evidence.",
        ],
    }
    print(json.dumps(output, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
