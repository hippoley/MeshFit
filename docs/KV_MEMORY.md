# Workload-aware KV memory

MeshFit must not treat KV cache as a fixed model constant when placement depends on context length and scheduler residency.

## Execution profile

`ModelIR.kv_cache_model` supports two explicit forms:

- `bytes_per_token`: a supplied execution-profile coefficient.
- `transformer`: derives bytes/token as `2 × layers × kv_heads × head_dim × bytes_per_element`, where the factor 2 accounts for K and V.

If no structured KV model is supplied, `kv_cache_gb` remains a legacy fixed fallback. MeshFit labels that fallback in plan assumptions instead of pretending it is workload-aware.

## Workload

`WorkloadIR` distinguishes:

- `concurrency`: request concurrency.
- `max_active_sequences`: the number of sequences assumed to have resident KV state at once.

When `max_active_sequences` is absent, MeshFit conservatively uses `concurrency`.

The planner therefore computes:

```text
KV bytes =
    bytes_per_token
  × context_tokens
  × active_sequences

required accelerator memory =
    weight_memory
  + KV memory
```

This deliberately keeps scheduler residency separate from request concurrency while preserving a conservative default.

## Reality boundary

The analytical KV model is still an estimate. Real benchmark evidence and peak-memory calibration remain the authority for later prediction stages.
