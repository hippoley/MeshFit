# Cost Estimation Contract

MeshFit separates **declared marginal execution cost** from total cost of ownership.

The first cost contract accepts:

- a concrete `PlanIR`;
- an explicit predicted output throughput in tokens/s.

It computes:

```text
compute $ / 1M output tokens
  = declared hourly cost
  / (predicted output tokens/s × 3600)
  × 1,000,000

total declared marginal $ / 1M output tokens
  = compute component
  + modeled communication egress component
```

If a plan has an analytical communication estimate, its egress cost per million tokens is included.

## Evidence boundary

- Cross-node plans without a communication estimate are rejected. MeshFit does not silently assume zero network egress.
- A declared hourly cost of zero means only that the plan's **declared marginal compute cost** is zero.
- Zero does **not** mean ownership, electricity, facilities, depreciation, operations, or labor cost is zero.
- Predicted output throughput is an explicit input in this version. MeshFit does not invent throughput to make a cost number.

This contract is designed to accept a calibrated throughput predictor later without changing the cost definition.
