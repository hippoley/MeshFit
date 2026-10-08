# MeshFit Evidence Index

_Last updated: 2026-10-08_

This page is the shortest stable entry point for reviewing MeshFit's
**decision-to-execution provenance** work.

It is not a product overview and not a substitute for raw benchmark evidence.
It answers three reviewer questions:

1. What has actually been implemented and frozen?
2. Which public artifacts can be independently inspected?
3. What has **not** yet received external validation?

## Core claim

MeshFit treats an infrastructure placement as a claim that must be bound to:

```text
workload / request identity
        ↓
observed infrastructure
        ↓
precommitted placement decision
        ↓
exact source + executable identity
        ↓
execution identity
        ↓
raw execution evidence
        ↓
claim gate / benchmark proof
```

The implementation goal is not to replace system inventory, topology schemas,
provider APIs, or benchmark policy. It is to make the
**decision → execution → claim** boundary auditable.

## Durable implementation evidence

| Capability | Public evidence | What it establishes |
|---|---|---|
| One source revision per campaign | `b7c2b970b12da4b1dc696055321a2c14f0d12142` / PR #103 | benchmark evidence cannot silently mix source revisions |
| Placement frozen before measurement | `4c36833ab708cdf332b53e74ce02d10915861626` / PR #104 | placement selection is precommitted before performance results |
| Repeated-run ExecutionIdentity invariant | `9b3d4b47707aeef73799e1741862efea7dda8189` / PR #105 | repeated runs preserve hardware/runtime/model/topology/placement identity |
| One frozen campaign binary | `ffc1d50b63a312a44aaa5e05d79fa8cd91a3142d` / PR #106 | campaign distribution uses one executable artifact |
| Executing binary SHA-256 bound into evidence | `e0730ca89621c4f9bafb7b13833a6d42c44e153f` / PR #116 | each BenchmarkBundle records the actual running executable bytes; publishability rejects mixed binaries |
| Runtime inventory before paid execution | `2b66e9b0415624867584da56734887c85503522c` / PR #107 | runtime compatibility is recorded before spend |
| Trace-to-Placement boundary | `7ad78e201e7c7f3cca9d345c7bfe5f097979eff6` / PR #108 | portable workload semantics are separated from physical placement |
| Bidirectional network evidence | `52de5cdbed6198c736dc83635bec061e3371c586` / PR #95 | undirected planning uses measured forward/reverse bandwidth |
| Conservative bandwidth invariant | `b00133ea715e937cb47cac3c130beb5c3cfb9a09` / PR #96 | effective bandwidth cannot exceed the slower measured direction |
| Peer-evidence freshness | `fa6b9ecd0afbef61480f3bb9169e9b491d873748` / PR #98 | stale/future-dated physical evidence is rejected |
| Freshness rechecked per real run | `59da171a73685e35a3306a31a73f53fd5ba96a95` / PR #99 | long batches cannot continue after evidence expires |
| Provider launch / termination evidence | `110440bd227029feef8c8dad3f6281f92884f85f` / PR #100 | provider identity, launch, path validation and billing-stop actions are retained |
| Executable Lambda pre-spend capacity receipt | `5ab411f9d50e8d988103a0466ba93d284c9fc7c8` / PR #113 | authenticated instance-type response is hashed; exact shapes, common live region, burn rate and $50 wall-clock ceiling are fail-closed |
| External audit handoff root | `d882be1f6301876e4ed79aa9441d4d0418a14244` / PR #117 | `benchmark-proof.yaml` is the reviewer root linking frozen inputs, candidate plans and raw bundles by SHA-256 |
| Independent proof verification | `130ff0c64a4d89e46b0d29b7a37e2a1ff876cfd2` / PR #120 | an external consumer can reject path escape, hash tamper, identity drift, completeness mismatch and report mismatch without asking the planner to make a new decision |
| Reviewer-facing Proof Explorer | `a3bd8b1f53f767124f887163472e89d240f91da1` / PR #125 | a third party can inspect the public proof contract interactively without treating the UI as the verifier |

## External audit root

The preferred reviewer entry point after a real campaign is:

```text
benchmark-proof.yaml
        ↓
frozen input hashes
        ↓
candidate plan_id
        ↓
raw BenchmarkBundle hashes
        ↓
MeshFit source commit + executing binary SHA-256
        ↓
ExecutionIdentity / request / model / runtime / topology
        ↓
measurements
        ↓
comparison report / claim gate
```

A reviewer should be able to reject a claim when any referenced hash, plan
binding, binary identity, execution identity, or publication gate fails.

No second MeshFit-specific run-audit sidecar is required for this chain.

## Primary reality gate

The engineering framework is not the current proof bottleneck.

The active campaign remains **#82 — P0 Reality Campaign: produce the first real
Benchmark 001 headline**.

Before spending GPU money, the captured authenticated Lambda
`GET /api/v1/instance-types` response is passed through
`tools/lambda_capacity_gate.py`. The resulting receipt binds the provider
response hash to the exact selected shapes, common live region, current
concurrent burn and budget-derived maximum wall-clock time.

A campaign is still not complete until it has:

