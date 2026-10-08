# Action Guide — Reality First, Pivot When Value Moves

_Last updated: 2026-10-08_

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

The open implementation work is already sufficient. Close it before opening new
framework lines:

1. **#132** — portable proof package: source verify -> dependency materialize ->
   exported-tree re-verify.
2. **#133** — MLCommons/InfraGraph planning seed with unknown evidence preserved
   as unknown.
3. **#138** — proof-consumer/interoperability intake surface.
4. **#139** — user-story truth audit / roadmap correction.

#134 is already merged: portable verification JSON, public schema, digest/predicate
binding, and standard attestation composition are now mainline code/contract
capabilities.

Merge only after the relevant CI succeeds. A mergeable PR with queued or failed
CI is not completed evidence.

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

**Primary institutional node: MLCommons Benchmark Infra**

Why:

- reproducibility and system-spec ownership are explicit;
- logging/reporting and benchmark documentation are explicit deliverables;
- MeshFit evidence can be projected into an externally governed environment;
- accepted contribution/review would produce stronger identity capital than
  additional self-authored framework work.

**High-upside option: IETF CATS / CATPTS**

Why:

- active work on heterogeneous compute-aware task placement;
- explicit placement lifecycle / decision-epoch concepts;
- potential white space around portable placement commitment -> realized
  execution assurance.

Caveat: CATPTS is an active individual Internet-Draft, not an adopted standard.
Keep the option cheap until authors/WG discussion creates pull.

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
