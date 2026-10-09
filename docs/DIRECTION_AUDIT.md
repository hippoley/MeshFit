# Direction Audit

This document is the guardrail for MeshFit.

Every milestone and major feature should be checked against the original product goal:

> Given a real heterogeneous compute estate, automatically discover and model the infrastructure, determine feasible model placements, produce comparable executable deployment plans, explicitly exclude harmful resources, and calibrate predictions with real benchmarks.

## Five direction checks

### 1. Reality contact

Does this feature increase contact with real hardware, runtimes, links, costs, workloads, or benchmark evidence?

Good:

- device discovery
- runtime version capture
- measured RTT / bandwidth
- executable launch plans
- observed TTFT / tok/s
- prediction error

Weak:

- another abstract score with no evidence source
- another README promise
- another untested backend flag

### 2. Input automation

Does this reduce manually authored infrastructure/model/runtime facts?

The long-term product cannot depend on users hand-writing the world into YAML.

Manual YAML is acceptable for schema development, not as the final interaction model.

### 3. Distance to execution

Does this move a placement decision closer to something a runtime can actually execute?

Preferred direction:

~~~text
idea
→ typed PlanIR
→ runtime-specific config
→ launch
→ benchmark
→ evidence
~~~

Avoid accumulating recommendation text that cannot be compiled or verified.

### 4. Evidence discipline

Does every quantitative claim expose:

- provenance
- execution identity
- sample count
- uncertainty
- transfer distance, if extrapolated

If not, return Unavailable rather than inventing precision.

### 5. Layer discipline

MeshFit still performs placement intelligence, but its harder-to-substitute
boundary is **placement-claim conformance**: observed infrastructure ->
precommitted placement -> physical execution -> evidence -> independently
reviewable claim.

Planner intelligence is replaceable by a stronger optimizer, LLM, agent, or
human. Evidence/conformance work is preferred when the two compete for effort.

It should not become:

- a tensor kernel library
- a new inference engine
- a Kubernetes replacement
- a Ray replacement
- a model hosting platform

It may compile plans **to** these systems and learn from their telemetry.

## Milestone health

### v0.1 Structural Placement

Health: **on direction**

Why:

- establishes typed placement space;
- rejects impossible plans;
- represents fabric, runtime, model and workload separately;
- introduces Pareto reasoning instead of one arbitrary score.

Risk:

- cross-node thresholds are currently conservative constants, not calibrated evidence.

Required follow-up:

- replace hard-coded gates with runtime/topology capability evidence over time.

### v0.2 Evidence Identity

Health: **on direction**

Why:

- prevents benchmark contamination;
- makes calibration possible;
- explicitly returns Unavailable when evidence is absent.

Risk:

- example evidence is synthetic;
- no real artifact/runtime/driver capture yet.

Required follow-up:

- one real benchmark bundle.

### v0.2.5 Discovery

Health: **critical path**

This milestone was moved earlier.

Without it, MeshFit remains a hand-authored offline planner and misses the original user story.

### v0.3 Plan Compiler + Benchmark

Health: **critical Reality Gate**

This is the first point where MeshFit can prove that a placement decision becomes real behavior.

### v0.4 Prediction

Health: **must remain downstream of evidence**

Do not build a sophisticated predictor before executable plans and real benchmark bundles exist.

## Kill / defer rules

Defer a feature when:

- it needs a runtime capability we have not verified;
- it produces a precise number with no evidence path;
- it adds a backend before the first two backends close the full loop;
- it duplicates execution work already owned by vLLM/SGLang/llama.cpp/exo/Ray;
- it does not improve placement quality, evidence quality, execution quality, or external consumability;
- it exists mainly to complete an old roadmap checkbox after the product boundary has moved;
- a real campaign or external consumer has not demonstrated the need for it.

## Pivot / valuation rule

Long-termism is not a commitment to one repository or one roadmap.

Revalue the next action when an adjacent node offers a materially better
combination of:

- active external maintainers / governance;
- unresolved technical white space;
- reuse of MeshFit's existing evidence assets;
- probability of third-party review/adoption;
- 5–10 year institutional portability;
- low cost to test the hypothesis.

MeshFit may become the implementation/evidence vehicle for a higher-value node
such as MLCommons Benchmark Infra or an emerging placement-assurance standard.
Do not preserve low-value internal scope for sunk-cost reasons.

## North-star Reality Gate

The project has reached its first meaningful product proof when:

1. two or more real machines are discovered automatically;
2. their link is measured;
3. one real model is inspected;
4. MeshFit generates multiple placement candidates;
5. at least one node is excluded for an evidence-backed reason;
6. one generated plan launches;
7. benchmark evidence is captured automatically;
8. predicted/expected and observed outcomes can be compared.

Until then, feature breadth is secondary.
