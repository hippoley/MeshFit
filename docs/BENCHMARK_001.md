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

- `single-best-node` — highest relative compute among feasible single-device `SingleHost` plans;
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

The benchmark service port is part of that materialized execution contract. It defaults to `18080` for backward compatibility and can be selected explicitly:

```bash
meshfit benchmark-kit \
  cluster.yaml \
  target.yaml \
  <meshfit-plan-id> \
  <model-path> \
  model-identity.yaml \
  --listen-port 18181 \
  --write-dir benchmark-001
```

The same `listen_port` is reused by compiler preflight, generated executables, run-one/candidate/host dry-runs, host-local execution locking, and the runtime service contract. Preflight reports whether the selected port is currently bindable; an occupied port makes the candidate not ready before any GPU runtime is started. The host-local port lock remains the race-safety layer between the readiness check and actual launch.

Before running anything, inspect the experiment as a host-oriented work queue:

```bash
# all hosts and all slots
meshfit benchmark-worklist benchmark-001

# only the work assigned to one logical host
meshfit benchmark-worklist benchmark-001 --host node-b

# on the real host, use its discovered MESHFIT_NODE_ID and hide completed slots
MESHFIT_NODE_ID=node-b meshfit benchmark-worklist benchmark-001 --current-host --pending-only
```

Each slot is reported as `pending`, `locked`, `valid`, or `invalid`. A `valid` slot must parse as a BenchmarkBundle, pass bundle validation, and match the candidate plan ID. `locked` means the result-slot lock exists; MeshFit deliberately does not claim the process is alive because the lock may be stale. Every visible slot includes the exact `benchmark-run-one` command.

If the same immutable model artifact lives at a different filesystem path on a benchmark host, override only the path—not the identity:

```bash
meshfit benchmark-preflight benchmark-001 meshfit \
  --model-path /data/models/qwen.gguf

meshfit benchmark-run-one benchmark-001 meshfit 1 \
  --model-path /data/models/qwen.gguf

meshfit benchmark-run-candidate benchmark-001 meshfit \
  --model-path /data/models/qwen.gguf --resume
```

Before starting any runtime on a real machine, run one host-level readiness audit. This reuses the same candidate preflight and hardware-attestation logic as real execution, but performs no benchmark launch and writes no evidence:

```bash
meshfit benchmark-host-check benchmark-001 --host node-b

# on the real machine, prefer its current logical ID and make readiness a shell gate
MESHFIT_NODE_ID=node-b \
  meshfit benchmark-host-check benchmark-001 --current-host --require-ready
```

The report includes the current machine's discovered hardware identity plus every assigned candidate's runtime availability, host match, hardware-profile match, model verification, warnings, and preflight issues. `--require-ready` exits non-zero if any assigned candidate is not safe to run.

For any candidate spanning more than one node, readiness also requires peer-link measurement provenance for every participating node pair. The materialized snapshot must contain a `meshfit-peer-probe` record with measured RTT, measured bandwidth, and a capture timestamp. Preflight exposes each pair's source, values, measurement age, readiness, and issues. Single-host candidates do not require peer probes. MeshFit currently reports age without inventing a fixed expiration threshold; operators can see whether a topology measurement is old while the first real benchmark establishes a defensible freshness policy.

For a real host, prefer configuring the local path once instead of repeating it on every command:

```bash
export MESHFIT_MODEL_PATH=/data/models/qwen.gguf

meshfit benchmark-preflight benchmark-001 meshfit --host node-b
meshfit benchmark-run-candidate benchmark-001 meshfit --host node-b --resume

# or execute every candidate assigned to this host
meshfit benchmark-run-host benchmark-001 --host node-b --resume
```

An explicit `--model-path` always wins over `MESHFIT_MODEL_PATH`.

`MESHFIT_NODE_ID` is only a logical host selector; it is not accepted as physical proof that the operator is on the machine captured in the snapshot. Benchmark preflight reloads the materialized snapshot and compares the current machine's discovered hardware profile against `hardware_identities[benchmark_host]`. Architecture, operating system, accelerator count/vendor/model/backend/memory, and a tolerance-bounded RAM capacity are hard gates. This applies to the heterogeneous NVIDIA/AMD/Intel inventory emitted by discovery. CPU-model and driver drift are surfaced as warnings so a software update is visible without silently turning a matching accelerator host into a different placement. A missing snapshot hardware identity is also a hard failure for real Benchmark 001 execution.

A local model-path override is accepted only when the materialized model identity contains `artifact_sha256` and the host-local file hashes to exactly the same value. The path is canonicalized before compilation. Dry-run output for both one-run and candidate-level execution includes the effective path and verification state. If an executable already exists with a different model source, MeshFit recompiles it under the executable lock and replaces it with backup/restore protection. The local benchmark runner hashes the executable's model source again immediately before launch, so path relocation is verified twice while model identity remains immutable. Candidate- and host-level repeated execution performs the expensive host-local artifact preflight once per planned candidate, then reuses that verified resolved path across its run slots instead of hashing a multi-gigabyte model before every independent run. Executable cache reuse requires the expected plan ID, resolved model source, and service port to match. Changing only the benchmark listen port therefore invalidates the old executable and triggers the same safe recompilation path.

Run one assigned candidate/run directly from the materialized kit:

```bash
cd benchmark-001

# inspect the exact slot plus host/runtime/model preflight result
meshfit benchmark-run-one . meshfit 1 --dry-run

# execute on the candidate's required benchmark_host
meshfit benchmark-run-one . meshfit 1
```

