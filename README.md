<div align="center">

# MeshFit

### **Your infrastructure is messy. Your deployment plan shouldn't be.**

**Point MeshFit at your real compute estate. Get the best way to run the model.**

Local servers · cloud GPUs · edge devices · mixed CPU/GPU/NPU · NVLink · PCIe · LAN · WAN · VPN · multi-cloud

<br/>

**Fit models to infrastructure — not infrastructure to models.**

</div>

---

## The 10-second idea

You have:

~~~text
Local DC                 Office                  Cloud
2× RTX 4090              Mac Studio 128GB        2× H100
10 GbE                    1 GbE                   48 ms RTT
     │                       │                       │
     └──────────── VPN / WAN / LAN / NVLink / PCIe ┘
                             │
                         Jetson Edge
~~~

And you want to run a 70B model.

The hard question is **not**:

> “If I add all the VRAM together, does it fit?”

The hard questions are:

- Which machines should actually participate?
- Which machines should stay out?
- Should this be Tensor Parallel, Pipeline Parallel, Expert Parallel, replicas, RPC, or offload?
- Which runtime should execute it?
- What happens at 32K context?
- Is the bottleneck VRAM, memory bandwidth, PCIe, NVLink, WAN latency, or cost?
- Will adding one more machine make it faster — or slower?
- Does the cheapest plan violate the SLO?
- How wrong is the estimate once we benchmark reality?

**MeshFit is built to answer those questions.**

---

# Infrastructure in. Deployment plan out.

MeshFit turns:

~~~text
Infrastructure Graph
        +
Model
        +
Context
        +
Concurrency
        +
SLO
        +
Budget
~~~

into:

~~~text
Deployment Plan
├── selected nodes
├── excluded nodes + reasons
├── runtime
├── quantization
├── Tensor / Pipeline / Expert Parallel
├── replicas
├── CPU/GPU offload
├── expected VRAM / RAM
├── expected network traffic
├── TTFT / TPOT / tok/s
├── throughput
├── cost
├── bottleneck
└── confidence
~~~

Think of it as:

> ## **an infrastructure-to-model deployment compiler**

---

# The screenshot moment

~~~bash
meshfit plan \
  --infra ./my-cluster.yaml \
  --model deepseek-r1-70b \
  --context 32768 \
  --concurrency 20 \
  --p95 2s \
  --budget 200usd/day
~~~

Target output:

~~~text
MeshFit evaluated 14 feasible deployment candidates.

BEST LATENCY
────────────
AWS Tokyo
2× H100
vLLM
TP=2
BF16
32K context

TTFT p95       1.28 s
Decode         71 tok/s
Throughput     1,420 tok/s
Peak VRAM      74.1 GB/GPU
Cost           $6.74/h
Bottleneck     HBM bandwidth
SLO            PASS
Budget         PASS
Confidence     0.86


BEST COST
────────────
Foshan DC
2× RTX 4090
llama.cpp
Q4_K_M
GPU split + RAM offload

Decode         22 tok/s
Cost           local sunk cost
Bottleneck     PCIe / memory bandwidth
SLO            PASS
Budget         PASS
Confidence     0.72


DO NOT USE IN PRIMARY SHARD
───────────────────────────
Shenzhen Mac Studio

Why:
Adding it to the primary shard increases communication
latency more than the extra compute reduces inference time.

Better role:
  → independent replica
  → batch worker
  → failover endpoint
~~~

### **“DO NOT USE THIS NODE” is a first-class result.**

That is one of the core ideas behind MeshFit.

---

# One rule changes everything

## Aggregate memory ≠ usable model memory

This:

~~~text
A100 80GB
+ RTX 4090 24GB
+ Mac Studio 128GB
+ H100 80GB
~~~

does **not** automatically become:

~~~text
312GB usable for one model
~~~

A machine only helps if the actual execution path can use it efficiently.

MeshFit must reason about:

- CUDA / ROCm / Metal / SYCL / NPU compatibility
- current free memory, not just theoretical capacity
- PCIe / NVLink / InfiniBand topology
- LAN / WAN bandwidth
- RTT and jitter
- runtime capabilities
- collective communication cost
- KV-cache placement
- dense vs MoE communication
- storage / model load time
- cloud egress
- failure domains
- workload concurrency
- cost

