# MeshFit Architecture

MeshFit is a **placement intelligence layer** for heterogeneous inference.

It does not execute model kernels. It does not replace vLLM, SGLang, llama.cpp, MLX, exo, Ray, Kubernetes, or Slurm.

Its job is to decide:

- what is feasible;
- where a model should run;
- how it should be partitioned or replicated;
- which runtime should execute the plan;
- which resources should be excluded;
- what evidence supports the decision.

## Core intermediate representations

### HardwareIR

Describes compute resources:

- CPU / RAM
- accelerators
- backend
- available memory
- relative compute
- site
- cost

### FabricIR

Describes the paths between resources:

- PCIe
- NVLink
- Ethernet
- InfiniBand
- Thunderbolt
- WAN
- bandwidth
- latency
- jitter
- egress cost

The important abstraction is **path capability**, not raw node capacity.

### ModelIR

Describes execution-relevant model structure:

- total parameters
- active parameters
- weight memory
- KV cache
- dense vs MoE
- future: attention heads, KV heads, expert count, communication signature

### RuntimeIR

Describes what an execution backend can actually do:

- supported accelerator backends
- TP
- PP
- EP
- RPC
- CPU offload
- future: disaggregated prefill/decode, KV connectors, distributed constraints

### WorkloadIR

Describes the workload contract:

- context
- concurrency
- latency SLO
- budget
- future: reliability, privacy, region, energy constraints

### PlanIR

Describes a candidate placement:

- selected nodes
- placement kind
- runtime
- memory
- compute
- cost
- assumptions
- future: predicted latency/throughput intervals, data movement, launch configuration

## Planning pipeline

~~~text
HardwareIR + FabricIR + ModelIR + RuntimeIR + WorkloadIR
                         │
                         ▼
                Feasibility Solver
                         │
                impossible plans removed
                         │
                         ▼
                  Candidate Search
                         │
                         ▼
                  Pareto Frontier
                         │
                         ▼
                       PlanIR
                         │
              ┌──────────┴──────────┐
              │                     │
         Plan Compiler          Predictor
              │                     │
        runtime config          EvidenceIR
              │                     │
              └──────────┬──────────┘
                         ▼
                       Verify
                         │
                         ▼
                     Calibrate
~~~

## v0.1 invariant

v0.1 only makes **structural feasibility claims**.

It may say:

- runtime/backend incompatible;
- memory insufficient;
- no measured link exists;
- WAN is below the cross-node TP admission gate;
- a candidate is Pareto-dominated;
- a node is not used by any Pareto plan.

It must not claim:

- real TTFT;
- real tokens/s;
- precise network traffic;
- production SLO compliance.

Those belong to later evidence-backed versions.

## Long-term PlanIR

The mature placement plan should eventually separate:

~~~text
PlacementPlan
├─ WeightPlacement
├─ KVPlacement
├─ PrefillPlacement
├─ DecodePlacement
├─ ExpertPlacement
├─ ReplicaPlacement
├─ DataMovementPlan
├─ RuntimePlan
└─ Evidence
~~~

This separation is important because modern inference is no longer one indivisible placement problem.
