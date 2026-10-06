# Tensor-parallel communication cost

Cross-node tensor parallelism is not admitted from bandwidth and latency thresholds alone.

MeshFit requires three pieces of evidence:

1. a measured node-to-node link with bandwidth and latency,
2. an explicit model TP communication profile,
3. a workload communication budget in milliseconds per generated token.

## Analytical model

For TP size `N`, MeshFit applies a ring-style volume factor:

```text
ring_factor = 2 × (N - 1) / N
bytes_per_token = base_bytes_per_token × ring_factor
```

The first communication profile supports:

- direct `bytes_per_token + synchronizations_per_token`
- transformer shape:
  `layers × hidden_size × bytes_per_element × collectives_per_layer`

The link estimate uses a conservative effective-bandwidth factor:

```text
effective_bandwidth = measured_bandwidth × 0.70
transfer_ms/token = bytes_per_token / effective_bytes_per_second
sync_ms/token = measured_latency_ms × synchronizations_per_token
total_ms/token = transfer_ms/token + sync_ms/token
```

A candidate is structurally admitted only when:

```text
total_ms/token <= workload.max_tp_communication_ms_per_token
```

The estimate also records approximate egress cost per million tokens.

## Reality boundary

This is an analytical structural gate, not a throughput predictor. Its confidence is emitted as `analytical_unvalidated` until runtime benchmarks calibrate the communication model.

Missing model profile or missing workload budget is a rejection, not permission to guess.


## Pareto use

Communication evidence is also a Pareto objective.

- lower known `total_ms_per_token` is better,
- a known communication estimate is preferred over an unknown TP communication cost when all other objectives are equal or better,
- unknown TP communication is never silently treated as zero,
- non-distributed placements have zero cross-node communication tax for this objective.

This makes communication evidence affect plan selection, not only feasibility admission.
