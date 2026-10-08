# Verify and Attest a MeshFit Benchmark Proof

This recipe composes MeshFit proof verification with GitHub Artifact
Attestations. It deliberately keeps the two trust questions separate:

1. **MeshFit verifier:** do the supplied proof and audit root recompute and pass
   the requested publication gate?
2. **Attestation layer:** who authenticated the exact proof bytes and
   verification result, under which workflow identity?

GitHub's attestation action supports custom in-toto predicates and subjects
identified directly by SHA-256 digest.

## Workflow sketch

```yaml
permissions:
  contents: read
  id-token: write
  attestations: write
  artifact-metadata: write

steps:
  - uses: actions/checkout@v6

  - id: meshfit-proof
    uses: hippoley/MeshFit/.github/actions/verify-proof@<PINNED_MESHFIT_COMMIT_SHA>
    with:
      proof: evidence/benchmark-proof.yaml
      root: evidence/benchmark-001
      require-publishable: "true"

  - name: Attest the exact verified proof
    uses: actions/attest@v4
    with:
      subject-name: benchmark-proof.yaml
      subject-digest: sha256:${{ steps.meshfit-proof.outputs.proof-sha256 }}
      predicate-type: https://github.com/hippoley/MeshFit/blob/main/docs/PROOF_CONSUMER_CONTRACT.md#benchmark-proof-verification-v1-predicate-semantics
      predicate-path: ${{ steps.meshfit-proof.outputs.verification-json }}
```

For a production workflow, pin third-party actions to full commit SHAs according
to the relying organization's GitHub Actions policy.

## Resulting trust chain

```text
materialized audit root
        ↓
MeshFit independent verifier
        ↓
exact benchmark-proof.yaml SHA-256
        + verification JSON
        ↓
GitHub / Sigstore-backed in-toto attestation
        ↓
consumer signer/workflow policy
```

A valid attestation authenticates the statement and its workflow identity. It
does not repair invalid evidence or make a non-publishable MeshFit proof
publishable.

## Why the proof digest is the subject

The subject is the exact serialized `benchmark-proof.yaml` bytes that MeshFit
verified. This prevents a later attestation from silently referring to a
different proof object with similar fields.

The verification JSON is predicate material, not a second audit root.

## Verification

A relying party can independently verify the GitHub attestation using the
GitHub CLI and its own owner/repository/predicate policy, then separately retain
or rerun the MeshFit proof verification when the materialized evidence set is
available.

This recipe is an interoperability pattern, not evidence that an external party
has adopted or endorsed MeshFit.
