# Roadmap

MeshFit's roadmap is ordered by the shortest path to a real closed loop.

## 1. Structural placement — v0.1

- [x] core IRs
- [x] hard feasibility pruning
- [x] single-host candidates
- [x] conservative cross-node TP gate
- [x] Pareto frontier
- [x] explicit rejection / exclusion
- [ ] CI/build verification

## 2. Evidence identity — v0.2

- [x] benchmark evidence store
- [x] provenance
- [x] exact-match evidence query
- [x] structured execution identity
- [x] stable execution fingerprint
- [x] model artifact identity from file bytes — SHA-256 inspection implemented, real model validation pending
- [x] runtime + driver identity capture primitives implemented — real benchmark bundle pending
- [ ] repeated-run statistics

## 3. Automatic discovery — v0.2.5

- [x] CPU / RAM — implementation complete on Linux, real host validation pending
- [x] NVIDIA GPU / free VRAM / driver — implementation complete, real GPU validation pending
- [ ] Apple unified memory
- [ ] AMD / Intel baseline discovery
- [x] PCIe / NVLink topology — parser implemented via `nvidia-smi topo -m`, real GPU validation pending
- [x] runtime versions — vLLM / llama.cpp PATH discovery implemented
- [x] RTT probe — implementation complete, real peer validation pending
- [x] optional bandwidth probe — `iperf3` client path implemented, real peer validation pending
- [x] merge discovered hosts + measured peer edges into one generated InfrastructureIR — 2-node and N-node manifest fixture E2E implemented, real multi-host validation pending

## 4. Execute and measure — v0.3

- [x] llama.cpp plan compiler — single-host and explicit CPU-offload launch specs implemented
- [x] vLLM plan compiler — single-host and same-node TP launch specs implemented
- [x] executable launch plan contract — program/args/service endpoint
- [x] benchmark bundle contract + evidence conversion — real process/HTTP runner still pending
- [x] TTFT / TPOT metric derivation contract — real measurement runner pending
- [x] decode tok/s derivation contract — prefill and real measurement runner pending
- [ ] peak memory
- [ ] evidence bundle

## 5. Predict and calibrate — v0.4

- [ ] repeated-run distributions
- [ ] prediction intervals
- [ ] memory calibration
- [ ] latency / throughput model
- [ ] cost model
- [ ] estimate vs observed error
- [ ] bounded evidence transfer

## 6. Ask for the missing evidence — v0.5

- [ ] uncertainty-aware probe selection
- [ ] targeted network probe
- [ ] targeted compute probe
- [ ] decision-changing probe validation

## 7. Adapt — v0.6

- [ ] live telemetry
- [ ] re-placement
- [ ] failure / load / cost events
- [ ] counterfactual plan comparison

See [docs/VERSIONS.md](docs/VERSIONS.md) for exit criteria.
