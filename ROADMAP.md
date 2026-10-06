# Roadmap

MeshFit's roadmap is ordered by the shortest path to a real closed loop.

## 1. Structural placement — v0.1

- [x] core IRs
- [x] hard feasibility pruning
- [x] workload-aware KV memory — context + active-sequence residency affect feasibility; CI Reality Verified on run #326
- [x] single-host candidates — explicit accelerator-level selection; no host-wide VRAM aggregation
- [x] conservative cross-node TP gate — explicit device pairs over measured node fabric
- [x] per-device TP shard capacity — weakest-device headroom, no aggregate-VRAM false positives; CI Reality Verified on run #353
- [x] analytical TP communication admission — measured fabric + explicit model profile + workload ms/token budget; CI Reality Verified on run #418
- [x] Pareto frontier
- [x] communication-aware Pareto — lower known TP communication tax participates in dominance; unknown TP communication is not treated as zero; CI Reality Verified on run #445
- [x] explicit rejection / exclusion
- [x] CI/build verification — Reality Verified on PR CI run #295: fmt, clippy -D warnings, 45 workspace tests, release build, and full CLI smoke chain all passed

## 2. Evidence identity — v0.2

- [x] benchmark evidence store
- [x] provenance
- [x] exact-match evidence query
- [x] structured execution identity
- [x] stable execution fingerprint
- [x] model artifact identity from file bytes — SHA-256 inspection implemented, real model validation pending
- [x] runtime + driver identity capture primitives implemented — real benchmark bundle pending
- [x] repeated-run aggregation + p50/p95/stddev/CV comparison harness — stability gate CI Reality Verified on run #463; real repeated hardware runs pending

## 3. Automatic discovery — v0.2.5

- [x] CPU / RAM — implementation complete on Linux, real host validation pending
- [x] NVIDIA GPU / free VRAM / driver — implementation complete, real GPU validation pending
- [ ] Apple unified memory
- [x] AMD / Intel baseline discovery — `amd-smi` / `xpu-smi` inventory parsers and unit conversion CI-verified on run #787; real AMD/Intel hardware validation pending
- [x] stable vendor-local accelerator IDs — NVIDIA `gpuN`, AMD `amdN`, Intel `xpuN`, Apple `metal0`; CI Reality Verified on run #835
- [x] PCIe / NVLink topology — parser implemented via `nvidia-smi topo -m`, real GPU validation pending
- [x] runtime versions — vLLM / llama.cpp PATH discovery implemented
- [x] RTT probe — implementation complete, real peer validation pending
- [x] optional bandwidth probe — `iperf3` client path implemented, real peer validation pending
- [x] merge discovered hosts + measured peer edges into one generated InfrastructureIR — 2-node and N-node manifest fixture E2E implemented, real multi-host validation pending

## 4. Execute and measure — v0.3

- [x] llama.cpp plan compiler — single-host and explicit CPU-offload launch specs implemented
- [x] vLLM plan compiler — single-host and same-node TP launch specs implemented
- [x] executable launch plan contract — program/args/service endpoint + explicit accelerator binding
- [x] benchmark source provenance — build-time MeshFit Git commit embedded and CI-verified (run #224)
- [x] benchmark bundle + evidence conversion — request-level latency/decode records plus wave-level throughput/resource records
- [x] TTFT / TPOT — local HTTP/SSE measurement runner implemented and mock-runtime CI verified
- [x] decode tok/s — usage-backed derivation implemented; no token-rate invention when usage is unavailable
- [x] peak RAM — process-tree RSS sampling integrated and Linux mock-runtime CI verified (run #216)
- [ ] peak VRAM — NVIDIA process-tree sampler implemented and parser-tested; real GPU validation pending
- [x] evidence bundle — ExecutionIdentity + provenance + request/wave measurements emitted; real model/GPU validation still pending

## 5. Predict and calibrate — v0.4

- [x] repeated-run candidate aggregation and observed-oracle placement regret
- [x] stability-aware publishability gate — objective-specific independent-run CV <= 20%; CI Reality Verified on run #463
- [x] direct best-baseline headline — objective-aware MeshFit improvement/regression delta, still gated by publishability; CI Reality Verified on run #480
- [x] per-host benchmark preflight — host/runtime/model/evidence hard gates; full CLI Reality Verified on CI run #553
- [x] preflight-gated one-run executor — logical host override, locking, source-plan validation, atomic bundle write; full CLI Reality Verified on CI run #576
- [x] evidence-safe overwrite — backup/restore replacement protects existing benchmark evidence; full CI Reality Verified on run #583
- [x] candidate-level repeated-run executor — shared one-run core, pre-scan, validated resume, explicit overwrite, completeness recheck; 62-step CI Reality Verified on run #616
- [x] host-local model path reuse — `MESHFIT_MODEL_PATH`, SHA-gated host path, plan+model executable cache identity, repeated-run hash reuse; PR CI Reality Verified on run #716
- [x] host-oriented worklist — logical-host pending/valid/invalid run visibility; CI Reality Verified on run #601
- [x] host-local model artifact override — SHA-256 identity verification across preflight/run paths; CI Reality Verified on run #641
- [x] fail-fast host-level executor — run every candidate assigned to one logical host through the shared safe execution core; CI Reality Verified on run #655
- [x] experiment-level finalization — complete-kit gate + shared comparison path + publishability enforcement; 80-step CI Reality Verified on run #674
- [x] benchmark-host hardware attestation — logical host IDs are checked against snapshot hardware identity before execution; architecture/OS/heterogeneous accelerator profile/RAM are hard gates, CPU/driver drift is explicit warning
- [x] host-level readiness audit — one read-only report reuses execution preflight across every candidate assigned to a machine, exposes local hardware/runtime/model readiness, and supports a non-zero `--require-ready` gate before any runtime launch
- [x] materialized workload payload — Benchmark 001 kit owns the exact prompt/max-output/warmup/measured/timeouts used by real execution; prompt-file materialization and config equality prevent a short hidden prompt from masquerading as a long-context workload
- [x] port-scoped batch execution isolation — run-one locks one slot, candidate/host executors hold the listen-port lock across the full batch to prevent interleaved timing evidence; 109-step main CI Reality Verified on run #764
- [ ] real repeated-run distributions on heterogeneous hardware
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


## Device placement invariant

MeshFit plans concrete accelerators, not just hosts.

- single-device plans account only for the selected accelerator's usable memory
- local TP plans list every selected accelerator explicitly
- vLLM launch specs bind CUDA selections through `CUDA_VISIBLE_DEVICES`
- TP size is derived from selected devices, never inferred from aggregate memory
- runtime evidence fingerprints preserve flag/value ordering to avoid accidental identity collisions
