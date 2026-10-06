# Versioned Delivery Plan

MeshFit develops by **Reality Gates**, not feature count.

The product goal is:

> discover a real heterogeneous compute estate → search feasible placements → compile an executable plan → benchmark reality → calibrate future placement decisions.

## v0.1 — Structural Placement

**Question:** What plans are structurally possible?

Status: **implemented and CI-verified**

Delivered:

- HardwareIR
- FabricIR
- ModelIR
- workload-aware KV cache profile (bytes/token or transformer shape)
- RuntimeIR
- WorkloadIR with explicit max active sequence residency
- PlanIR
- hard feasibility pruning
- single-host candidates
- conservative two-node TP admission
- per-device tensor-parallel shard gate; aggregate VRAM is not treated as sufficient capacity
- analytical cross-node TP communication gate with explicit model profile, measured fabric, workload ms/token budget, and structured plan estimate
- Pareto frontier
- communication-aware Pareto dominance with explicit unknown-evidence semantics
- explicit rejection/exclusion reasons
- executable YAML scenario
- strict CI: rustfmt, clippy -D warnings, workspace tests, release build, and CLI E2E verified after workload-aware placement on PR run #326

Exit gate:

> One heterogeneous scenario produces explainable feasible, rejected, Pareto, and excluded results.

---

## v0.2 — Evidence Identity

**Question:** Can benchmark evidence be trusted and matched to the exact execution that produced it?

Status: **in progress**

Delivered / current:

- EvidenceStore
- BenchmarkRecord
- provenance
- exact-match prediction
- structured Hardware / Model Artifact / Runtime / Topology identity
- stable execution fingerprint
- explicit Unavailable when evidence is missing

Remaining:

- benchmark artifact schema validation
- model artifact hash capture
- runtime/driver capture from real execution
- repeated-run statistics and objective-specific run stability gate (CV <= 20%)
- direct best-baseline improvement headline with provisional/publishable separation

Validation: PR CI run #463 passed fmt, clippy, workspace tests, release build, and the expanded Benchmark 001 CLI chain including candidate selection, execution-kit generation, provisional comparison, and publishability rejection.

Exit gate:

> Changing runtime, driver, model artifact, quantization, topology, context, or concurrency prevents accidental evidence reuse.

---

## v0.2.5 — Discovery & Fabric Snapshot

**Question:** Can MeshFit build its input from reality instead of hand-written YAML?

This milestone was moved earlier because automatic discovery is part of the core user story.

Add:

- CPU / RAM discovery
- NVIDIA GPU + free VRAM + driver
- Apple Silicon / unified memory
- initial AMD / Intel device discovery
- PCIe / NVLink topology
- runtime versions
- host identity
- link RTT
- optional bandwidth measurement
- generated InfrastructureIR + ExecutionIdentity snapshot

Not yet:

- uncertainty-driven active probing
- continuous telemetry

Exit gate:

> Two real machines can generate a reproducible MeshFit snapshot without manually typing their hardware specification.

---

## v0.3 — Plan Compiler + Benchmark Harness

**Question:** Can a MeshFit decision become a real execution and produce evidence?

Start narrow:

- llama.cpp compiler
- vLLM compiler

Output:

- launch command / config
- device mapping
- TP/PP settings
- model/quant selection
- assumptions
- evidence bundle path

Benchmark harness captures:

- TTFT
- TPOT
- prefill tok/s
- decode tok/s
- throughput
- peak VRAM / RAM
- runtime identity
- artifact identity
- topology identity

Exit gate:

> At least one generated plan launches successfully and writes a provenance-complete benchmark record without manual topology translation.

---

## v0.4 — Calibrated Prediction

**Question:** Among feasible plans, what performance should we expect?

Only now add stronger predictors because real evidence exists.

Add:

- exact empirical aggregation
- p50 / p95
- variance
- prediction intervals
- cost model
- memory calibration contract and CLI: required-memory vs observed peak-VRAM error/correction ratios (synthetic CI verified; real GPU repeated calibration pending)
- latency / throughput predictors
- estimate-vs-observed reports — VRAM dimension implemented; TTFT/decode dimensions pending
- carefully bounded cross-hardware transfer

Calibration validation: CI runs #877 and #900 verified the strict VRAM calibration contract, CLI path, underprediction direction, conservative observed/predicted ratio, and compatibility with the evolving Benchmark 001 execution stack. These validations use synthetic fixtures; real GPU calibration remains pending.

Principle:

> No naked performance number without provenance, uncertainty, and transfer distance.

Exit gate:

> At least two real placements have predicted vs observed memory, TTFT, and decode throughput with recorded error.

---

## v0.5 — Active Probe Planner

**Question:** What is the minimum additional measurement needed to choose between plans?

Add probes chosen by uncertainty:

- iperf3
- RTT / jitter
- GPU microbenchmark
- storage throughput
- runtime capability probe
- model artifact inspection

MeshFit should request the **minimum probe that can change the decision**.

Exit gate:

> A previously ambiguous placement choice changes because a targeted probe supplied missing evidence.

---

## v0.6 — Adaptive Placement

**Question:** Should placement change when reality changes?

Add:

- live telemetry
- failures
- workload shifts
- cloud cost changes
- runtime degradation
- re-placement proposals
- counterfactual before/after evidence

Execution remains delegated to runtimes/orchestrators.

Exit gate:

> A measured runtime change triggers a justified re-placement proposal with before/after evidence.

---

## Later research tracks

Only after the earlier Reality Gates are real:

- KV placement
- prefill/decode disaggregation
- expert placement for MoE
- multi-objective policy learning
- community calibration corpus
- energy-aware placement
- reliability/failure-domain optimization
- learned placement policies
