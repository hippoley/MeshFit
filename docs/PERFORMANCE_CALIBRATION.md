# Performance Calibration Contract

MeshFit must measure prediction error before it attempts cross-hardware performance transfer.

This contract calibrates an explicit performance prediction against benchmark bundles for the **same execution identity and workload**.

## Identity boundary

Calibration requires every bundle to match:

- plan ID;
- placement;
- execution fingerprint;
- context length;
- concurrency;
- complete BenchmarkConfig.

Any drift is a hard failure.

## Supported metrics

The first contract covers:

- p95 TTFT;
- mean per-request decode tokens/s.

Predictions are explicit inputs. MeshFit does not invent a latency or throughput estimate merely because benchmark evidence exists.

For every metric, the report records:

- predicted value;
- observed value per benchmark bundle;
- signed error;
- absolute error;
- absolute percentage error;
- observed/predicted ratio;
- prediction bias;
- worst optimistic error;
- a conservative correction ratio.

For lower-is-better TTFT, the conservative correction ratio is the largest observed/predicted ratio.

For higher-is-better decode throughput, it is the smallest observed/predicted ratio.

## Reality boundary

This is an **exact-identity calibration contract**, not a cross-hardware model.

A real v0.4 performance predictor can plug into this contract later. Until then, the CLI accepts explicit predicted values and reports how wrong they were.

Synthetic CI verifies math, identity rejection, missing-usage rejection, and CLI serialization. Real heterogeneous GPU calibration remains a separate Reality Gate.
