# User-Story Truth Audit

_Last audited: 2026-10-08_

This document is the product truth table for MeshFit.

It exists to prevent three failure modes:

1. treating CI/synthetic coverage as a completed real-world user story;
2. re-implementing functionality that already exists because roadmap text is stale;
3. continuing low-value roadmap work after the durable product boundary has moved.

## North-star user

The durable user is not merely someone asking:

> Where should I place this model?

The higher-value user is someone who must answer:

> Given this heterogeneous infrastructure and workload, what placement was
> actually selected, what physical execution occurred, and does the retained
> evidence support the resulting performance/cost claim?

Planner quality still matters, but it is replaceable by stronger optimizers,
agents, or humans. The long-lived MeshFit boundary is **placement-claim
conformance**.

## Truth-state vocabulary

- **CLOSED — CODE + CONTRACT**: behavior exists and is regression-tested.
- **CLOSED — REALITY**: behavior has also been exercised on retained real
  heterogeneous hardware evidence.
- **REALITY-BLOCKED**: additional framework code is not the missing step; real
  hardware/external behavior is.
- **PARTIAL**: useful primitives exist, but the user can still fall through a
  manual or semantically ambiguous gap.
- **DEFER / KILL**: do not spend near-term effort unless reality or an external
  consumer proves the story is valuable.

## User-story audit

| ID | User story | Truth state | What is actually true | Closure condition |
|---|---|---|---|---|
| US-01 | As an operator, I can discover a heterogeneous estate without hand-authoring hardware facts. | **REALITY-BLOCKED** | Linux CPU/RAM, NVIDIA, Apple, AMD/Intel baseline parsers, local topology and runtime inventory exist. Multi-host fixture paths exist. | Retain discovery from >=2 real heterogeneous hosts and prove the generated snapshot requires no manual hardware invention. |
| US-02 | As an operator, I can measure the links that matter to cross-host placement. | **REALITY-BLOCKED** | RTT/jitter and optional iperf3 bandwidth probe exist; freshness/provenance gates exist. | Retain real bidirectional peer evidence for the campaign topology and use it in a real placement decision. |
| US-03 | As an operator, I can obtain real compute-ordering evidence instead of marketing TFLOPS. | **REALITY-BLOCKED** | `tools/compute_proxy.py` emits stable measured relative-compute evidence and the readiness gate rejects zero/unmeasured scores. | Run the same proxy on all campaign GPUs, retain raw logs, and bind scores to discovered physical devices. |
| US-04 | As a planner user, I can enumerate feasible placements and explicitly exclude harmful resources. | **CLOSED — CODE + CONTRACT** | Structural feasibility, per-device memory, TP communication gates, Pareto frontier and rejection/exclusion reasons are implemented. | Real campaign should demonstrate at least one evidence-backed exclusion; no new planner feature is required beforehand. |
| US-05 | As an operator, a placement can compile into something a runtime can execute. | **REALITY-BLOCKED** | llama.cpp and vLLM compilers, device binding, executable plan contract and host/run preflight exist. | One generated plan must launch on real hardware from the frozen campaign artifact. |
| US-06 | As an experiment owner, execution writes provenance-complete benchmark evidence automatically. | **REALITY-BLOCKED** | BenchmarkBundle, ExecutionIdentity, source/binary identity, request/wave measurements, repeated-run execution and re-attestation exist. | Produce retained real BenchmarkBundles for all Benchmark 001 candidates. |
| US-07 | As a reviewer, I can tell whether a published placement claim is supported by the exact frozen evidence. | **CLOSED — CODE + CONTRACT** | `benchmark-proof.yaml`, independent verifier, fail-closed publication checks and proof-consumer Action exist; positive/negative wrapper conformance is merged. | External consumer/reviewer must independently run or consume it before this becomes institutionally countersigned. |
| US-08 | As a reviewer, I can receive a proof without reconstructing MeshFit's internal directory layout. | **PARTIAL** | Proof already indexes all frozen inputs/bundles. PR #132 adds self-contained materialization + re-verification. | Merge #132 after CI; then exercise the exported package outside the producing working tree. |
| US-09 | As a downstream CI/policy system, I can consume a stable machine verification result. | **PARTIAL** | Proof Action is merged. PR #134 adds portable JSON verification, public schema, exact subject/predicate digest binding and standard attestation composition. | Merge #134 after CI and get one independent downstream consumer. |
| US-10 | As an MLCommons/InfraGraph consumer, I can reuse observed inventory without MeshFit inventing inter-host fabric or runtime evidence. | **PARTIAL** | PR #133 implements a fail-conservative planning-seed bridge, preserves CPU-only nodes, keeps unknown runtime/network facts unknown, and round-trips through `snapshot-manifest`. | Merge after CI, then validate the pairing identifier with MLCommons/InfraGraph maintainers or adapt to the upstream answer. |
| US-11 | As a contributor/consumer, I have a standard way to report proof/interoperability failures with immutable evidence. | **PARTIAL** | PR #138 adds a proof-consumer interoperability issue form. | Merge after CI; first independent report proves the surface is actually useful. |
| US-12 | As an operator, MeshFit asks for the minimum missing evidence and tells me what measurement to run. | **PARTIAL** | `recommend-probes` ranks evidence gaps and explains suggested actions. `meshfit probe` and `compute_proxy.py` exist separately. | Only promote this story when recommendations carry enough concrete target identity to produce an executable probe action and a before/after decision delta can be demonstrated. |
| US-13 | As a user, MeshFit predicts latency/throughput for unseen placements with calibrated uncertainty. | **DEFER / KILL NEAR-TERM** | Exact-identity empirical calibration exists; a general cross-hardware latency/throughput predictor does not. | Do not implement before Benchmark 001 real evidence demonstrates a decision where such transfer would change the outcome. |
| US-14 | As an operator, MeshFit continuously watches telemetry and re-places workloads when reality changes. | **DEFER / KILL NEAR-TERM** | No live telemetry/re-placement loop exists. | Keep out of the near-term roadmap until a real external operator asks for it after the static placement-claim loop is externally consumed. |
| US-15 | As a third party, I can cite/reuse MeshFit without first trusting hippoley's interpretation. | **REALITY-BLOCKED / EXTERNAL** | Public schemas, verifier, Action, evidence index and interoperability surfaces exist. | Independent repo use, reproduction/rejection, upstream review, WG record, or accepted contribution. |
| US-16 | As the project owner, I can produce the first defensible product claim. | **REALITY-BLOCKED — P0** | Framework is intentionally frozen around Benchmark 001. | Close #82: real heterogeneous hardware + measured compute/network evidence + >=3 distinct executable plans + repeated raw bundles + publishability + proof + headline. |

