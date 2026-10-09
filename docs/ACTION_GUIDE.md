# Action Guide — Reality First, Pivot When Value Moves

_Last updated: 2026-10-09_

This guide converts the user-story audit into operating rules.

It is not a backlog. It exists to prevent MeshFit from accumulating work that
looks like progress while producing little external or real-world value.

## 1. Current mode

**Mode: framework freeze / reality + externalization.**

MeshFit already has enough internal machinery to attempt the first real closed
loop. Until Reality Campaign 001 or an external consumer exposes a concrete
defect, do not expand planner/proof scope merely because an old roadmap item is
unfinished.

## 2. What may still change before the first real proof

Only changes in these classes have priority:

### A. Evidence-integrity blocker

Examples:

- proof verifier accepts drift/tamper it should reject;
- execution identity can silently mix incompatible runs;
- campaign evidence cannot be transferred/re-attested safely;
- a persisted result can be mistaken for a different plan/runtime/model.

Action: fix immediately, regression-test, then return to Reality Campaign.

### B. Real-execution blocker

Examples:

- discovered hardware cannot be represented without manual invention;
- a supported compiled plan cannot actually launch;
- required peer evidence cannot be measured or bound to the right nodes;
- campaign binary/model/runtime identity cannot be frozen across hosts.

Action: implement only the smallest blocker fix needed to resume the real run.

### C. External-consumer blocker

Examples:

- an independent consumer cannot validate a proof because the wire contract is
  ambiguous;
- MLCommons/InfraGraph gives a stable identifier MeshFit cannot consume without
  guessing;
- an external reviewer requests a concrete compatibility/conformance artifact.

Action: implement the narrow compatibility surface requested by reality.

Everything else is lower priority.

## 3. Current PR closure order

Only **#139** remains as an internal closure item before framework freeze.

Already resolved:

- **#133 merged** — MLCommons/InfraGraph planning seed with fail-conservative
  runtime/network handling and explicit node-cost ownership;
- **#134 merged** — portable verification JSON, public schema,
  digest/predicate binding and standard attestation composition;
- **#135 merged** — Evidence Index factual refresh;
- **#140 merged** — portable self-contained proof materialization and
  re-verification from current main;
- **#132 closed as superseded by #140** after verifier/mainline merge-base drift;
- **#138 closed not planned** — no dedicated proof-consumer issue form until a
  real external consumer demonstrates that normal GitHub issues are inadequate.

After #139 passes CI, merge it and stop opening internal PRs unless a real run or
independent consumer exposes a concrete integrity/execution/interop blocker.

## 4. Reality Campaign 001 is the product gate

After the open PRs above are closed, the default action is **not another PR**.

The next product-state transition is:

```text
real heterogeneous hosts
  -> retained discovery
  -> measured compute ordering
  -> measured peer links
  -> >=3 distinct executable placements
  -> frozen kit / binary / model
  -> real repeated BenchmarkBundles
  -> finalize
  -> benchmark-proof.yaml
  -> independent verification
  -> publishable or explicit negative result
```

A negative result is acceptable. Rewriting the framework until MeshFit wins is
not.

## 5. User-story closure rules

### Close by reality, not code

These stories must not receive more architecture work unless a real run exposes
a specific blocker:

- heterogeneous discovery;
- real network evidence;
- compute ordering;
- runtime launch;
- GPU resource observation;
- repeated BenchmarkBundles;
- first headline / negative result.

### Close by external behavior

These cannot be self-certified inside this repository:

- proof/action/schema adoption;
- MLCommons semantic ownership;
- third-party citation/reproduction;
- institutional reputation.

Required evidence is an independent repo/workflow, upstream maintainer response,
accepted contribution, reproduction/rejection, or working-group record.

### Explicitly deferred

Do not resume solely to complete the original vision:

- generic PP/EP/replica planning;
- generic multi-node runtime orchestration;
- storage/failure-domain optimization;
- general cross-hardware latency/throughput prediction;
- continuous telemetry;
- automatic re-placement.

Re-open only when a real operator/consumer demonstrates material decision value.

## 6. Node valuation

Treat MeshFit as an implementation/evidence vehicle, not the destination.

Revalue candidate nodes on:

```text
external maintainer/governance activity
× unresolved technical white space
× reuse of existing MeshFit evidence assets
× probability of third-party review/adoption
× 5–10 year institutional portability
÷ validation cost
```

### Current allocation

**Product reality gate: MeshFit Reality Campaign #82**

No internal feature substitutes for authenticated live capacity, real
heterogeneous hosts, real repeated bundles and the first verified positive or
negative claim.

**Highest near-term feedback node: MLPerf Endpoints submission/review tooling**

Why:

- v1.0 rolling submission begins 2026-10-12;
- the official runner -> submission CLI seam is already documented in
  `mlcommons/endpoints-submission-cli#77`;
- real submissions force runner, packaging, checker, reproducibility and review
  semantics to meet;
- a small fail-closed handoff/conformance contribution would create more
  externally reviewable identity capital than another MeshFit-internal feature.

Current limitation: the connected GitHub integration cannot write to the
upstream repository (403), and no `hippoley/endpoints-submission-cli` fork is
available through the current connection. Therefore this is a high-value node,
not yet an achieved contribution.

**Architectural/governance node: MLCommons Benchmark Infra / InfraGraph**

Why:

- reproducibility and system-spec ownership are explicit;
- active #1088 work provides a real multi-node infrastructure boundary;
- MeshFit can compose decision/execution evidence without duplicating inventory;
- accepted contribution/review would be a durable institutional credential.

**High-upside standards option: IETF CATS / CATPTS**

CATPTS remains a cheap option around placement commitment -> realized execution
assurance. Do not make it primary while Endpoints has a live submission window
and no CATPTS author/WG pull exists.

## 7. Pivot triggers

### Double down on a node when

- a maintainer requests a PR/example/conformance vector;
- an independent consumer starts using/rejecting a MeshFit artifact;
- a WG discussion asks exactly the placement/evidence question MeshFit can
  answer;
- a real campaign produces a result that external reviewers want to inspect.

### Hold as an option when

- the technical white space is attractive;
- the external project is actively moving;
- but nobody has yet requested MeshFit-specific work.

### Pivot away when

- multiple cycles produce only self-authored commits/issues/docs;
- another project/standard fully owns the same boundary;
- the next internal feature has lower expected institutional value than an
  adjacent external contribution;
- real evidence shows the problem is not decision-relevant.

### Kill when

- the feature exists mainly because it was once in the roadmap;
- a stronger planner/model would make it irrelevant and it leaves no durable
  evidence/contract;
- no real operator, consumer, or external reviewer can be named.

## 8. The durable credential

The 6–12 month credential is not “MeshFit has many features.”

It is:

> hippoley built a falsifiable placement-decision evidence chain, exercised it
> against real heterogeneous infrastructure, exposed independent verification
> surfaces, and carried the methodology into external benchmark/standards
> review.

The 5–10 year credential is stronger only if outside institutions leave a record
of relying on that judgment.
