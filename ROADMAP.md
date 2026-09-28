# Roadmap

## Gate 1 — Single-host parity

- hardware discovery
- RAM / VRAM / unified memory
- model + quant + context memory model
- CPU/GPU offload
- runtime capability matrix
- measured local benchmark

## Gate 2 — Compute graph

- host graph
- accelerator graph
- PCIe / NVLink / InfiniBand / Ethernet links
- WAN links
- latency / bandwidth / jitter
- cost and failure domains

## Gate 3 — Candidate planner

- single-host
- local multi-GPU
- Tensor Parallel
- Pipeline Parallel
- Expert Parallel
- replica routing
- RPC / layer partition
- hybrid offload
- explicit node exclusion

## Gate 4 — Performance model

- TTFT
- TPOT
- decode tok/s
- prefill tok/s
- throughput
- memory
- network traffic
- $/hour
- $/1M tokens
- confidence

## Gate 5 — Reality calibration

- benchmark harness
- estimate vs observed
- prediction error
- device/runtime calibration
- community evidence