- real heterogeneous hardware;
- real measured compute/network evidence;
- repeated raw BenchmarkBundles;
- re-attestation;
- a generated benchmark proof;
- an explicit claim result.

Synthetic fixtures do not satisfy that gate.

## External interoperability boundary

MeshFit should not duplicate existing infrastructure inventory, topology graph,
or generic automation-provenance systems.

The intended boundary is:

```text
external system inventory / topology
        ↓
MeshFit placement decision
        ↓
decision fingerprint
        ↓
ExecutionIdentity
        ↓
evidence references
        ↓
claim status
```

Current public alignment work is tracked in:

- #109 — Chakra trace-to-placement boundary
- #111 — Chakra → ASTRA-sim → MeshFit conformance case
- #112 — MLPerf / MLCommons disclosure and provenance alignment

These issues document interoperability questions. They are **not** evidence of
external adoption.

## External validation status

As of 2026-10-08:

- **Third-party citation / reuse / dependency:** not yet verified.
- **Durable portfolio credential:** yes — public commits, contracts and evidence
  rules are stable and reviewable.
- **Externally countersigned institutional credential:** not yet.
- **Portable reputation inside MLCommons / benchmark-governance communities:**
  not yet.

A self-authored issue, internal merge, star, or README mention does not count as
external validation.

Signals that do count include:

- substantive upstream maintainer response;
- an external issue or PR linking a MeshFit artifact;
- independent reproduction;
- request for an example, adapter, PR or review;
- merged contribution in an external repository;
- working-group record or proactive reviewer invitation.

## Three-minute reviewer path

A reviewer can test the central claim in this order:

1. [Proof Explorer](../web/proof-explorer/) — understand the decision/evidence/claim boundary interactively
2. [Trace-to-Placement Contract](TRACE_TO_PLACEMENT_CONTRACT.md)
3. [Reality Campaign 001](REALITY_CAMPAIGN_001.md)
4. PR #104 — verify placement is frozen before measurement
5. PR #105 — verify repeated-run execution identity is enforced
6. PR #116 — verify actual executable bytes are bound into each run
7. PRs #95/#96/#98/#99 — verify physical network evidence is conservative and fresh
8. PR #113 — verify provider capacity/cost selection is a fail-closed receipt
9. PR #117 — verify `benchmark-proof.yaml` is the external audit root
10. PR #120 — independently verify the receipt against the frozen kit and raw bundles
11. Issue #82 — verify whether real BenchmarkBundles and a benchmark proof now exist

If #82 still has zero real bundles, the correct conclusion is:

> the methodology is implemented and externally inspectable, but the first real
> placement proof is still pending.

## Anti-substitution north star

MeshFit should assume that placement generation itself will become increasingly
commoditized by stronger optimizers, LLMs and infrastructure agents.

The durable layer is therefore not ownership of a particular scheduling
heuristic. It is the trust boundary around a decision:

```text
optimizer / heuristic / LLM / human
        ↓
precommitted decision
        ↓
measured physical evidence
        ↓
execution identity
        ↓
tamper-evident proof
        ↓
independent verification
        ↓
externally reviewable claim
```

A stronger model can become another decision producer without replacing this
verification layer.

Future work should be prioritized only when it does at least one of the
following:

- binds a claim to physical or externally observable reality;
- reduces the gap between a precommitted decision and what actually executed;
- makes evidence easier for an independent party to reject or reproduce;
- creates a reusable artifact that can enter an external review or governance
  process;
- solves a concrete cost, reliability, capacity, compliance or audit problem on
  real infrastructure.

Features whose only advantage is "the planner is smarter" should be treated as
replaceable unless they produce one of those durable outputs.

## 5–10 year credential boundary

Not every current artifact is expected to remain important for a decade.

### Durable methodology assets

These should remain legible even if runtimes, accelerators, providers, and
benchmark versions change:

- precommit the placement decision before observing performance;
- bind claims to immutable source and executable identity;
- bind repeated runs to one ExecutionIdentity;
- retain measured physical evidence with capture time/freshness;
- preserve raw result hashes and an audit root;
- separate observed infrastructure from downstream placement decisions;
- make claim publication fail closed when evidence cannot support it.

### Replaceable 2026 implementation details

These are useful execution details, not the long-term credential itself:

- Lambda-specific instance-type names and prices;
- A6000 / H100 / B200 as the first campaign shape;
- vLLM or any particular runtime version;
- the exact Benchmark 001 objective and campaign budget;
- individual CLI command names;
- current provider/network plumbing.

A durable identity claim should therefore be phrased around the methodology and
externally reviewed evidence chain, not around a transient provider or release.

The 5–10 year credential becomes materially stronger only when the methodology is
also countersigned outside this repository through review, reproduction, merged
contributions, working-group records, or repeated requests for technical judgment.

## Graduation criteria

MeshFit moves from a project toward an external position only when:

```text
real campaign artifact
    ↓
third-party technical response
    ↓
external review / reproduction / reuse
    ↓
accepted external contribution
    ↓
repeated request for MeshFit / hippoley context
```

This index exists to keep those transitions falsifiable and inexpensive to
inspect over time.