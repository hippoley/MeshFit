<div align="center">

# MeshFit

### Placement intelligence for heterogeneous inference.

Give MeshFit the machines you actually have.  
It searches **where the model belongs, how it should be split, what it will cost, and what should be left out.**

<br>

**The unit of planning is not a GPU. It is a graph.**

</div>

---

## The problem

A real AI fleet does not look like a benchmark table.

It looks like this:

~~~text
                         ┌──────────── AWS / 2× H100
                         │             48 ms RTT
                         │
2× RTX 4090 ── 10GbE ───┼──────────── A100 80GB / Azure
on-prem                  │             22 ms RTT
                         │
                         └──────────── Mac Studio 128GB
                                       1GbE / Metal
~~~

Now pick a 70B model.

Adding memory is easy.

Deciding whether those machines should **cooperate** is not.

---

## Target UX

The interaction below is the product target, not the current pre-alpha CLI.

~~~text
meshfit plan infra.yaml \
  --model qwen3-72b \
  --context 32k \
  --concurrency 20 \
  --p95 2s \
  --budget '$200/day'
~~~

~~~text
01  LATENCY

    aws-h100-a + aws-h100-b
    vLLM · TP=2 · BF16 · 32K

    p95 TTFT       1.31 s
    decode         69 tok/s
    throughput     1,360 tok/s
    cost           $6.74/h
    bottleneck     HBM
    confidence     0.84


02  COST

    local-4090-a + local-4090-b
    llama.cpp · Q4_K_M · partial RAM offload

    decode         23 tok/s
    cost           sunk
    bottleneck     PCIe / memory bandwidth
    confidence     0.71


—   EXCLUDE

    office-mac

    reason
    WAN + backend mismatch costs more than the added compute saves.

    better use
    replica · batch worker · failover
~~~

The last answer matters as much as the first two.

> **A machine can have compute and still be negative compute for a given plan.**

---

## The model

MeshFit evaluates two graphs.

~~~text
INFRASTRUCTURE GRAPH                    MODEL GRAPH

node                                    layers
├─ CPU / RAM                            ├─ weights
├─ GPU / NPU                            ├─ attention
├─ free memory                          ├─ KV cache
├─ runtime support                      ├─ experts
└─ cost                                 └─ communication

edge
├─ PCIe / NVLink
├─ Ethernet / InfiniBand
├─ WAN
├─ bandwidth
├─ latency / jitter
└─ egress
~~~

Then it searches for a mapping:

~~~text
model graph
     ↓
candidate placements
     ↓
single host / TP / PP / EP / replicas / RPC / offload
     ↓
latency × throughput × memory × network × cost
     ↓
ranked deployment plans
~~~

This is the project.

---

## The constraint most tools miss

### Aggregate memory is not usable memory.

~~~text
80GB H100 + 80GB A100 + 24GB 4090 + 128GB Apple Unified Memory
≠
312GB of useful model memory
~~~

Usability depends on:

`backend × topology × runtime × communication × context × workload`

A slow link can erase a fast GPU.

A mixed backend can make a shard impossible.

A cheap cloud node can become expensive once egress enters the loop.

A larger cluster can be slower than a smaller one.

MeshFit should be able to say **no**.

---

## Three jobs

| | Job | Question |
|---|---|---|
| **Fit** | capacity reasoning | Can it run? |
| **Place** | topology search | Where should it run? |
| **Verify** | benchmark calibration | Was the prediction right? |

Most existing tools are excellent at one of these.

MeshFit is trying to connect all three.

### A placement is a claim, not just a recommendation

For real-hardware evaluation, MeshFit treats a placement decision as something
that should be reproducible and falsifiable. A publishable comparison binds the
pre-benchmark plan, workload/request inputs, model artifact, hardware/topology,
runtime identity, MeshFit source/binary identity, measured peer evidence, and
repeated-run execution identity.

That boundary is documented in the
[Trace-to-Placement Contract](docs/TRACE_TO_PLACEMENT_CONTRACT.md), with
[Reality Campaign 001](docs/REALITY_CAMPAIGN_001.md) as the first concrete
conformance case. For the shortest reviewer path from claims to exact public
commits, current Reality status, and external-validation gaps, see the
[Decision-Provenance Evidence Index](docs/EVIDENCE_INDEX.md).

### Inspect the proof, not just the README

The [Proof Explorer](web/proof-explorer/) is the reviewer-facing interaction
surface for `meshfit.benchmark-proof/v1`. It walks a receipt through the
precommitted decision, frozen inputs, execution identity, raw evidence, claim
gate, and independent verification boundary.

The Explorer never upgrades evidence to truth. A publishable result still has to
pass:

```bash
meshfit benchmark-verify-proof benchmark-proof.yaml \
  --root <materialized-kit> \
  --require-publishable
```

Until Reality Campaign 001 produces retained real BenchmarkBundles, the bundled
Explorer example remains explicitly non-publishable.

### Use MeshFit as a CI gate

The reusable verifier action turns the proof contract into a machine-consumption
surface:

```yaml
- id: meshfit-proof
  uses: hippoley/MeshFit/.github/actions/verify-proof@<PINNED_COMMIT_SHA>
  with:
    proof: evidence/benchmark-proof.yaml
    root: evidence/benchmark-001
    require-publishable: "true"
```

The action fails closed on an invalid proof and exports the SHA-256 of the exact
verified proof bytes so downstream attestation or policy systems can bind their
decision to an immutable subject. See
[Proof Consumer Contract](docs/PROOF_CONSUMER_CONTRACT.md).

---

## It sits above the runtimes

MeshFit is not another inference engine.

It plans over them.

