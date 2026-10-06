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


## Benchmark 001 kit integration

For a materialized Benchmark 001 experiment, calibrate one candidate directly from the frozen experiment inputs and collected result slots:

```bash
meshfit benchmark-calibrate-memory benchmark-001 meshfit
```

A partial diagnostic is allowed when at least one valid bundle exists. The report makes incompleteness explicit:

```text
complete
expected_runs
calibrated_runs
missing_bundles
```

Require every planned repeated run before producing a calibration report with:

```bash
meshfit benchmark-calibrate-memory benchmark-001 meshfit --require-complete
```

Kit-level calibration is stricter than the low-level `calibrate-memory` command. Every included bundle must match the frozen experiment on:

- source plan ID and placement,
- candidate runtime,
- target model ID,
- materialized model artifact identity,
- target context length,
- benchmark concurrency,
- frozen benchmark-host hardware identity,
- and the exact execution fingerprint shared by all repeated calibration runs.

An existing but corrupt, wrong-plan, wrong-workload, wrong-model, wrong-hardware, or mixed-execution bundle is a hard failure. It is never silently treated as a missing run.

This prevents correction factors from mixing observations across experiment generations.