Sometimes the highest-value optimization is **removing a machine from the plan**.

---

# Two worlds. One planner.

## 🖥️ Single host

Before distributing anything, MeshFit asks:

> **Can one box run this better by itself?**

A host might have:

~~~text
CPU + RAM
   │
PCIe
   │
GPU0 ─ NVLink ─ GPU1
   │
  NPU
~~~

Possible plans:

- single GPU
- local multi-GPU
- tensor split
- CPU ↔ GPU offload
- unified memory
- quant alternatives
- KV-cache alternatives
- runtime alternatives

Default bias:

> **Never distribute a workload when one machine is the better execution plan.**

## 🌐 Infrastructure mesh

When scale-out is useful, MeshFit reasons over the topology:

~~~text
DC A ── 100GbE ── DC B
 │                  │
NVLink             PCIe
 │                  │
H100×8            A100×4
 │
WAN 35ms
 │
Cloud burst
~~~

Possible plans:

- Replica Routing
- Tensor Parallel
- Pipeline Parallel
- Expert Parallel
- Layer / ring partition
- llama.cpp RPC
- hybrid CPU/RAM offload
- cloud burst
- failover replicas
- batch workers

The planner does **not** assume that more nodes means better performance.

---

# MeshFit is not another inference engine

Excellent engines already exist.

MeshFit plans **above** them.

| System | What MeshFit learns / uses |
|---|---|
| **llmfit** | machine → model fit, quant/memory reasoning, measured benchmark feedback |
| **llama.cpp** | heterogeneous local execution, GGUF, CPU/GPU offload |
| **llama.cpp RPC** | remote device execution |
| **exo** | discovery, topology-aware distributed inference |
| **vLLM** | production TP/PP serving |
| **SGLang** | high-throughput distributed serving |
| **MLX** | Apple Silicon execution |
| **Ollama / LM Studio / LocalAI** | local model lifecycle and serving |

MeshFit's job is the decision **before** deployment:

> **Given what I own or can rent, which execution plan should I actually use?**

---

# Why now

Today, engineers still do some version of this by hand:

~~~text
nvidia-smi
lscpu
ip link
iperf3
cloud pricing page
model card
quant calculator
vLLM docs
llama.cpp docs
spreadsheet
guess
deploy
benchmark
discover assumption was wrong
repeat
~~~

MeshFit's goal is to collapse that workflow into:

~~~bash
meshfit plan ./infra.yaml --model <model>
~~~

with assumptions that are visible, comparable, and testable.

---

# The planner

~~~text
                      ┌───────────────────┐
                      │      MeshFit      │
                      └─────────┬─────────┘
                                │
             ┌──────────────────┴──────────────────┐
             │                                     │
      Infrastructure Graph                  Model Profile
             │                                     │
             └──────────────────┬──────────────────┘
                                │
                          Candidate Search
                                │
               ┌────────────────┼────────────────┐
               │                │                │
          Single Host       Sharded          Replicated
               │                │                │
               └────────────────┼────────────────┘
                                │
                         Runtime Selection
                                │
           ┌──────────┬─────────┼─────────┬──────────┐
           │          │         │         │          │
       llama.cpp     MLX      vLLM     SGLang      exo
           │          │         │         │          │
           └──────────┴─────────┼─────────┴──────────┘
                                │
                             Bench
                                │
                          Calibration
~~~

The planner searches plans.

The runtime executes them.

The benchmark decides whether the planner was right.

---

# Reality loop

Estimates are hypotheses.

~~~text
prediction
   ↓
real deployment
   ↓
benchmark
   ↓
observed performance
   ↓
prediction error
   ↓
calibration
   ↓
better next prediction
~~~

Target evidence:

