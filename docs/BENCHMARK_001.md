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

Create a comparison manifest:

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
```

The comparison rejects mismatched model identity, context, concurrency, or any BenchmarkConfig field. A result is only marked publishable when each candidate has at least 2 independent bundles, at least 20 measured samples, unique benchmark IDs, runner provenance, and capture timestamps.

## Output contract

The report contains, per candidate:

- bundle and sample counts;
- p50 and p95 TTFT;
- mean total latency;
- mean decode tok/s when token usage is available;
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
