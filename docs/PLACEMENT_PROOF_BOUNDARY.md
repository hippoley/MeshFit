# Placement Proof Boundary

_Status: working category boundary, not a standards claim_

MeshFit should not compete for the broad category "decision provenance".

That category already includes systems for human/AI decision records, agent
conversation provenance, compliance records, runtime execution graphs, and
generic cryptographic attestation.

MeshFit's narrower durable primitive is:

> **execution-bound placement proof** — binding a placement decision made before
> measurement to the exact physical execution and evidence later used to support
> an infrastructure performance/cost claim.

## The object being proved

A placement proof binds:

```text
frozen workload + observed infrastructure
        ↓
precommitted placement decision
        ↓
exact source / executable / runtime identity
        ↓
physical host + topology + network evidence
        ↓
repeated execution identity
        ↓
raw benchmark bundles
        ↓
recomputed comparison
        ↓
claim / publication gate
```

The proof is useful even when the placement producer changes from a heuristic to
an optimizer, LLM, infrastructure agent, or human.

## What makes this narrower than generic provenance

Generic provenance can answer:

- who or what made a decision;
- which inputs or policies influenced it;
- which actions occurred;
- whether a record is authentic.

Placement proof additionally asks domain-specific questions:

- was the candidate placement frozen before its performance was known?
- was the observed infrastructure the infrastructure the planner actually saw?
- did the candidate execute on the host/topology it claimed?
- were cross-host link measurements real and fresh enough for that placement?
- did repeated runs preserve the same execution identity?
- do the raw bundles recompute to the published comparison?
- does the evidence set satisfy the explicit publication gate?

Those are infrastructure-placement conformance questions, not a replacement for
a general provenance format.

## Relationship to adjacent layers

MeshFit should compose with, not replace:

- **in-toto / Sigstore / GitHub Artifact Attestations** for authenticated
  attestation and relying-party identity policy;
- **generic decision-provenance systems** for organizational or human/AI
  decision records;
- **runtime agent-provenance systems** for process/tool/file/network execution
  telemetry;
- **infrastructure control/authorization protocols** for permission to execute;
- **MLPerf / benchmark policy** for externally governed benchmark rules.

The MeshFit-specific boundary begins where an infrastructure placement becomes a
falsifiable claim about what should run where, and ends with an independently
recomputable evidence/claim result.

## Anti-substitution test

A proposed feature belongs in the durable layer only when replacing the planner
with a stronger model would **not** make the feature unnecessary.

Durable examples:

- placement precommit;
- physical evidence capture/freshness;
- execution identity;
- evidence hashes;
- independent recomputation;
- fail-closed publication;
- consumer verification interfaces.

Replaceable examples:

- one ranking heuristic;
- one LLM prompt for node selection;
- one provider-specific recommendation UI;
- planner-only explanations with no evidence binding.

## External validation threshold

This category boundary becomes more than a self-description only when an
independent consumer does at least one of the following:

- runs the proof verifier in its own CI;
- consumes the proof digest in an attestation or policy decision;
- independently reproduces a campaign proof;
- cites the placement-proof contract in an external issue, paper, benchmark, or
  implementation;
- requests a compatibility adapter or conformance test.

Until then, "execution-bound placement proof" is a working positioning boundary,
not an established external category.
