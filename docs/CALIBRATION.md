# Memory calibration

MeshFit calibration compares an analytical plan estimate with observed benchmark evidence.

The first calibration dimension is total peak VRAM.

## Contract

For a supported placement:

```text
predicted = PlanIR.required_memory_gb
observed  = max(wave.peak_vram_gb) for each BenchmarkBundle
```

MeshFit reports:

- signed error in GB,
- absolute error in GB,
- absolute percentage error,
- observed/predicted ratio,
- underprediction / overprediction / exact direction,
- repeated-run mean and max observed peak VRAM,
- repeated-run sample standard deviation,
- mean observed/predicted correction ratio,
- conservative max-observed/predicted correction ratio,
- worst underprediction in GB.

Positive signed error means the analytical model predicted more memory than observed. Negative signed error is dangerous underprediction.

## Identity gate

Calibration refuses evidence when:

- the benchmark bundle fails validation,
- the bundle source plan ID does not equal the calibration plan ID,
- placement kind differs,
- peak VRAM was not observed,
- the plan has non-positive required memory.

The first version supports only `single_host` and `tensor_parallel`. CPU offload is intentionally excluded because total model memory and observed accelerator VRAM are not directly comparable.

## Reality boundary

The files under `examples/` are synthetic fixtures used only to validate the calibration contract. They are not MeshFit performance evidence.

Real correction factors must come from repeated Benchmark 001 bundles collected on actual hardware.
