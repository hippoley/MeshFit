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

## Relationship to standardized placement decisions

Kubernetes KEP-5313 is a useful boundary test because it proposes a
vendor-neutral `PlacementDecision` API for the resolved "which clusters" answer
while deliberately leaving scheduling logic and downstream deployment behavior
out of scope. As of 2026-10-08 its KEP metadata is still `status: provisional`
and `stage: alpha`, so MeshFit treats it as an emerging interface boundary, not
a stable dependency.

MeshFit should treat that direction as complementary rather than inventing
another general placement-decision API.

A future composition can look like:

```text
vendor scheduler / LLM / placement controller
        ↓
standard PlacementDecision
        ↓
decision correlation / frozen digest
        ↓
actual deployment and physical execution
        ↓
MeshFit execution-bound placement proof
```

The long-lived MeshFit question begins **after** a decision object exists:

> Did the later execution and benchmark claim actually realize the frozen
> placement decision under the physical conditions claimed?

This also distinguishes MeshFit from trusted-placement architectures that attest
a node before admitting a workload. Pre-placement platform trust is valuable,
but it is not the same as binding a later performance/cost claim to the exact
decision and execution evidence.

No Kubernetes API integration is claimed today. In particular, MeshFit should
not change `meshfit.benchmark-proof/v1` merely to mirror a still-evolving
placement API. A standard placement object can become an optional precommit
input only when there is a real consumer/use case and a stable correlation
boundary.

KEP-5313 also keeps **consumer feedback** out of the read-only
`PlacementDecision` object and points feedback toward a separate channel such
as events, metrics, or a purpose-built feedback API. That ownership rule is a
good fit for placement proof:

```text
PlacementDecision (scheduler-owned, read-only to consumers)
        ↓
deployment / execution
        ↓
separate execution-proof / conformance evidence
        ↓
review, policy, or optional feedback channel
```

MeshFit should therefore never require a consumer to mutate the placement
decision in order to attach proof. Correlation should be by stable decision /
placement identity or digest, while proof remains a separately owned evidence
object.

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
