# Heterogeneous GPU discovery

MeshFit discovery is evidence-first: a vendor device is added only when the management tool exposes a usable memory capacity. Missing vendor tools are not treated as errors.

## NVIDIA

Existing discovery uses:

```bash
nvidia-smi --query-gpu=index,name,memory.total,memory.free,driver_version --format=csv,noheader,nounits
nvidia-smi topo -m
```

This produces CUDA device identity, total/free memory, driver version, and local PCIe/NVLink relation types without inventing link bandwidth or latency.

## AMD

Baseline ROCm discovery uses the current AMD SMI CLI:

```bash
amd-smi static --asic --driver --vram --json
amd-smi monitor --vram-usage --json
```

MeshFit extracts:

- market/device name when available;
- driver version when available;
- total VRAM;
- used VRAM and therefore free VRAM when the monitor output provides it.

AMD SMI reports memory with explicit units. MeshFit converts those values into MiB before constructing `DeviceIdentity`.

This baseline does not yet infer XGMI/PCIe topology, compute score, or performance.

## Intel

Baseline Intel XPU discovery uses:

```bash
xpu-smi discovery -j
```

MeshFit extracts:

- `device_name`;
- `driver_version`;
- `memory_physical_size_byte`;
- `memory_free_size_byte`.

Those byte counts are converted to MiB and represented with the `xpu` accelerator backend.

This is intentionally a baseline inventory path. Intel topology and runtime-specific capability validation remain separate work.

## Failure semantics

- A vendor CLI that is not installed is silently skipped.
- A CLI that exists but fails returns a discovery warning.
- Invalid JSON returns a discovery warning.
- A device without usable total memory is not admitted as an accelerator.
- Unknown performance is represented as `relative_compute: 0`; MeshFit does not invent a score.

The planner therefore receives observed capacity facts while unsupported vendor-specific topology remains explicitly absent.
