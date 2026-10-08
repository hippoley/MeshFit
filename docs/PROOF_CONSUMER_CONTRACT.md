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


## Benchmark-proof-verification v1 predicate semantics

The machine-readable verifier result has schema:

```text
meshfit.benchmark-proof-verification/v1
```

Its public JSON Schema is [`schemas/benchmark-proof-verification-v1.schema.json`](../schemas/benchmark-proof-verification-v1.schema.json). Downstream consumers may validate the result shape independently of the planner.

The v1 verification result intentionally excludes local `proof_path` and
`audit_root` filesystem strings. Those are invocation diagnostics, not portable
evidence facts, and including canonical runner paths would make otherwise
equivalent attestations differ across machines.

It is suitable as an attestation predicate because the attestation subject is
the exact verified proof digest, while the predicate records the independent
verification result.

A relying party should still apply its own signer/workflow policy. The
verification predicate does not become trustworthy merely because it is signed.

The current custom predicate TypeURI used by MeshFit examples is:

```text
https://github.com/hippoley/MeshFit/blob/main/docs/PROOF_CONSUMER_CONTRACT.md#benchmark-proof-verification-v1-predicate-semantics
```

This URI identifies the predicate semantics; it does not imply endorsement by
GitHub, Sigstore, in-toto, or another standards body.

## Portable proof package

A reviewer or downstream repository should not have to infer which files belong
to a proof.

MeshFit can materialize the exact dependency closure already named by the proof:

```bash
meshfit benchmark-export-proof benchmark-proof.yaml \
  --root benchmark-001 \
  --out exported-proof
```

The export operation:

1. verifies the source proof against the source audit root;
2. reads only input and bundle paths already referenced by
   `meshfit.benchmark-proof/v1`;
3. rejects path escape and the reserved `benchmark-proof.yaml` destination;
4. writes the exact proof bytes plus the referenced artifacts while preserving
   their relative paths;
5. re-runs independent proof verification against the exported directory;
6. requires the exported proof SHA-256 to match the source proof SHA-256.

The destination must not already exist. A failed export is removed rather than
left as a partial evidence tree.

This is intentionally **not** a new manifest or archive format. The proof remains
the only evidence index, and the exported directory is simply a portable
materialization of that index.

For a public campaign, use `--require-publishable` so a provisional evidence
set cannot be exported as if it had passed the publication gate.
