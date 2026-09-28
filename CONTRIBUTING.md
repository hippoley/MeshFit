# Contributing to MeshFit

MeshFit gets better from **weird real infrastructure**, not synthetic perfect clusters.

The most valuable contributions are often not code.

## Bring us a topology

Open a **Weird Cluster / Reality Probe** issue if you have:

- mixed NVIDIA + Apple Silicon
- old + new GPUs
- cloud + on-prem
- multi-region cloud
- NVLink + Ethernet
- InfiniBand + ordinary LAN
- 1/10/25/100/400GbE
- Thunderbolt
- high-latency WAN
- edge devices
- large RAM / small VRAM
- MoE workloads
- anything that makes deployment planning awkward

Please include:

1. node hardware;
2. accelerator memory;
3. RAM;
4. network topology;
5. measured RTT/bandwidth if available;
6. target model;
7. context length;
8. concurrency;
9. latency/cost constraints;
10. what you tried and what actually happened.

## Evidence is more valuable than opinion

A useful contribution looks like:

~~~text
Prediction:
TP=2 on nodes A+B should reach 42 tok/s.

Observed:
31 tok/s.

Evidence:
iperf3: 22.4 Gbps
RTT: 1.6 ms
runtime: vLLM ...
~~~

That discrepancy is exactly the kind of thing MeshFit should learn from.

## Code contributions

The project roadmap is organized around five gates:

1. single-host parity
2. compute graph
3. candidate planner
4. performance/cost model
5. reality calibration

Please keep new features tied to one of these gates and include a test or evidence artifact where possible.
