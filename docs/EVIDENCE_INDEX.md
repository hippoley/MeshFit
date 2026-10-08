# MeshFit Evidence Index

_Last updated: 2026-10-08_

This page is the shortest stable entry point for reviewing MeshFit's
**decision-to-execution provenance** work.

It is intentionally not a product overview and not a substitute for raw
benchmark evidence. It answers three reviewer questions:

1. What has actually been implemented and frozen?
2. What evidence can be independently inspected?
3. What has **not** yet received external validation?

## Core claim

MeshFit treats an infrastructure placement as a claim that must be bound to the
decision that produced it, the physical evidence used by that decision, the
exact execution identity that ran it, and the result that is eventually
published.

```text
workload / request identity
        ↓
observed infrastructure
        ↓
precommitted placement decision
        ↓
execution identity
        ↓
raw execution evidence
        ↓
claim gate / benchmark proof
```

The implementation goal is not to replace system inventory, topology schemas,
or benchmark policy. It is to make the **decision → execution → claim** boundary
auditable.

## Durable implementation evidence

| Capability | Public evidence | What it establishes |
|---|---|---|
| One source revision per campaign | commit `b7c2b970b12da4b1dc696055321a2c14f0d12142` / PR #103 | benchmark evidence cannot silently mix source revisions |
| Placement frozen before measurement | commit `4c36833ab708cdf332b53e74ce02d10915861626` / PR #104 | placement selection is precommitted before performance results |
| Repeated-run ExecutionIdentity invariant | commit `9b3d4b47707aeef73799e1741862efea7dda8189` / PR #105 | repeated runs must preserve hardware/runtime/model/topology/placement identity |
| One frozen campaign binary | commit `ffc1d50b63a312a44aaa5e05d79fa8cd91a3142d` / PR #106 | execution cannot silently switch binaries mid-campaign |
| Runtime inventory before paid execution | commit `2b66e9b0415624867584da56734887c85503522c` / PR #107 | runtime compatibility is recorded before spend |
| Trace-to-Placement boundary | commit `7ad78e201e7c7f3cca9d345c7bfe5f097979eff6` / PR #108 | portable workload semantics are separated from physical placement |
| Bidirectional network evidence | commit `52de5cdbed6198c736dc83635bec061e3371c586` / PR #95 | undirected planning uses measured forward/reverse bandwidth |
| Conservative bandwidth invariant | commit `b00133ea715e937cb47cac3c130beb5c3cfb9a09` / PR #96 | effective bandwidth cannot exceed the slower measured direction |
| Peer-evidence freshness contract | commit `fa6b9ecd0afbef61480f3bb9169e9b491d873748` / PR #98 | stale/future-dated physical evidence is rejected |
| Freshness rechecked before every run | commit `59da171a73685e35a3306a31a73f53fd5ba96a95` / PR #99 | long batches cannot continue after evidence expires |
| Provider execution/termination receipts | commit `110440bd227029feef8c8dad3f6281f92884f85f` / PR #100 | provider identity, launch, network-path and billing-stop actions are retained |

## Primary reality gate

The engineering framework is not the current proof bottleneck.

The active campaign is tracked in **#82 — P0 Reality Campaign: produce the
first real Benchmark 001 headline**.

A campaign is not complete until it has real heterogeneous hardware,
real measured compute/network evidence, repeated raw BenchmarkBundles,
re-attestation, a benchmark proof, and an explicit claim result.

Synthetic fixtures do not satisfy that gate.

## External-interop boundary

MeshFit should not duplicate existing infrastructure inventory or automation
provenance systems.

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
- #112 — MLPerf/MLCommons disclosure and provenance alignment

These issues document interoperability questions; they are **not** evidence of
external adoption.

## External validation status

As of 2026-10-08:

- **Third-party citation / reuse / dependency:** not yet verified.
- **Durable portfolio credential:** yes — the public commits, contracts and
  evidence rules are stable and reviewable.
- **Externally countersigned institutional credential:** not yet.
- **Portable reputation inside MLCommons / benchmark-governance communities:** not yet.

This distinction is deliberate. A self-authored issue, internal merge, star,
or README mention does not count as external validation.

Signals that do count include:

- substantive upstream maintainer response;
- an external issue or PR linking a MeshFit artifact;
- independent reproduction;
- request for an example, adapter, PR or review;
- merged contribution in an external repository;
- working-group record or proactive reviewer invitation.

## Three-minute reviewer path

A reviewer who wants to test the central claim can inspect, in order:

1. [Trace-to-Placement Contract](TRACE_TO_PLACEMENT_CONTRACT.md)
2. [Reality Campaign 001](REALITY_CAMPAIGN_001.md)
3. PR #104 — verify placement is frozen before measurement
4. PR #105 — verify repeated-run execution identity is enforced
5. PRs #95/#96/#98/#99 — verify physical network evidence is conservative and fresh
6. Issue #82 — verify whether real BenchmarkBundles and a benchmark proof now exist

If #82 still has zero real bundles, the correct conclusion is:

> the methodology is implemented, but the first real externalizable placement
> proof is still pending.

## Graduation criteria

MeshFit should be considered to have moved from a project toward an external
position only when the following transitions occur:

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

The purpose of this index is to keep those transitions falsifiable and easy to
inspect over time.
