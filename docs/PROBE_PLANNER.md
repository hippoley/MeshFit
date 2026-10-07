# Decision-relevant probe planner

MeshFit should not respond to missing evidence with a generic "benchmark more" recommendation.

The first probe planner consumes the real `PlacementReport.rejected` set and recommends only evidence that can plausibly change an admission decision.

## Evidence gaps that produce probes

| Rejection | Probe |
|---|---|
| `unmeasured_link` | measure peer RTT + bandwidth |
| `missing_link` | discover/declare the peer fabric path |
| `missing_local_fabric` | discover local PCIe/NVLink topology |
| `missing_tp_communication_profile` | supply model TP communication profile |
| `missing_tp_communication_budget` | supply workload TP communication budget |

Structural failures such as insufficient memory, TP shard capacity failure, or backend incompatibility do **not** produce a probe recommendation. More measurement cannot make those structural facts disappear.

## Priority

Recommendations are ranked by:

1. base decision value of the evidence gap,
2. how many rejected candidates the same probe can unlock,
3. an additional boost when no feasible plan exists at all.

The output is structured YAML and includes the affected candidates, the missing evidence, why it may change the decision, and a suggested action.

## CLI

```bash
meshfit recommend-probes snapshot.yaml target.yaml
```

This command first runs the normal solver and then derives recommendations from its rejection set. There is no second placement logic.

## Reality boundary

This is the first v0.5 step. It ranks known missing-evidence classes; it does not yet estimate expected value of information or automatically execute probes. Those come only after this mapping is CI-verified and then validated on real ambiguous placements.
