# Benchmark Proof Attestation Envelope

MeshFit's `benchmark-proof.yaml` is an **audit root**: it binds frozen inputs,
candidate plans, raw benchmark bundles, executable/source identity and the final
comparison result.

`meshfit benchmark-verify-proof` verifies that this decision → execution →
claim chain is internally consistent.

That is necessary, but it is not an authenticity boundary by itself.

A third party must also be able to answer:

> Who attested to this exact proof receipt, under which identity, and can I
> verify that attestation without trusting the MeshFit planner?

This document defines the minimal external envelope for that purpose.

## Design rule

Do not duplicate BenchmarkBundle or BenchmarkProofReceipt fields into a second
MeshFit-specific provenance format.

The subject of the attestation is the exact serialized
`benchmark-proof.yaml` bytes.

At minimum an external envelope must bind:

- SHA-256 of `benchmark-proof.yaml`;
- a stable subject name;
- an authenticated signer/verifier identity;
- an attestation predicate type;
- enough issuer/workflow identity for a relying party to apply policy.

The proof receipt remains authoritative for campaign evidence. The envelope only
adds an external trust root.

## Interoperability profile

The preferred transport is an **in-toto Statement v1** or another ecosystem
attestation format capable of binding the proof digest to an authenticated
identity.

Conceptual shape:

```json
{
  "_type": "https://in-toto.io/Statement/v1",
  "subject": [{
    "name": "benchmark-proof.yaml",
    "digest": {
      "sha256": "<sha256 of exact proof bytes>"
    }
  }],
  "predicateType": "https://meshfit.dev/attestations/benchmark-proof/v1",
  "predicate": {
    "schema": "meshfit.benchmark-proof/v1",
    "benchmark_id": "<id>",
    "evidence_publishable": true
  }
}
```

The predicate is intentionally small. A verifier must dereference the subject
bytes and run MeshFit's proof verification (or an independent implementation)
against the materialized audit root.

## Verification policy

An external consumer SHOULD fail closed unless all of the following hold:

1. the attested subject digest equals the exact proof receipt bytes;
2. the signer identity matches an explicitly trusted issuer/workflow policy;
3. the receipt schema is `meshfit.benchmark-proof/v1`;
4. the receipt validates against
   `schemas/benchmark-proof-v1.schema.json`;
5. every referenced input and bundle hash verifies;
6. candidate plan/bundle identities agree;
7. source and executable identity constraints agree;
8. the comparison report can be recomputed from the retained bundles;
9. any required publishability gate passes.

A valid signature does **not** make a benchmark claim true. It authenticates who
made the attestation about a specific proof object. The underlying evidence must
still verify.

## Why this is separate from planner intelligence

An optimizer, heuristic, human or LLM can all produce candidate placements.

The attestation boundary is downstream of that choice:

```text
decision producer
      ↓
precommitted plan
      ↓
real execution evidence
      ↓
benchmark-proof.yaml
      ↓
independent proof verification
      ↓
authenticated attestation envelope
      ↓
consumer policy
```

A better model can replace the decision producer without invalidating the trust
contract.

## Current implementation status

Implemented today:

- provenance-bound benchmark bundles;
- frozen campaign inputs;
- `benchmark-proof.yaml` audit root;
- independent proof verification;
- JSON Schema for the public receipt shape.

Not yet claimed:

- a MeshFit-operated signing authority;
- a new cryptographic scheme;
- a custom transparency log;
- equivalence with SLSA levels.

The intended next integration is to use an existing attestation ecosystem
(Sigstore/GitHub Artifact Attestations/in-toto-compatible tooling) rather than
inventing a MeshFit-only signer.