~~~text
                   MeshFit
                      │
        ┌─────────────┼─────────────┐
        │             │             │
     local         distributed     replica
        │             │             │
   llama.cpp        vLLM          Ollama
      MLX           SGLang        LocalAI
                    exo
                 llama.cpp RPC
~~~

The runtime executes.

MeshFit decides **which runtime, which nodes, which topology, and why**.

---

## Influences

MeshFit starts by learning from systems that already solved important pieces well:

- **llmfit** — hardware/model fit and practical memory reasoning
- **llama.cpp** — heterogeneous execution and CPU/GPU offload
- **llama.cpp RPC** — remote accelerator execution
- **exo** — discovery and topology-aware distributed inference
- **vLLM** — production tensor/pipeline parallel serving
- **SGLang** — high-throughput distributed serving
- **MLX** — Apple Silicon execution

The goal is not a collage of their features.

The goal is a better abstraction over the **placement problem**.

---

## Evidence loop

No estimate gets promoted to truth.

~~~text
predict
  ↓
deploy
  ↓
measure
  ↓
error
  ↓
calibrate
  ↺
~~~

MeshFit should track:

- TTFT / TPOT
- prefill / decode tok/s
- throughput under concurrency
- peak VRAM / RAM
- interconnect traffic
- model load time
- failure rate
- $/hour and $/1M tokens
- prediction error

The planner gets better only when reality disagrees with it.

---

## First reality gate

The first version is deliberately narrow.

~~~text
3–5 heterogeneous machines
        +
measured links
        +
one target model
        ↓
search candidate placements
        ↓
return Plan A / B / C
        ↓
exclude at least one bad participant
        ↓
run one plan
        ↓
compare prediction with reality
~~~

If that loop works, MeshFit has earned the right to become larger.

---

## Example topology

[`examples/heterogeneous-cluster.yaml`](examples/heterogeneous-cluster.yaml)

~~~yaml
sites:
  local:
    nodes:
      - id: workstation
        gpu: [RTX4090, RTX4090]
        ram_gb: 512
    network: 10GbE

  aws:
    nodes:
      - id: h100-pair
        gpu: [H100-80GB, H100-80GB]
        hourly_cost_usd: 6.74
    rtt_ms: 48

  office:
    nodes:
      - id: mac-studio
        accelerator: Apple-M3-Ultra
        unified_memory_gb: 128
    rtt_ms: 6
~~~

---

## Build order

**01 / Fit**  
Single-host discovery, model memory, quantization, context, KV cache, offload.

**02 / Graph**  
Hosts, accelerators, links, bandwidth, latency, cost, failure domains.

**03 / Search**  
Single-host, TP, PP, EP, replicas, RPC, hybrid placement.

**04 / Predict**  
Latency, throughput, memory, network traffic, cost, confidence.

**05 / Verify**  
Real benchmark runs, prediction error, calibration.

Full roadmap: [`ROADMAP.md`](ROADMAP.md)

---

## Current state

**Pre-alpha, but no longer docs-only.**

Implemented now:

~~~bash
# automatic local CPU/RAM + NVIDIA snapshot
meshfit discover

# optional stable logical node identity for containers / replaceable cloud hosts
MESHFIT_NODE_ID=local-4090 meshfit discover

# installed runtime identity
meshfit runtimes

# model artifact SHA-256 identity
meshfit inspect-model <model-file> <model-id> <format> <quantization>

# measured host-to-host link evidence
meshfit probe <peer> --bandwidth

# merge two discoveries + a probe into a compute graph
meshfit snapshot node-a.yaml node-b.yaml a-b.yaml > cluster.yaml

# run placement directly on the discovered graph
meshfit plan-snapshot cluster.yaml examples/placement-target.yaml

# structural placement from a hand-authored scenario still works
meshfit plan examples/v0.1-placement.yaml

# run a real local plan and emit a provenance-bound benchmark bundle
meshfit benchmark-local benchmark-request.yaml

# aggregate repeated candidates and compute observed placement regret
meshfit compare-benchmarks benchmark-001.yaml --markdown

# exact-match evidence query with structured execution fingerprint
meshfit predict examples/v0.2-evidence.yaml examples/v0.2-query.yaml
~~~

Current code includes:

- typed Hardware / Fabric / Model / Runtime / Workload / Plan IRs
- structural feasibility solver
- conservative cross-node TP admission gate
- Pareto frontier and explicit exclusion reasoning
- provenance-bound benchmark evidence
- structured execution identity and stable SHA-256 fingerprint
- first Linux/NVIDIA automatic discovery slice
- NVIDIA local NVLink/PCIe topology snapshot
- peer RTT/jitter probe and optional `iperf3` bandwidth measurement
- typed fabric endpoints for host-level and accelerator-level graph edges
- concurrent-wave benchmark runner with TTFT/TPOT/decode-rate and RAM/VRAM observation
- repeated benchmark comparison with observed oracle, placement regret, and publication gate

Still missing before the first real product proof:

- real multi-machine discovery + measured links
- real heterogeneous Benchmark 001 runs across MeshFit and baseline placements
- real GPU validation of the VRAM sampler
- calibrated performance prediction from captured evidence

The current proof target is **Benchmark 001**: same real workload, simple baselines versus MeshFit, with the observed oracle and placement regret reported from retained benchmark bundles. Synthetic fixtures are never accepted as a publishable performance claim.

See [`docs/BENCHMARK_001.md`](docs/BENCHMARK_001.md) for the frozen protocol and [`docs/DIRECTION_AUDIT.md`](docs/DIRECTION_AUDIT.md) for the Reality Gates that keep the project on course.

---

## Bring an ugly cluster

Perfect clusters are boring.

If your setup is difficult to reason about, it is useful.

Open a **Weird Cluster / Reality Probe** issue.

---

<div align="center">

### MeshFit

**Place the model where it actually belongs.**

</div>