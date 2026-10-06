# Benchmark 001 — first defensible MeshFit number

Benchmark 001 exists to answer one question:

> Does MeshFit choose a better placement than simple baselines on the same real workload?

It is deliberately narrow. Do not expand runtime or hardware coverage until this gate produces a real result.

## Frozen comparison

Use the same:

- model artifact identity;
- prompt;
- context length;
- max output tokens;
- concurrency;
- benchmark runner;
- measured topology.

Compare at least:

1. `single-best-node`
2. `max-aggregate-compute`
3. `meshfit`

Run each candidate repeatedly. Keep the raw `BenchmarkBundle` files.

## Primary number

The first primary objective is `p95_ttft_ms`.

MeshFit computes the observed oracle from all candidates and reports placement regret:

```text
lower-is-better:
regret = (candidate - oracle) / oracle

higher-is-better:
regret = (oracle - candidate) / oracle
```

The headline number is:

```text
regret reduction vs best baseline
```

This is only publishable when it comes from real benchmark bundles. Synthetic fixtures may test code paths but must never be presented as product evidence.

## CLI

Start from a discovered snapshot and a placement target. The MeshFit-selected plan stays explicit; the CLI only derives the two naive baselines:

```bash
meshfit benchmark-candidates cluster.yaml target.yaml <meshfit-plan-id>

# hard gate: exit non-zero if the three strategies collapse onto fewer than
# three distinct plan IDs
meshfit benchmark-candidates cluster.yaml target.yaml <meshfit-plan-id> --require-distinct
```

This emits:

- `single-best-node` — highest relative compute among feasible single-node plans;
- `max-aggregate-compute` — highest relative compute among all structurally feasible plans;
- `meshfit` — the exact plan ID you supplied.

If these resolve to overlapping plan IDs, the output carries a warning. That cluster/workload cannot support a publishable three-strategy Benchmark 001 without choosing a different discriminating setup.

You can also generate an auditable execution kit:

```bash
meshfit benchmark-kit \
  cluster.yaml \
  target.yaml \
  <meshfit-plan-id> \
  <model-path> \
  model-identity.yaml > benchmark-kit.yaml

# hard gate: fail unless all three candidates are distinct and compiler-ready
meshfit benchmark-kit \
  cluster.yaml \
  target.yaml \
  <meshfit-plan-id> \
  <model-path> \
  model-identity.yaml \
  --require-ready > benchmark-kit.yaml
```

The kit fixes:

- two independent runs per candidate;
- at least 10 measured requests per run, rounded to a full concurrency wave;
- `results/<strategy>/run-01.yaml` and `run-02.yaml`;
- executable artifact paths;
- compile commands;
- the exact `benchmark_host` where each runtime must execute;
- benchmark commands;
- the final comparison-manifest skeleton.

The kit exposes a top-level `ready` boolean. Every candidate is compiler-preflighted before a command is emitted. To materialize a self-contained experiment control directory (without copying the model weights), use:

```bash
meshfit benchmark-kit \
  cluster.yaml \
  target.yaml \
  <meshfit-plan-id> \
  <model-path> \
  model-identity.yaml \
  --write-dir benchmark-001
```

This writes `kit.yaml`, `comparison.yaml`, `RUNBOOK.md`, an `inputs/` snapshot of the small control files, and per-strategy `artifacts/` + `results/` directories. Commands in the materialized kit are rewritten to run from that directory. Local model paths are canonicalized instead of copying large weights; model hub identifiers are preserved as-is.

Run one assigned candidate/run directly from the materialized kit:

```bash
cd benchmark-001

# inspect the exact slot plus host/runtime/model preflight result
meshfit benchmark-run-one . meshfit 1 --dry-run

# execute on the candidate's required benchmark_host
meshfit benchmark-run-one . meshfit 1
```

The real execution path reuses `benchmark-preflight`, so a non-ready kit, wrong host, missing runtime, missing explicit local model, out-of-range run number, or accidental bundle overwrite is rejected before evidence is written. If the executable is missing it is compiled from the materialized snapshot/target first. The resulting bundle is validated and atomically renamed into its fixed result slot. A local experiment lock serializes real runs from the same kit on one host, preventing fixed-port conflicts and benchmark interference; result slots and first-time executable compilation are separately locked against concurrent writers.

During real execution, inspect progress at any time:

```bash
meshfit benchmark-status benchmark-001

# final gate before comparison
meshfit benchmark-status benchmark-001 --require-complete
```

The status command reports each candidate's `benchmark_host`, expected/valid run counts, missing bundle paths, and invalid bundles. Existing bundle files are parsed and validated, and their `source_plan_id` must match the candidate plan; a merely present file does not count as a completed run.

Every candidate is compiler-preflighted before a command is emitted. Unsupported plans are marked `compile_ready: false` with the compiler error instead of receiving a command that is known to fail. The kit also validates that the model identity matches the target model and carries an artifact hash or revision.

Then compile and execute each selected plan with the same model artifact and BenchmarkConfig, retaining every raw bundle. `benchmark-auto` defaults to at least 10 measured requests per run, so two independent runs can satisfy the 20-sample publication floor. Override with `--measured-requests N` when needed.

Finally create a comparison manifest:


```yaml
benchmark_id: benchmark-001
meshfit_candidate: meshfit
objective: p95_ttft_ms

candidates:
  - name: single-best-node
    strategy: strongest feasible single node
    hourly_cost_usd: 0.0
    bundles:
      - results/single-best-node/run-01.yaml
      - results/single-best-node/run-02.yaml

  - name: max-aggregate-compute
    strategy: use the largest feasible aggregate compute placement
    hourly_cost_usd: 6.74
    bundles:
      - results/max-aggregate-compute/run-01.yaml
      - results/max-aggregate-compute/run-02.yaml

  - name: meshfit
    strategy: topology/runtime/memory/cost-aware MeshFit placement
    hourly_cost_usd: 1.80
    bundles:
      - results/meshfit/run-01.yaml
      - results/meshfit/run-02.yaml
```

Then run:

```bash
meshfit compare-benchmarks benchmark-001.yaml

# README-ready table and headline
meshfit compare-benchmarks benchmark-001.yaml --markdown

# release/CI gate: exit non-zero unless the evidence is publishable
meshfit compare-benchmarks benchmark-001.yaml --require-publishable
```

The comparison rejects mismatched model identity, context, concurrency, or any BenchmarkConfig field. A result is only marked publishable when each candidate has at least 2 independent bundles and 20 measured samples, every strategy maps to one stable plan ID, plan IDs are distinct across strategies, benchmark IDs are unique, runner provenance includes the MeshFit source commit, and capture timestamps are present.

## Output contract

The report contains, per candidate:

- bundle and sample counts;
- p50 and p95 TTFT;
- mean total latency;
- mean per-request decode tok/s when token usage is available;
- mean aggregate wave throughput tok/s when wave usage is available;
- peak observed RAM and VRAM;
- optional estimated cost per 1M output tokens from aggregate wave output divided by measured wave wall-clock time;
- objective value;
- regret against the observed oracle.

The report also contains:

- oracle candidate;
- MeshFit objective value;
- MeshFit regret;
- best baseline regret;
- regret reduction vs the best baseline.

## Reality gate

The intended execution chain is:

```text
discover / snapshot
        ↓
plan-snapshot
        ↓
benchmark-candidates
        ↓
compile each selected plan
        ↓
benchmark-auto / benchmark-local
        ↓
retain raw bundles
        ↓
compare-benchmarks
        ↓
--require-publishable
        ↓
README headline
```

Benchmark 001 is complete only when:

- [ ] at least 3 real heterogeneous machines are discovered;
- [ ] relevant links are measured;
- [ ] one immutable model artifact is used by every candidate;
- [ ] all 3 strategies above are executable;
- [ ] every candidate has at least 2 independent real runs and at least 20 measured samples;
- [ ] raw bundles are retained;
- [ ] `compare-benchmarks` produces the comparison report;
- [ ] the README publishes the real number with provenance;
- [ ] at least one harmful or inferior resource/placement is explainable from evidence.

Until these boxes are checked, MeshFit must say **result pending**, not invent a benchmark claim.


## Run-to-run stability

A Benchmark 001 comparison is not publishable merely because it has enough samples.

For each candidate, MeshFit derives one objective value per independent benchmark bundle and computes the run-level coefficient of variation:

```text
run_objective_cv = sample_stddev(run_objective_values) / abs(mean(run_objective_values))
```

The current publication contract requires:

- at least 2 independent benchmark bundles per candidate,
- at least 20 measured request samples per candidate,
- unique benchmark IDs and MeshFit source provenance,
- identical model/workload/benchmark configuration across candidates,
- **run-level objective CV <= 20%** for every candidate.

The stability metric follows the selected comparison objective:

- `p95_ttft_ms` -> p95 TTFT per run,
- `mean_decode_tokens_per_second` -> mean decode rate per run,
- `mean_total_ms` -> mean total request time per run.

Results above the 20% CV limit remain available as provisional diagnostics but must not be labeled PUBLISHABLE or used as a MeshFit performance claim.

Comparison output also reports TTFT standard deviation/CV, decode-rate standard deviation, and run-level objective CV.


## Host preflight

Before compiling or spending GPU time on one candidate, run the preflight on the host that will execute it:

```bash
meshfit benchmark-preflight benchmark-001 meshfit --require-ready
```

If the logical snapshot node ID differs from the operating-system hostname, declare it explicitly:

```bash
meshfit benchmark-preflight benchmark-001 meshfit --host node-b --require-ready
```

The preflight verifies:

- the execution kit itself is ready;
- the selected candidate is compiler-ready;
- the current/declarative host matches `benchmark_host`;
- the candidate's required runtime is discoverable on `PATH`;
- an explicit local model path exists;
- existing result bundles will not be overwritten accidentally.

Hub/runtime-resolved model identifiers are reported as `runtime_resolved_unverified`, not falsely treated as downloaded artifacts.

Existing result bundles are a hard failure by default. Use `--allow-existing` only when intentionally resuming an experiment without overwriting those files.

A normal preflight always emits the diagnostic report. `--require-ready` turns any hard issue into a non-zero exit code for scripts and runbooks.


The `--host` option follows the same logical-node contract as `benchmark-preflight`: use it when the snapshot node ID intentionally differs from the operating-system hostname. Real execution still requires runtime/model readiness; `--host` does not bypass those checks.