## False gaps removed by this audit

### Bounded evidence transfer is already implemented

The roadmap still described bounded evidence transfer as missing, but the current
CLI already provides:

```bash
meshfit benchmark-export-host ...
meshfit benchmark-import-host ...
```

with typed transfer records and coordinator re-attestation.

That story is **closed at code/contract level**. A real campaign is still needed
to validate it operationally.

### Targeted probes are not "missing from zero"

MeshFit already has:

- `meshfit probe <peer> --bandwidth`;
- `tools/compute_proxy.py`;
- `recommend-probes`.

The remaining user-story gap is not another probe engine. It is the
**recommendation -> exact target -> executable measurement -> changed decision**
loop.

Do not create duplicate probe implementations.

## Product-story correction

The original product story is:

```text
Fit -> Place -> Verify
```

The durable priority is now:

```text
Observe
  -> Place
  -> Precommit
  -> Execute
  -> Evidence
  -> Verify
  -> Claim
```

"Place" remains useful, but "Verify" is now the harder-to-substitute identity
surface.

## Near-term action order

1. **Reality Campaign 001 (#82)** — no substitute.
2. Merge already-implemented consumption/interop work after CI:
   #132, #133, #134, #135, #138.
3. Obtain one external semantic answer / consumer behavior:
   MLCommons Benchmark Infra first; CATPTS/CATS as a high-upside option.
4. Only if a real campaign exposes a concrete operational gap, implement that
   blocker.
5. Do not start v0.6 adaptive placement or a general latency/throughput predictor
   merely to complete the old roadmap.

## Pivot rule

MeshFit is an implementation/evidence vehicle, not a sunk-cost commitment.

Revalue the active node whenever one of these changes:

- an external maintainer asks for or rejects an artifact;
- another project begins consuming the proof/action/schema;
- an upstream standard defines the same boundary better;
- real hardware shows the current methodology is unnecessary or wrong;
- a newer node offers materially higher institutional leverage with high reuse of
  MeshFit's evidence assets.

If the expected external-position gain from another month of MeshFit-internal
work is lower than the expected gain from an adjacent active node, **pivot the
public contribution while carrying the reusable evidence methodology forward**.
