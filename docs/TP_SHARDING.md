# Tensor-parallel shard capacity

MeshFit does not treat aggregate accelerator memory as sufficient evidence for tensor parallel feasibility.

For a structural TP candidate with `N` selected accelerators:

```text
equal_shard_required = required_memory / N
```

Every selected accelerator must expose at least `equal_shard_required` usable memory.

The reported TP memory headroom is the bottleneck shard headroom:

```text
min(device_usable_memory - equal_shard_required)
```

not `sum(device_memory) - required_memory`.

This deliberately rejects configurations such as 80GB + 24GB for a 100GB two-way shard even though aggregate memory is 104GB. Later benchmark evidence may refine runtime-specific sharding behavior, but structural feasibility remains conservative by default.
