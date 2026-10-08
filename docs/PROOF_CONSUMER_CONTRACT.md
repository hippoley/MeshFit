# Proof Consumer Contract

_Status: experimental / pre-v1_

This document defines the stable behavior expected by a downstream consumer of
MeshFit benchmark proofs. It is intentionally narrower than the planner API.

## Purpose

A consumer should be able to decide:

> Do the exact proof bytes I received independently verify against the supplied
> audit root, and may my policy continue?

The consumer must not need to ask MeshFit to generate a new placement.

## Inputs

The CI action accepts:

- `proof` — path to a serialized `meshfit.benchmark-proof/v1` receipt;
- `root` — path to the materialized audit root referenced by the receipt;
- `require-publishable` — when `true`, fail unless the recomputed report is
  publishable.

Paths are interpreted in the caller's workspace. The action does not upload the
proof or audit root.

## Fail-closed semantics

Verification fails when the underlying independent verifier rejects, including
for:

- unsupported proof schema;
- path escape or unresolved audit-root references;
- frozen-input hash mismatch;
- raw BenchmarkBundle hash mismatch;
- benchmark, plan, source commit, executing binary, or capture identity drift;
- missing or duplicate required inputs;
- completeness mismatch;
- comparison/bundle-list mismatch;
- report mismatch after recomputation;
- publishability metadata mismatch;
- non-publishable evidence when `require-publishable=true`.

A failed verification must produce a failing CI step.

## Output

On success the action exports:

```text
proof-sha256
```

This is the SHA-256 of the exact serialized proof bytes that passed
verification.

The digest is intended to be the bridge to a relying party's next layer:

```text
verified MeshFit proof
        ↓
proof-sha256
        ↓
attestation / review record / release evidence
        ↓
consumer policy
```

## Trust non-claims

Successful MeshFit proof verification means the receipt is internally consistent
with the materialized evidence set and its recomputed report under the MeshFit
verification rules.

It does **not** by itself establish:

- who produced or endorsed the proof;
- that the evidence source is institutionally trusted;
- that a signer is authorized by the relying party;
- SLSA conformance;
- MLPerf / MLCommons acceptance;
- InfraBench acceptance;
- correctness of a third-party attestation.

Those belong to an authenticated attestation and consumer-policy layer.

## Versioning

Consumers should pin the GitHub Action to a full MeshFit commit SHA.

The proof wire format is independently versioned by its `schema` field. A task
fixture, experiment-local document, or UI representation must not reuse
`meshfit.benchmark-proof/v1` unless it implements that public receipt shape.

Breaking consumer-action semantics should require a new documented action
version before a stable release tag is offered.

## Externalization threshold

This action currently lives inside the MeshFit repository while the interface is
being exercised.

If an independent repository begins using it, the preferred next step is to
extract the action into a dedicated repository/release unit so it can be
versioned and discovered independently without coupling consumers to MeshFit's
application release cadence.
