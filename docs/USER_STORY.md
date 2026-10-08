# Core User Story

> **Scope note (2026-10-08):** this document contains the long-term product
> vision. It is not a claim that every placement mode or predictor named below is
> implemented. The executable near-term contract is narrower and is tracked in
> [USER_STORY_AUDIT.md](USER_STORY_AUDIT.md).

Given a real compute estate containing local servers, cloud instances, edge devices, heterogeneous CPU/GPU/NPU resources, memory, storage, and complex network topology, MeshFit should automatically model compute nodes and the links between them.

For a target model, MeshFit should reason about:

- parameter count and architecture
- dense vs MoE execution
- quantization
- context length
- KV cache
- Tensor / Pipeline / Expert Parallelism
- replication
- CPU/GPU offload
- runtime compatibility
- network bandwidth and latency
- cloud/on-prem cost
- workload concurrency
- latency SLOs

MeshFit should determine:

1. which models can run;
2. which deployment topologies are technically feasible;
3. which plans are actually useful rather than merely loadable;
4. which nodes should participate;
5. which nodes should explicitly be excluded;
6. how the model should be partitioned or replicated;
7. which runtime should execute the plan;
8. expected memory, network, latency, throughput and cost;
9. the dominant bottleneck;
10. how prediction differs from measured reality.

## Critical invariant

**Available compute is not the same as useful compute.**

A machine can have significant accelerator memory and still be a bad participant because of:

- insufficient link bandwidth
- excessive latency
- incompatible accelerator backend
- unsupported runtime
- poor collective communication characteristics
- asymmetric topology
- egress cost
- failure-domain risk
- worse end-to-end performance after joining the plan

Therefore, **“do not use this node” is a valid and important planner decision.**

## Reality requirement

A plan is not validated because it fits a formula.

Every serious performance claim should be benchmarkable against a real runtime and record:

- predicted metric
- observed metric
- prediction error
- hardware profile
- model profile
- topology profile
- runtime configuration

The long-term product is an **infrastructure-to-model deployment compiler**, not a hardware compatibility table.

## Current executable product contract

For the first defensible product proof, MeshFit currently commits to a narrower
story:

```text
observe real heterogeneous infrastructure
  -> generate feasible/rejected placement candidates
  -> freeze one placement decision
  -> compile supported placement modes
  -> execute on the intended physical host/device set
  -> retain provenance-bound benchmark evidence
  -> independently verify the resulting placement claim
```

Current solver/compiler coverage is strongest for:

- single-device / single-host placement;
- local tensor parallel placement;
- conservative cross-node tensor-parallel feasibility reasoning;
- single-host CPU offload;
- vLLM single-host / same-host TP compilation;
- llama.cpp single-host / CPU-offload compilation.

The following names exist in the IR, examples, evidence identity, or long-term
vision but are **not current end-to-end decision commitments**:

- automatic fit derivation from parameter count or a generic model-architecture
  parser — current memory feasibility is driven by explicit
  `weight_memory_gb` plus the KV-cache execution profile; `parameters_b` is
  descriptive today;
- generic NPU discovery/placement — do not infer support merely from the
  heterogeneous-backend vocabulary;
- MoE-aware placement policy from `is_moe` / `active_parameters_b`;
- automatic quantization selection (quantization is currently evidence/artifact identity);
- hard placement gating from `p95_latency_ms` or `budget_per_day_usd`;
- storage-aware placement;
- failure-domain-aware placement;
- Pipeline Parallel placement search/execution;
- Expert Parallel placement search/execution;
- Replica placement search/execution;
- general multi-node runtime orchestration;
- generic storage-aware placement;
- failure-domain optimization;
- a general cross-hardware latency/throughput predictor;
- continuous telemetry and automatic re-placement.

Do not infer implementation status from an enum value or runtime capability flag.
A placement mode is considered implemented only when it is enumerated by the
solver, compiled by a supported runtime adapter when required, exercised by the
CLI, and covered by the relevant evidence contract.

## First-product-proof interpretation of the ten decisions

The ten long-term questions above currently resolve as follows:

1. **which models can run** — structural fit exists for an explicit target
   model/profile whose weight-memory/KV requirements are supplied; MeshFit does
   not currently infer those requirements from parameter count or arbitrary
   architecture metadata and is not a universal model-catalog compatibility
   service;
2. **which topologies are feasible** — implemented for the currently supported
   placement modes above;
3. **which plans are useful** — Pareto/rejection/exclusion logic exists, but its
   product value still needs real Benchmark 001 evidence;
4. **which nodes should participate** — implemented structurally;
5. **which nodes should be excluded** — implemented structurally; real
   evidence-backed exclusion remains part of the Reality Gate;
6. **how the model should be partitioned or replicated** — **partial**; TP and
   CPU-offload paths exist, while PP/EP/replication are deferred;
7. **which runtime should execute** — represented and compiled for the current
   vLLM/llama.cpp subset, not every advertised runtime capability;
8. **expected memory/network/latency/throughput/cost** — **partial**; memory
   and TP communication/cost contracts plus exact-evidence calibration exist,
   but `p95_latency_ms` / daily budget are not yet full solver gates and no
   generic cross-hardware latency/throughput predictor is claimed;
9. **dominant bottleneck** — available where current structural/evidence
   contracts support it; not a universal causal diagnosis engine;
10. **prediction vs measured reality** — exact-identity calibration contracts
    exist, but the first retained real heterogeneous campaign is still pending.

The project should expand any deferred item only after real evidence or an
external consumer demonstrates that it changes a decision or creates material
adoption value.
