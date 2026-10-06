# Stable accelerator IDs

MeshFit placement plans refer to concrete accelerator IDs. Those IDs must remain stable when unrelated vendor discovery succeeds or fails.

Global discovery-order numbering is therefore invalid for heterogeneous hosts.

## ID contract

MeshFit uses vendor-local identifiers:

```text
NVIDIA CUDA   gpu<N>
AMD ROCm      amd<N>
Intel XPU     xpu<N>
Apple Metal   metal0
```

NVIDIA keeps `gpu<N>` because the compiler maps that suffix directly to the CUDA ordinal and NVIDIA topology discovery also emits `GPU<N>`.

AMD and Intel preserve the vendor tool's own device ordinal. Apple silicon exposes one integrated Metal GPU as `metal0`.

## Why this matters

On a host containing multiple accelerator vendors, these two discovery states must refer to the same AMD card:

```text
NVIDIA available:   gpu0, gpu1, amd0
NVIDIA unavailable: amd0
```

The AMD card must not silently change from `gpu2` to `gpu0`.

Stable IDs protect:

- `PlanIR.accelerators`;
- executable device binding;
- plan IDs;
- local fabric endpoints;
- benchmark candidate referents.

They are local execution identifiers, not global hardware fingerprints. Hardware identity remains based on observed device properties.
