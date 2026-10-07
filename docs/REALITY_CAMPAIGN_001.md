# Reality Campaign 001 — compute proxy

Benchmark 001 names two baselines by compute rank: `single-best-node` and
`max-aggregate-compute`. Automatic discovery intentionally does not invent a
compute score, so real discovered accelerators currently carry
`relative_compute: 0.0` and the readiness gate refuses publication.

Use `tools/compute_proxy.py` only to supply that missing ordering signal.

## Contract

Run the same command on every CUDA accelerator that may participate in the
experiment:

```bash
python3 tools/compute_proxy.py \
  --device 0 \
  --dtype bf16 \
  --size 8192 \
  --warmup 5 \
  --repeats 20 \
  --max-cv 0.10 \
  --require-stable \
  > compute-proxy-gpu0.json
```

For multi-GPU hosts, run it once per physical GPU by changing `--device`.
Do not run multiple probes concurrently on the same host.

`--device` is a PyTorch logical CUDA index. If `CUDA_VISIBLE_DEVICES` is set,
that index may be remapped relative to `nvidia-smi`. The evidence JSON records
`CUDA_VISIBLE_DEVICES` and, when the PyTorch runtime exposes them, the device
UUID / PCI bus identifier. Use those fields to bind the proxy evidence to the
physical discovery record; never assume logical `cuda:0` means physical GPU 0.

The JSON emits raw timings, per-sample TFLOPS, median/mean TFLOPS, sample
standard deviation, CV, and a `stable` result. With `--require-stable`, the
command exits non-zero when CV exceeds `--max-cv`. Copy
`relative_compute` into the corresponding accelerator entry of the discovery
YAML **without changing any other discovered identity field**, and retain the
JSON next to the discovery/snapshot inputs.

For a local TP placement, MeshFit already sums the selected devices'
`relative_compute`, so the same per-device proxy naturally produces an
aggregate-compute baseline.

## Rules

- Use one precision across the whole campaign. The default is BF16.
- If any compared GPU cannot run that precision, choose another common precision
  and rerun **every** node. Never mix BF16 and FP16 scores.
- Keep matrix size, warmup count, repeat count, and `max_cv` identical.
- Use `--require-stable` for Reality Campaign evidence. Do not copy a score
  from an output where `stable: false`.
- Treat the score only as a baseline ordering proxy.
- Do not present the GEMM TFLOPS result as inference throughput or as a MeshFit
  performance result.
- Retain the output JSON as Reality Campaign evidence.

After filling non-zero scores, rerun:

```bash
meshfit benchmark-candidates cluster.yaml target.yaml <meshfit-plan-id> --require-distinct
meshfit benchmark-kit cluster.yaml target.yaml <meshfit-plan-id> <model-path> model-identity.yaml --require-ready --write-dir benchmark-001
```

The paid benchmark run should not start until both commands pass.


## End-to-end real-hardware campaign

The software control plane is no longer the bottleneck. The next publishable
result must come from real heterogeneous machines, real network measurements,
real runtime execution, repeated raw bundles, and the existing finalization
gate.

Use one campaign ID and keep every generated file under one evidence directory.
Do not edit generated BenchmarkBundle files by hand.

### 1. Discover every real host

On each machine, give MeshFit a stable logical node ID and capture discovery:

```bash
export MESHFIT_NODE_ID=node-a
meshfit discover > node-a.raw.yaml
cp node-a.raw.yaml node-a.yaml
```

Repeat for every host. Treat `node-*.raw.yaml` as immutable machine-derived
evidence. Insert `relative_compute` only into the working `node-*.yaml`
copies used to build the coordinator snapshot. Never edit the raw discovery
artifacts in place.

The hardware identity, accelerator inventory, local topology, driver metadata,
and runtime discovery fields in the working copies must remain byte-for-byte
derived from their raw counterparts; only the explicitly documented
`relative_compute` field may be added or changed.

### 2. Measure the compute-ordering proxy

Run the fixed proxy once per candidate GPU using exactly the same dtype, matrix
size, warmup count, and repeat count across the campaign:

```bash
python3 tools/compute_proxy.py \
  --device 0 \
  --dtype bf16 \
  --size 8192 \
  --warmup 5 \
  --repeats 20 \
  --max-cv 0.10 \
  --require-stable \
  > node-a-gpu0-compute-proxy.json
```