The real execution path reuses `benchmark-preflight`, so a non-ready kit, wrong host, missing runtime, missing explicit local model, out-of-range run number, or accidental bundle overwrite is rejected before evidence is written. If the executable is missing it is compiled from the materialized snapshot/target first. The resulting bundle is validated and atomically renamed into its fixed result slot. A host-local execution lock in the operating-system temporary directory is scoped to MeshFit's benchmark runtime port, so real runs using different kit copies on the same machine cannot collide on the fixed port or contaminate each other's measurements. Because the lock is host-local rather than stored inside the kit, separate machines sharing a kit over NFS do not block each other. Result slots and executable compilation remain separately locked against concurrent writers.

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
- [ ] relevant links are measured with `meshfit-peer-probe` provenance, RTT, bandwidth, and capture timestamps;
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

For real benchmark hosts, prefer a stable discovery identity instead of relying on a container or cloud-instance hostname:

```bash
export MESHFIT_NODE_ID=node-b
meshfit discover > node-b.yaml

# the same identity is then reused by preflight and benchmark-run-one
meshfit benchmark-preflight benchmark-001 meshfit --require-ready
```

`MESHFIT_NODE_ID` overrides `HOSTNAME/COMPUTERNAME` for both the discovered node and local fabric endpoints. Leave it unset to use the operating-system hostname. An empty override is ignored.

For diagnostics only, if the logical snapshot node ID differs from the current operating-system identity, preflight can still be declared explicitly:

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


## Evidence overwrite safety

Benchmark bundles are evidence artifacts. MeshFit never deletes an existing bundle before a replacement is safely prepared.

Default behavior refuses replacement. With an explicit `--overwrite`, the one-run executor uses:

```text
new bundle validated
→ write temp file
→ old bundle renamed to backup
→ temp renamed into target
→ backup removed only after commit
```

If the final commit rename fails, MeshFit attempts to restore the previous bundle from backup. A stale backup blocks later replacement until it is resolved instead of being silently overwritten.

This protects real benchmark evidence from a remove-then-rename failure window.


## Host-level executor

On a real benchmark machine, the preferred operator path is one command for all candidates assigned to that logical host:

```bash
MESHFIT_NODE_ID=node-b meshfit benchmark-run-host benchmark-001 \
  --current-host \
  --model-path /data/models/qwen.gguf \
  --resume
```

Inspect the entire host plan without starting a runtime:

```bash
MESHFIT_NODE_ID=node-b meshfit benchmark-run-host benchmark-001 \
  --current-host \
  --model-path /data/models/qwen.gguf \
  --resume \
  --dry-run
```

Host execution is fail-fast before the first GPU process starts. MeshFit plans every candidate assigned to the host, runs the same host/runtime/model/evidence preflight for each one, and sets `ready: false` if any candidate is not executable. Only a fully ready host plan is executed. Runs are still performed sequentially through the existing one-run execution core, preserving local experiment locking, model SHA-256 verification, executable/source-plan checks, evidence validation, and atomic bundle commits. After execution, MeshFit re-reads the host worklist and requires every assigned slot to be valid with zero pending, locked, or invalid slots.

Use `--resume` for the normal restartable workflow: already-existing bundles are skipped only after they parse, validate, and match the expected candidate plan. `--overwrite` is explicit destructive intent and remains mutually exclusive with `--resume`.

## Candidate-level repeated-run executor

Run every independent slot for one candidate through the same validated execution core:

```bash
meshfit benchmark-run-candidate benchmark-001 meshfit --host node-b
```

Inspect the plan without starting the runtime:

```bash
meshfit benchmark-run-candidate benchmark-001 meshfit --host node-b --dry-run
```

The command pre-scans every expected bundle before GPU execution:

- default mode refuses any pre-existing target bundle before the first new run starts;
- `--resume` skips only bundles that parse, validate, and match the expected candidate plan ID;
- corrupt or mismatched existing evidence is a hard failure;
- `--overwrite` reruns every slot through the evidence-safe backup/restore commit path;
- `--resume` and `--overwrite` are mutually exclusive.

Dry-run reports preflight state, the effective model path and verification state, plus pending/existing-valid run numbers even when the host is not runtime-ready. Real execution requires preflight readiness and, after all pending slots finish, re-validates candidate completeness through `benchmark-status`. A `--model-path` supplied to candidate-level execution is reused for every pending run.

`benchmark-run-one` and `benchmark-run-candidate` share one internal execution function so locking, compilation, source-plan validation, benchmark execution, bundle validation, and evidence-safe commit semantics cannot drift apart.


## Experiment finalization

After every assigned host has completed its work, finalize the materialized experiment directory directly:

```bash
meshfit benchmark-finalize benchmark-001 --markdown
```

For a release/README gate:

```bash
meshfit benchmark-finalize benchmark-001 --markdown --require-publishable
```

Finalization is intentionally stricter than `compare-benchmarks`.

It first re-runs the materialized kit status and refuses to compare if any expected bundle is missing or invalid. Only a complete kit is allowed to load `comparison.yaml` and enter the same shared Benchmark 001 comparison path used by `compare-benchmarks`.

Therefore the end-to-end operator flow becomes:

```text
benchmark-worklist
→ benchmark-run-host on each logical host
→ benchmark-status
→ benchmark-finalize
→ publishable / provisional report
```

`--require-publishable` still enforces the existing provenance, distinct-plan, repeated-run, sample-count and stability gates. Finalization does not weaken or duplicate those rules.
