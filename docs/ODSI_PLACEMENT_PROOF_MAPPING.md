# ODSI ↔ MeshFit Placement-Proof Mapping

_Status: exploratory implementation mapping; not an ODSI conformance claim._

This note maps the current MeshFit evidence objects onto concepts in
`draft-wang-cats-odsi-01`, "An Architecture for Open, Decentralized, and
Scalable Large Language Model Inference".

The purpose is to test whether MeshFit's execution-bound placement proof can
serve as one implementation experiment for an ODSI-style path verification
profile.

It does **not** claim that MeshFit implements ODSI, that ODSI has adopted these
objects, or that the terminology below is normative.

## Why this boundary is interesting

ODSI deliberately allows path construction to be performed by different
mechanisms and does not require one globally optimal scheduler. It separately
defines verification as a profile-level concern.

That separation matches MeshFit's anti-substitution boundary:

```text
replaceable path / placement producer
        ↓
precommitted decision
        ↓
execution-bound evidence
        ↓
independent verification
```

A stronger optimizer or LLM can replace the placement producer without removing
the need to verify what was actually executed.

## Object mapping

| ODSI concept | MeshFit object today | Mapping strength | Important gap |
|---|---|---:|---|
| RESOURCE PROFILE | `InfrastructureSnapshot`, discovery records, hardware identity, measured peer evidence | partial | MeshFit does not currently define participant signatures, sequence numbers, or an ODSI resource-profile wire format |
| recent observations | timestamped peer RTT/bandwidth evidence, compute proxy, runtime inventory | strong for Benchmark 001 | observation trust/authentication is deployment-specific |
| EXECUTION PATH | frozen MeshFit placement / candidate `plan_id` + selected nodes/runtime/topology | partial-to-strong | MeshFit plans inference placement, but does not model every ODSI execution-unit/predecessor semantic |
| EXECUTION COMMITMENT | pre-benchmark placement freeze + request/model/source/executable binding | partial | no participant-signed ODSI commitment object exists |
| EXECUTION PROFILE | frozen workload/request/model/runtime constraints in benchmark kit | partial | no ODSI profile identifier or negotiation semantics |
| EXECUTION RECEIPT | `BenchmarkBundle` + `ExecutionIdentity` + provenance + measurements | strong as an experimental analogue | MeshFit bundles are benchmark receipts, not signed per-unit ODSI receipts |
| VERIFICATION PROFILE | `meshfit.benchmark-proof/v1` verification rules + publishability gate | partial-to-strong | currently MeshFit-specific, not expressed as an ODSI profile |
| verification decision | `meshfit.benchmark-proof-verification/v1` | strong as an implementation result | no ODSI decision-report wire format |
| evidence adjudication | fail-closed hash/identity/report/publishability checks | partial | no multi-verifier/adjudicator/finality semantics |
| settlement consequence | none | none | intentionally outside MeshFit scope |

## Placement-proof profile experiment

A narrow experiment can be stated as:

> Given a precommitted heterogeneous inference path, determine whether the
> materialized execution evidence supports the later placement-performance
> claim without trusting the path constructor.

MeshFit currently verifies:

1. exact frozen input hashes;
2. candidate plan binding;
3. raw bundle hashes;
4. benchmark ID and source plan identity;
5. source commit and executing binary identity;
6. capture identity;
7. completeness of expected repeated runs;
8. comparison manifest ↔ bundle-list consistency;
9. recomputed benchmark report;
10. evidence publishability state.

These checks concern **decision-to-execution conformance for a placement claim**.
They do not attempt to prove arbitrary model-computation correctness.

## Important semantic distinction

ODSI correctly distinguishes signed statements from proof of underlying facts.

MeshFit should preserve the same boundary:

```text
signature / attestation
  proves origin + integrity of statement
        ≠
proof that advertised resources existed
        ≠
proof that computation was correct
        ≠
proof that a performance claim is supported
```

MeshFit's current verifier addresses only the last category for a specific
materialized benchmark evidence set.

An authenticated attestation envelope can answer who attested to the exact proof
bytes; it must not replace the evidence verifier.

## Freshness and measurement source

ODSI treats fast-changing resource values as claims rather than facts and
requires deployments to account for measurement source and freshness.

MeshFit already has concrete fail-closed behavior for a subset of this problem:

- peer evidence has capture timestamps;
- stale/future-dated network evidence is rejected;
- real benchmark runs recheck freshness;
- compute ranking requires measured positive scores for the real campaign;
- synthetic/default network values cannot support cross-node Benchmark 001
  publishability.

This is one area where an implementation-driven profile could provide useful
feedback to ODSI: a path decision should carry enough observation identity to
let the verifier distinguish a valid pre-execution observation from stale or
post-hoc evidence.

## Missing pieces before any ODSI conformance claim

MeshFit would still need explicit choices for at least:

- ODSI object identifiers and canonical serialization;
- participant identity and signature semantics;
- signed RESOURCE PROFILE / EXECUTION COMMITMENT objects;
- execution-unit and predecessor semantics for multi-stage paths;
- attempt/retry/replacement identity compatible with ODSI path repair;
- verification-profile identifier and fault model;
- verifier independence/finality semantics;
- receipt exchange protocol;
- privacy and disclosure rules;
- any settlement-facing consequence semantics.

Until those exist, this document is only an interoperability experiment.

## Candidate feedback to the ODSI draft

The implementation suggests one concrete question for the ODSI verification
profile boundary:

> Should an execution-path verification profile be able to bind the
> path-construction observations that justified a path (resource/topology
> measurements and freshness) to the later execution receipts, so a verifier can
> distinguish a precommitted path from one reconstructed after seeing outcome
> performance?

For heterogeneous distributed inference, this matters because a path may be
technically executable while a later performance claim depends on network or
resource conditions that were not the conditions used when the path was chosen.

MeshFit's current experiment uses:

```text
observed infrastructure
        ↓
precommitted plan_id
        ↓
ExecutionIdentity
        ↓
raw BenchmarkBundles
        ↓
benchmark-proof.yaml
        ↓
independent recomputation
```

as one testable shape for that question.

## Evidence before advocacy

Before proposing text upstream, MeshFit should first retain one real Benchmark
001 proof and show that this mapping survives a real heterogeneous run.

A useful upstream contribution would then be implementation evidence:

- one real path decision;
- exact pre-execution observations;
- repeated execution receipts;
- one independently verified claim;
- explicit failure cases when observation freshness, path identity, or evidence
  binding is broken.

That is stronger than proposing terminology without a working artifact.
