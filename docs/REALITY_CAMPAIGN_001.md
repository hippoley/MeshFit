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
  > compute-proxy-gpu0.json
```

For multi-GPU hosts, run it once per physical GPU by changing `--device`.
Do not run multiple probes concurrently on the same host.

The JSON emits raw timings plus a median dense-GEMM TFLOPS value. Copy
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
- Keep matrix size, warmup count, and repeat count identical.
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
