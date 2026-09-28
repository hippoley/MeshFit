# Versioned Delivery Plan

MeshFit develops by evidence gates rather than feature count.

## v0.1 — Structural Placement

**Question:** What plans are structurally possible?

Deliverables:

- HardwareIR
- FabricIR
- ModelIR
- RuntimeIR
- WorkloadIR
- PlanIR
- hard feasibility pruning
- single-host candidates
- conservative two-node TP admission
- Pareto frontier
- explicit rejection/exclusion reasons
- executable YAML scenario

Not included:

- tok/s prediction
- TTFT prediction
- automatic hardware discovery
- runtime launch
- adaptive scheduling

Exit gate:

> One heterogeneous scenario produces explainable feasible, rejected, Pareto, and excluded results.

---

## v0.2 — Prediction + Evidence

**Question:** Among feasible plans, what performance should we expect?

Add:

- EvidenceIR
- benchmark provenance
- prediction intervals
- confidence
- memory calibration
- latency / throughput predictors
- cost model
- estimate vs observed reports

Principle:

> No naked performance number without provenance and uncertainty.

Exit gate:

> At least one physical machine has predicted vs observed memory, TTFT, and decode throughput.

---

## v0.3 — Plan Compiler

**Question:** Can MeshFit turn a decision into something executable?

Add compilers for a narrow set first:

- llama.cpp
- vLLM

Then:

- SGLang
- MLX
- exo / llama.cpp RPC

Output:

- launch commands
- environment
- device mapping
- TP/PP settings
- model/quant selection
- assumptions

Exit gate:

> At least one generated plan launches successfully without manual topology translation.

---

## v0.4 — Active Probe Planner

**Question:** What evidence is missing before we trust the plan?

Add probes:

- GPU/RAM discovery
- PCIe/NVLink topology
- iperf3
- RTT/jitter
- runtime version/capability
- model artifact inspection
- controlled microbenchmarks

MeshFit should ask for the **minimum probe that reduces decision uncertainty**.

Exit gate:

> A previously ambiguous placement decision changes based on measured probe evidence.

---

## v0.5 — Adaptive Placement

**Question:** Should placement change when reality changes?

Add:

- live telemetry
- failure events
- load changes
- cost changes
- model/workload changes
- re-placement proposals

This version remains a control/intelligence layer; execution stays delegated to runtimes and orchestrators.

Exit gate:

> A measured runtime change triggers a justified placement change with before/after evidence.

---

## Later research tracks

Only after the earlier gates are real:

- KV placement
- prefill/decode disaggregation
- expert placement for MoE
- multi-objective policy learning
- community calibration corpus
- counterfactual placement evidence
- energy-aware placement
- reliability / failure-domain optimization
