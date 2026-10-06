# Apple unified-memory discovery

MeshFit treats Apple silicon as a Metal accelerator with shared system memory, not as a discrete GPU with private VRAM.

## Detection

The Apple baseline activates only when both are true:

```text
operating_system == macos
architecture == aarch64
```

This avoids classifying Intel Macs as Apple unified-memory GPUs.

## Identity

MeshFit reads:

```bash
sysctl -n hw.memsize
sysctl -n machdep.cpu.brand_string
```

The accelerator is represented as:

- vendor: `apple`
- backend: `metal`
- model: `<Apple chip> GPU`
- total accelerator memory identity: total physical unified memory

The total value is an identity/capacity fact. It is not treated as current free GPU memory.

## Current usable shared memory

MeshFit reads `vm_stat` and conservatively counts:

```text
Pages free
+ Pages inactive
+ Pages speculative
```

using the page size reported by `vm_stat`.

The resulting value is capped by physical unified memory and stored as the accelerator's current free-memory observation.

If `vm_stat` is unavailable or those page classes cannot be parsed, MeshFit does not admit the Apple GPU as a plannable accelerator. It emits a warning instead of falling back to total physical memory.

## Reality boundary

This baseline intentionally does not infer:

- Metal GPU core count;
- memory bandwidth;
- compute throughput;
- maximum Metal working-set behavior;
- Neural Engine capacity;
- runtime-specific memory overhead.

Apple silicon shares memory between CPU and GPU, so current system availability is only a conservative structural capacity observation. Real Metal/MLX/llama.cpp benchmarking remains the authority for usable memory and performance calibration.
