# Core User Story

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