Copy only the resulting `relative_compute` value into the matching accelerator
entry in that host's working discovery YAML (for example `node-a.yaml`).
Retain the corresponding immutable `node-a.raw.yaml` and every proxy JSON
beside the snapshot inputs. These values rank compute baselines; they are not
inference throughput measurements.

### 3. Measure every cross-host link used by candidate plans

For every host pair that may participate in a distributed placement, capture a
real peer probe from the experiment environment. Preserve the probe YAML,
including source, timestamp, RTT, both directional bandwidth measurements, and
the conservative effective bandwidth.

`meshfit probe --bandwidth` measures both client-to-peer and peer-to-client
throughput. Because the current planner models a node link as an undirected
edge, `bandwidth_gbps` is the slower of the two measured directions. If either
direction cannot be measured, effective bandwidth is omitted so readiness fails
closed instead of treating one direction as representative of both.

Before measuring, prepare every participating host to receive peer probes over
the experiment's private network. `meshfit probe --bandwidth` uses ping for RTT
and iperf3 for throughput; iperf3 therefore needs a server on the peer.

On each host, bind the server to that host's private experiment address and keep
its PID so it can be stopped cleanly after evidence capture:

```bash
export MESHFIT_PRIVATE_IP=<this-host-private-address>

command -v iperf3
test ! -e /tmp/meshfit-iperf3.pid
iperf3 -s -D -B "$MESHFIT_PRIVATE_IP" -I /tmp/meshfit-iperf3.pid
test -s /tmp/meshfit-iperf3.pid
kill -0 "$(cat /tmp/meshfit-iperf3.pid)"
```

Allow ICMP and TCP/5201 only between the experiment hosts. Do not expose the
iperf3 listener to the public Internet. Verify the private path before recording
evidence:

```bash
ping -c 1 <peer-private-address>
```

Because one MeshFit bandwidth probe now measures both forward and reverse
throughput and the planner treats the resulting node link as undirected, capture
one probe for each unordered host pair:

```bash
meshfit probe <node-b-private-address> --bandwidth > node-a-node-b-probe.yaml
meshfit probe <node-c-private-address> --bandwidth > node-a-node-c-probe.yaml
# run from node-b:
meshfit probe <node-c-private-address> --bandwidth > node-b-node-c-probe.yaml
```

After all peer evidence has been captured on a host, stop only the server started
for this campaign:

```bash
kill "$(cat /tmp/meshfit-iperf3.pid)"
rm -f /tmp/meshfit-iperf3.pid
```

Do not substitute configured NIC speed for measured bandwidth. If a candidate
requires a peer link that has no measured provenance, the readiness gate should
remain closed.

### 4. Build the coordinator snapshot

On the coordinator, build one snapshot from the final discovery files and peer
measurements, then inspect the candidate set before materializing anything:

```bash
meshfit snapshot \
  node-a.yaml \
  node-b.yaml \
  node-c.yaml \
  node-a-node-b-probe.yaml \
  node-a-node-c-probe.yaml \
  node-b-node-c-probe.yaml \
  > cluster.yaml

meshfit benchmark-candidates \
  cluster.yaml \
  target.yaml \
  <meshfit-plan-id> \
  --require-distinct
```

The campaign should not proceed unless the selected MeshFit plan and baseline
plans are genuinely distinct and all compute-ranked baselines have non-zero
observed proxy scores.

### 5. Inspect and freeze the real model identity

Inspect the exact immutable model artifact that will be used for the run:

```bash
meshfit inspect-model \
  /models/model.gguf \
  <model-id> \
  gguf \
  <quantization> \
  <revision> \
  > model-identity.yaml
```

The same model bytes may live at a different filesystem path on another host,
but their SHA-256 must match the materialized identity.

### 6. Materialize Benchmark 001 once

Create the kit on the coordinator. Pick an unused service port explicitly so the
runtime port becomes part of the frozen execution identity:

```bash
meshfit benchmark-kit \
  cluster.yaml \
  target.yaml \
  <meshfit-plan-id> \
  /models/model.gguf \
  model-identity.yaml \
  --listen-port 18181 \
  --max-peer-probe-age-seconds 1800 \
  --require-ready \
  --write-dir benchmark-001
```

Keep this directory immutable except for generated runtime artifacts and declared
`results/` slots. Do not regenerate the kit independently on each host.

