# MeshFit Verify Proof Action

This composite action is the machine-consumption surface for MeshFit benchmark
proofs.

It lets another repository fail its CI when a supplied
`meshfit.benchmark-proof/v1` receipt does not verify against the materialized
audit root that it references.

## Consumer example

Pin the action to a commit SHA:

```yaml
jobs:
  verify-meshfit-proof:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6

      - id: meshfit-proof
        uses: hippoley/MeshFit/.github/actions/verify-proof@<MESHFIT_COMMIT_SHA>
        with:
          proof: evidence/benchmark-proof.yaml
          root: evidence/benchmark-001
          require-publishable: "true"

      - run: echo "verified proof ${{ steps.meshfit-proof.outputs.proof-sha256 }}"
```

The action exposes the SHA-256 of the exact proof bytes that passed verification
and the path to a machine-readable verification JSON document, so a downstream
system can bind an attestation, review record, release note, or policy decision
to that immutable subject.

For a standard GitHub/Sigstore-backed composition, see
[Verify and Attest a MeshFit Benchmark Proof](../../../docs/VERIFY_AND_ATTEST_PROOF.md).

## Trust boundary

This is a thin packaging layer over MeshFit's existing independent verifier:

```bash
meshfit benchmark-verify-proof benchmark-proof.yaml \
  --root benchmark-001 \
  --require-publishable
```

It does not ask the planner to generate a new placement.

It is also not a signature verifier and does not replace Sigstore, in-toto,
GitHub Artifact Attestations, or the relying party's own policy.

The intended composition is:

```text
MeshFit proof verification
        ↓
exact proof SHA-256
        ↓
standard attestation envelope
        ↓
consumer policy
```

A valid signature authenticates an attester; it does not make an invalid
benchmark claim true.

## Runner requirement

The action builds and executes the verifier from the exact MeshFit revision named
in `uses:`. A Rust/Cargo toolchain, Bash, and Python 3 must be available on the
runner. The bundled examples and contract tests target GitHub-hosted Ubuntu.

For external use, pin `uses:` to a full commit SHA rather than a mutable branch.
