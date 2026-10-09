# MLCommons InfraGraph → MeshFit Planning-Seed Boundary

_Status: experimental interoperability bridge; not MLCommons/InfraGraph conformance._

This integration exists to avoid duplicating infrastructure inventory work that
MLCommons is already building.

## Boundary

MLCommons `mlperf-automations` PR #1088 defines an opt-in InfraGraph path that
pairs, per node:

```text
<stem>.lstopo.xml
<stem>.json
```

and produces a merged InfraGraph YAML whose instances retain the source
`description = <stem>`.

As of upstream head `1aeb3c4dfbb09c6986e84a74f879a4f32547b7e6`
(checked 2026-10-08), this is no longer merely inferred from an example:
the PR description calls the shared stem "the whole pairing contract", and
`generate-infragraph/build_infragraph.py` explicitly keeps the stem in
`instance.description` because it is the link back to that node's sysinfo JSON
after `instance.name` has been replaced by the real hostname.

MeshFit consumes that pairing only as a **planning seed**.

It does not treat the imported material as final execution attestation.

## Adapter

```bash
cat > hourly-costs.yaml <<'YAML'
node-a: 3.29
node-b: 6.99
YAML

python3 tools/mlcommons_infragraph_to_meshfit.py \
  --infragraph-yaml infragraph.yaml \
  --sysinfo-dir /path/to/mlperf-node-captures \
  --out-dir meshfit-seed \
  --hourly-cost-map hourly-costs.yaml
```

The adapter writes:

```text
meshfit-seed/
  discovery-<node>.yaml
  ...
  snapshot-manifest.yaml
```

The generated manifest intentionally contains no peer probes.

## Facts imported

When present and parseable, the bridge carries:

- node identity from InfraGraph `instance.name`;
- source pairing from `instance.description`;
- CPU model from MLCommons sysinfo;
- approximate host RAM for capacity planning;
- accelerator model/count/per-device memory;
- compute backend;
- driver version;
- OS family;
- CPU architecture from the paired lstopo XML.

Node hourly cost is **not** imported from MLCommons sysinfo because the upstream
capture does not define that fact. MeshFit's current `HardwareNodeIR` uses a
non-optional `hourly_cost_usd`, where `0.0` means an explicitly declared
zero marginal compute cost rather than "unknown". The bridge therefore refuses
implicit zero and requires one of:

- `--hourly-cost-map <yaml/json>` with an exact `instance.name -> USD/hour`
  entry for every imported node; or
- `--assume-zero-hourly-cost` when the operator intentionally declares zero
  marginal compute cost for the entire imported estate.

For cloud/hybrid estates, prefer the cost map. The zero-cost flag is an explicit
semantic assertion, not a convenience default.

## Facts intentionally *not* promoted

The bridge deliberately leaves:

- `hardware_identity.ram_mib = null`;
- `free_memory_gb = null`;
- `relative_compute = 0`;
- `local_fabric = []`;
- peer probes empty;
- node hourly cost from upstream inventory (the operator must declare it
  separately).

Why:

1. MLCommons host-memory values are presentation-oriented strings, so converting
   them back into an exact RAM identity would manufacture precision.
2. Free accelerator memory is runtime state, not a static system description.
3. Benchmark 001 requires a measured compute proxy; model names or marketing
   FLOPS cannot substitute.
4. PR #1088 explicitly states that per-host lstopo data cannot infer the
   inter-node fabric.
5. Execution readiness must be checked on the machine that actually runs the
   candidate.
6. MLCommons sysinfo does not establish a node's marginal hourly compute cost;
   silently mapping absence to `0.0` would manufacture a cost claim.

## Required upgrade before real execution

An imported seed is therefore expected to fail or remain provisional until the
operator performs real MeshFit evidence collection:

```text
MLCommons InfraGraph/sysinfo seed
        ↓
meshfit discover on each execution host
        ↓
measured compute proxy
        ↓
meshfit probe <peer> --bandwidth
        ↓
frozen placement
        ↓
real BenchmarkBundles
        ↓
proof / independent verification
```

The goal is composition:

> **MLCommons/InfraGraph describes observed infrastructure; MeshFit binds a
> placement decision to later physical execution evidence.**

It is not for MeshFit to replace InfraGraph's graph model, submission sysinfo,
or hardware collection ecosystem.

## Remaining upstream observation

The source-stem pairing semantics are now clear enough for this bridge; no
generic pairing-key question remains.

PR #1088 also surfaces a CPU-only behavior where using a GPU-runtime variation
can suppress otherwise useful sysinfo from a node. That upstream issue matters
to MeshFit because a heterogeneous placement system should retain CPU-only
nodes as observed infrastructure even when they are not accelerator candidates.

The adapter's regression fixture explicitly retains a CPU-only InfraGraph
instance with an empty accelerator list. A node without an accelerator is still
observed infrastructure and must not disappear merely because a GPU-runtime
variation is unavailable.

A useful future upstream artifact is therefore a small compatibility example,
not a new competing schema:

```text
MLCommons node/system inventory
        ↓
MeshFit planning seed
        ↓
explicit missing-evidence gates
```

This document and adapter are not evidence that MLCommons has accepted or used
MeshFit.