### 7. Gate every real machine before launch

Place the same materialized kit on each benchmark host, preserving its contents.
Set the stable logical ID and, when necessary, the local path to the same model
bytes:

```bash
export MESHFIT_NODE_ID=node-a
export MESHFIT_MODEL_PATH=/data/models/model.gguf

meshfit benchmark-host-check benchmark-001 \
  --current-host \
  --require-ready
```

This gate must validate the actual machine, not just the declared node name. It
checks the snapshot hardware identity, local accelerator topology where required,
runtime availability, model SHA-256, listen port, and the relevant peer evidence.
Reality Campaign 001 freezes a 1800-second peer-probe freshness contract in
`kit.yaml`; if any required cross-host measurement is older than that at
preflight time, re-run the affected peer probe and rebuild the frozen snapshot
and kit rather than executing against stale topology evidence.

For TensorParallel candidates, local topology is part of the execution identity.
A machine with the same GPU inventory but different NVLink/PCIe structure is not
the same benchmark host.

### 8. Dry-run the complete host assignment

Before any GPU runtime starts:

```bash
meshfit benchmark-run-host benchmark-001 \
  --current-host \
  --resume \
  --dry-run
```

Read the candidate list, pending runs, effective model path, service port, and
preflight issues. Do not begin the paid run while `ready` is false.

### 9. Execute repeated runs

Run only the host assignment already frozen in the kit:

```bash
meshfit benchmark-run-host benchmark-001 \
  --current-host \
  --resume
```

Do not run unrelated GPU workloads on the machine during the measurement window.
Keep campaign power/performance configuration and runtime versions stable across
all repeated runs. Do not replace a failed run with a hand-edited bundle.

### 10. Return evidence to the coordinator

If all machines share the same writable experiment directory, no extra transport
step is needed.

If they do not share storage, do **not** copy arbitrary `results/` files into
the coordinator kit. On each benchmark host, export only the result slots that
the frozen kit assigned to that host:

```bash
meshfit benchmark-export-host benchmark-001 --current-host > node-a-evidence.yaml
```

Transfer the generated host-evidence package with your normal authenticated
file-transfer mechanism. Transport is not a trust source.

On the coordinator, import the package into the coordinator's original frozen
kit:

```bash
meshfit benchmark-import-host benchmark-001 node-a-evidence.yaml
```

The coordinator kit is the only slot whitelist. Import verifies the package
SHA-256 and re-attests the submitted evidence against the frozen hardware,
model, topology, runtime, listen port, plan, prompt, context, concurrency,
`max_tokens`, warmup count, measured request count, and timeout contract
**before writing any result file**.

The import is package-atomic: every submitted slot must validate before any slot
is written. Re-importing byte-equivalent evidence is idempotent. A different
bundle in an already-populated slot is rejected rather than overwritten.
Non-kit slots, path traversal, tampered packages, and request-contract drift must
all fail closed.

### 11. Require completeness before looking for a headline

On the coordinator:

```bash
meshfit benchmark-status benchmark-001 --require-complete
```

A missing, malformed, stale, foreign-hardware, wrong-model, wrong-runtime,
wrong-port, wrong-plan, or wrong-topology bundle must keep the campaign
incomplete.

### 12. Finalize through the publication gate

Only after completeness succeeds:

```bash
meshfit benchmark-finalize \
  benchmark-001 \
  --markdown \
  --require-publishable
```

Then emit the proof receipt:

```bash
meshfit benchmark-proof \
  benchmark-001 \
  --require-publishable
```

The first public MeshFit number must come from this finalized evidence, not from
the GEMM proxy, planner estimates, fixture bundles, CI data, or a manually chosen
best run.

## Stop conditions

Abort the campaign and fix reality instead of relaxing a gate if any of the
following occurs:

- a real accelerator still has zero or missing `relative_compute`;
- a required peer link lacks measured provenance;
- hardware or local topology no longer matches the frozen snapshot;
- the model SHA-256 differs between hosts;
- the selected runtime is unavailable on its assigned host;
- the service port is occupied;
- candidate plans collapse to the same effective placement;
- any persisted bundle fails re-attestation;
- repeated evidence is incomplete or non-publishable.

The next engineering decision should be driven by whichever of these real checks
fails on the actual machines. If none fail, stop changing the framework and run
the experiment.
