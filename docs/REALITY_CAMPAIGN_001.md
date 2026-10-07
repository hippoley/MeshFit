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