| Dimension | Metrics |
|---|---|
| latency | TTFT p50/p95 · TPOT p50/p95 |
| generation | decode tok/s · prefill tok/s |
| serving | throughput · concurrency scaling |
| memory | peak VRAM · peak RAM · KV growth |
| network | traffic · collective time · RTT sensitivity |
| operations | load time · failure rate · thermal throttling |
| economics | $/hour · $/1M tokens · egress |
| planner | estimate error · confidence calibration |

> **No benchmark evidence → no strong performance claim.**

---

# What makes this different

Most tools start from one side:

~~~text
machine → what model fits?
~~~

or:

~~~text
model → how do I distribute it?
~~~

MeshFit wants to solve the larger problem:

~~~text
real infrastructure
        ↓
all feasible execution topologies
        ↓
compare latency / throughput / memory / network / cost
        ↓
best deployment plan
        ↓
benchmark reality
~~~

The key abstraction is not a machine.

It is a **compute graph**.

---

# Example infrastructure

See `examples/heterogeneous-cluster.yaml`.

~~~yaml
sites:
  local-dc:
    nodes:
      - id: local-4090
        gpu: [RTX4090, RTX4090]
        ram_gb: 512
    network: 10GbE

  office:
    nodes:
      - id: office-mac
        accelerator: Apple-M3-Ultra
        unified_memory_gb: 128
    rtt_ms: 6

  aws-tokyo:
    nodes:
      - id: aws-h100
        gpu: [H100-80GB, H100-80GB]
    rtt_ms: 48
~~~

---

# CLI direction

~~~bash
meshfit discover ./infra.yaml
meshfit fit ./infra.yaml
meshfit plan ./infra.yaml --model qwen3-72b
meshfit compare ./infra.yaml --model qwen3-72b
meshfit explain node://office-mac
meshfit bench plan://latest
meshfit doctor
~~~

Machine-readable output:

~~~bash
meshfit plan ./infra.yaml --model qwen3-72b --json
~~~

---

# Build philosophy

### 01 — Parity first

Reproduce proven single-machine model-fit logic before pretending to solve the whole world.

### 02 — Synthesize proven ideas

Study llmfit, exo, llama.cpp, vLLM, SGLang, MLX, distributed inference systems, schedulers, and real benchmark projects.

Take mechanisms, not aesthetics.

### 03 — Reality before claims

Every performance model must eventually meet a real benchmark.

### 04 — Innovate where existing abstractions break

Especially:

- heterogeneous accelerator fleets
- hybrid cloud + local topology
- topology-aware cost
- explicit node exclusion
- uncertainty / confidence
- benchmark-driven calibration

---

# Status

> ## **Pre-alpha · building the first real planner**

This repository is intentionally honest about its state.

The first Reality Gate is:

~~~text
3–5 heterogeneous machines
        +
real measured links
        +
one target model
        ↓
generate Plan A / B / C
        ↓
exclude at least one bad participant
        ↓
run one plan
        ↓
compare estimate vs reality
~~~

When that works reliably, MeshFit stops being a good idea and starts becoming useful infrastructure.

---

# Roadmap

- [ ] single-host hardware discovery
- [ ] model / quant / context memory model
- [ ] infrastructure graph schema
- [ ] network link discovery and measurement
- [ ] candidate topology generation
- [ ] runtime capability matrix
- [ ] latency / throughput / cost estimator
- [ ] explicit node exclusion reasoning
- [ ] executable launch plans
- [ ] real benchmark harness
- [ ] calibration database
- [ ] community benchmark evidence
- [ ] TUI / visual topology explorer

---

# Contributing

If you have a weird setup, **that is exactly what this project needs**.

Interesting test cases include:

- CUDA + Apple Silicon
- old GPUs + new GPUs
- cloud + on-prem
- multi-region cloud
- NVLink + Ethernet
- 1/10/25/100/400GbE
- InfiniBand
- Thunderbolt
- high-latency WAN
- edge devices
- large RAM / small VRAM systems
- MoE models

Open an issue with your topology and the model you want to run.

The ugly cases are the useful cases.

---

<div align="center">

## The north star

### **Infrastructure in. Executable model topology out.**

`model × hardware × network × runtime × SLO × cost → plan`

**MeshFit**

*Fit models to infrastructure, not infrastructure to models.*

</div>
