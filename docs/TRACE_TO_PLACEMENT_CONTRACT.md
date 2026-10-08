# Trace-to-Placement Contract

MeshFit treats workload description and physical placement as separate concerns.

The **Trace-to-Placement Contract** defines the minimum boundary between a portable
workload representation and a heterogeneous placement system. It is intentionally
not tied to one trace format, simulator, runtime, or cluster vendor.

A workload-side producer may be an execution trace system such as MLCommons
Chakra, a benchmark workload, a framework profiler, or another workload model.
MeshFit is one placement-side consumer.

## Why this boundary exists

A portable workload trace can describe what work happened without deciding where
that work should run. A heterogeneous placement system can describe real hardware
without knowing which workload properties must be preserved.

Without an explicit boundary, placement experiments are easy to make
irreproducible:

- workload semantics can be silently reinterpreted by each consumer;
- physical topology assumptions can be mixed into the trace representation;
- a selected placement can be changed after observing benchmark results;
- execution evidence can no longer prove which decision was actually evaluated.

The contract separates those responsibilities.

## 1. Workload-side semantics

A workload producer should expose only the semantics needed to constrain or score
placement. Depending on the workload, that may include:

- operation/dependency structure;
- compute intensity or duration observations;
- memory footprint or memory-pressure signals;
- communication volume and collective semantics;
- data-movement requirements;
- concurrency / active-sequence behavior;
- timing or rate information;
- parallelism constraints or hints;
- explicitly unknown quantities.

The contract does **not** require a specific trace schema. Generic attributes are
acceptable when their meaning and units are stable and documented.

## 2. Infrastructure-side semantics

The placement side supplies physical facts independently from the workload trace:

- node identity;
- accelerator identity and usable memory;
- runtime/backend capability;
- local accelerator topology;
- cross-host topology;
- measured latency;
- measured directional bandwidth;
- architecture / operating-system identity;
- relevant software/runtime inventory;
- provenance and capture time for measured evidence.

Configured link speed or marketing specifications are not substitutes for
measured evidence when a decision depends on real communication performance.

## 3. Decision boundary

A placement decision must be explicit before performance evidence is observed.

At minimum the decision should bind:

- a stable plan identifier;
- selected nodes and accelerators;
- runtime/backend;
- placement mode;
- workload input identity/fingerprint;
- infrastructure snapshot identity;
- decision source revision;
- constraints relied upon by the decision.

A post-hoc plan change is a new experiment.

## 4. Execution conformance

Execution evidence should make it possible to answer:

> Did the measured run execute the decision that was actually selected?

A conformance record should therefore bind, directly or through immutable
referenced inputs:

- plan identity;
- workload/request contract;
- model/artifact identity;
- physical hardware identity;
- local topology identity where relevant;
- runtime identity;
- source revision;
- measured network evidence where relevant;
- capture time;
- repeated-run identity.

A comparison is not publishable if repeated runs silently mix incompatible
decision or execution identities.

## 5. Contract invariants

The current MeshFit Reality Campaign follows these invariants:

1. **No hidden topology substitution** — a required physical path must be
   demonstrated, not inferred from provider metadata.
2. **No stale network evidence** — peer measurements are freshness-gated at
   each real-run launch.
3. **No input drift** — materialized control-plane files are SHA-256 verified on
   every benchmark host.
4. **No mixed MeshFit build** — one frozen binary and one source revision are
   used across a campaign.
5. **No mixed repeated-run identity** — repeated bundles for a strategy must
   preserve one execution identity.
6. **No post-hoc placement selection** — the MeshFit plan is frozen before
   performance benchmarking begins.
7. **No unverifiable result import** — transferred host evidence is
   re-attested by the coordinator before it becomes campaign evidence.

## 6. Interoperability model

A trace system does not need to know MeshFit's internal IR.

The intended adapter shape is:

```text
Portable workload representation
        |
        | workload semantics
        v
Trace-to-Placement adapter
        |
        | placement-relevant constraints/signals
        v
MeshFit workload / target representation
        |
        +-----------------------+
        |                       |
        v                       v
InfrastructureSnapshot      measured evidence
        |                       |
        +-----------+-----------+
                    |
                    v
             placement decision
                    |
                    v
               execution
                    |
                    v
          conformance evidence
```

This leaves workload trace standards free to remain portable, and leaves
infrastructure description and physical evidence with the placement system.

## 7. Chakra as one possible producer

MLCommons Chakra is a useful example because its execution traces represent
compute, memory, communication, dependencies, timing, and extensible attributes.

This contract does **not** propose that Chakra adopt MeshFit-specific fields.

The interoperability question is narrower:

> Which placement-relevant workload semantics should a downstream heterogeneous
> placement consumer be able to interpret canonically, and which physical
> resource/topology facts should remain outside the workload trace?

If the existing Chakra attribute model is the intended extension boundary, a
MeshFit adapter can consume that boundary without requiring a schema change.
If the community identifies missing canonical semantics, those should be
discussed independently of MeshFit's internal representation.

## 8. Non-goals

This contract is not:

- a new trace standard;
- a replacement for Chakra, MLPerf, OCP, vLLM, Slurm, Ray, or Kubernetes;
- a universal scheduler API;
- a claim that all workloads require all fields above;
- a requirement that runtime choice be fixed across candidate placements.

Runtime may itself be part of a placement decision. What must remain fixed is
the identity of the decision and the evidence needed to reproduce it.

## 9. Current proof point

MeshFit Reality Campaign 001 is the first concrete conformance case for this
contract. It currently freezes and/or verifies:

- a pre-benchmark placement decision;
- one MeshFit source revision and binary artifact;
- workload/request inputs;
- model artifact SHA-256;
- host hardware/topology;
- runtime availability;
- measured peer links and freshness;
- repeated execution identity;
- bounded evidence transfer;
- final publication gates.

The next useful validation is external: map a portable workload representation
onto this contract and identify whether the boundary is sufficient without
changing either system's core schema.

## 10. External audit handoff root

For external review, the root artifact is the generated `benchmark-proof.yaml` receipt rather than a new provenance schema.

The receipt is designed to let an independent reviewer walk from a claim back to the exact frozen inputs and raw execution evidence used to support it. It includes SHA-256 references for:

- the execution kit and comparison manifest;
- the infrastructure snapshot;
- the placement target;
- the model identity;
- the frozen prompt;
- every raw BenchmarkBundle included in the comparison.

For each candidate it also records the frozen `plan_id`, benchmark host, runtime, bundle path/hash, MeshFit source commit, **executing MeshFit binary SHA-256**, and capture time. The referenced BenchmarkBundle carries the full execution identity, so the receipt does not duplicate that schema.

The intended verification path is:

```text
benchmark-proof.yaml
        |
        +--> frozen inputs (hash verified)
        |
        +--> candidate plan_id
        |
        +--> raw BenchmarkBundle hashes
                    |
                    +--> MeshFit source + binary identity
                    +--> ExecutionIdentity
                    +--> request/workload contract
                    +--> model/runtime/topology identity
                    +--> measurements
        |
        +--> comparison report / claim gate
```

An external consumer should be able to reject the claim if any referenced file hash, plan binding, source revision, binary identity, execution identity, or publication gate fails to match. No external integration should require MeshFit to invent a second sidecar carrying the same information.

This receipt is the preferred handoff artifact for future reproducibility or benchmark-infrastructure discussions. A downstream standard may choose to reference or hash it, but the underlying Reality Campaign evidence remains authoritative.

