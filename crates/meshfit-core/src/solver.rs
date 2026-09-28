use std::collections::HashSet;

use crate::ir::{
    AcceleratorBackend, ExclusionIR, InfrastructureIR, ModelIR, PlacementKind, PlacementReport,
    PlanIR, RejectionIR, RuntimeIR, ScenarioIR,
};

const CROSS_NODE_MAX_LATENCY_MS: f64 = 2.0;
const CROSS_NODE_MIN_BANDWIDTH_GBPS: f64 = 50.0;

pub fn solve(scenario: &ScenarioIR) -> PlacementReport {
    let mut feasible = Vec::new();
    let mut rejected = Vec::new();

    for runtime in &scenario.runtimes {
        enumerate_single_host(
            &scenario.infrastructure,
            &scenario.model,
            runtime,
            &mut feasible,
            &mut rejected,
        );
        enumerate_two_node_tp(
            &scenario.infrastructure,
            &scenario.model,
            runtime,
            &mut feasible,
            &mut rejected,
        );
    }

    let pareto = pareto_frontier(&feasible);
    let excluded_nodes = explain_exclusions(&scenario.infrastructure, &pareto);

    PlacementReport {
        model: scenario.model.id.clone(),
        feasible,
        pareto,
        rejected,
        excluded_nodes,
    }
}

fn enumerate_single_host(
    infra: &InfrastructureIR,
    model: &ModelIR,
    runtime: &RuntimeIR,
    feasible: &mut Vec<PlanIR>,
    rejected: &mut Vec<RejectionIR>,
) {
    for node in &infra.nodes {
        let backend = node.primary_backend();
        let candidate = format!("{}@{}", runtime.id, node.id);

        if !runtime.supports_backend(backend) {
            rejected.push(RejectionIR {
                candidate,
                code: "runtime_backend_mismatch".into(),
                reason: format!(
                    "runtime {} does not support {:?} on node {}",
                    runtime.id, backend, node.id
                ),
            });
            continue;
        }

        if !model.required_backends.is_empty() && !model.required_backends.contains(&backend) {
            rejected.push(RejectionIR {
                candidate,
                code: "model_backend_mismatch".into(),
                reason: format!("model {} does not allow {:?}", model.id, backend),
            });
            continue;
        }

        let required = model.required_memory_gb();
        let accel = node.accelerator_memory_gb();

        if accel >= required {
            let placement = if node.accelerators.len() > 1
                && required > node
                    .accelerators
                    .iter()
                    .map(|a| a.usable_memory_gb())
                    .fold(0.0_f64, f64::max)
                && runtime.supports_tp
            {
                PlacementKind::TensorParallel
            } else {
                PlacementKind::SingleHost
            };

            feasible.push(PlanIR {
                id: format!("{}:{}:{:?}", node.id, runtime.id, placement),
                placement,
                runtime: runtime.id.clone(),
                nodes: vec![node.id.clone()],
                required_memory_gb: required,
                accelerator_memory_gb: accel,
                relative_compute: node.relative_compute(),
                hourly_cost_usd: node.hourly_cost_usd,
                memory_headroom_gb: accel - required,
                assumptions: vec!["v0.1 uses structural feasibility only; no latency prediction".into()],
            });
            continue;
        }

        let offload_capacity = accel + node.ram_gb * 0.75;
        if runtime.supports_cpu_offload && accel > 0.0 && offload_capacity >= required {
            feasible.push(PlanIR {
                id: format!("{}:{}:offload", node.id, runtime.id),
                placement: PlacementKind::CpuOffload,
                runtime: runtime.id.clone(),
                nodes: vec![node.id.clone()],
                required_memory_gb: required,
                accelerator_memory_gb: accel,
                relative_compute: node.relative_compute() * 0.55,
                hourly_cost_usd: node.hourly_cost_usd,
                memory_headroom_gb: offload_capacity - required,
                assumptions: vec![
                    "CPU offload feasibility uses 75% of system RAM as usable capacity".into(),
                    "v0.1 does not predict offload throughput".into(),
                ],
            });
        } else {
            rejected.push(RejectionIR {
                candidate,
                code: "insufficient_memory".into(),
                reason: format!(
                    "requires {:.1}GB; node {} exposes {:.1}GB accelerator memory",
                    required, node.id, accel
                ),
            });
        }
    }
}

fn enumerate_two_node_tp(
    infra: &InfrastructureIR,
    model: &ModelIR,
    runtime: &RuntimeIR,
    feasible: &mut Vec<PlanIR>,
    rejected: &mut Vec<RejectionIR>,
) {
    if !runtime.supports_tp {
        return;
    }

    for i in 0..infra.nodes.len() {
        for j in (i + 1)..infra.nodes.len() {
            let a = &infra.nodes[i];
            let b = &infra.nodes[j];
            let candidate = format!("{}@{}+{}", runtime.id, a.id, b.id);
            let backend_a = a.primary_backend();
            let backend_b = b.primary_backend();

            if backend_a != backend_b || !runtime.supports_backend(backend_a) {
                rejected.push(RejectionIR {
                    candidate,
                    code: "cross_node_backend_mismatch".into(),
                    reason: format!(
                        "TP pair requires a runtime-compatible shared backend; got {:?} and {:?}",
                        backend_a, backend_b
                    ),
                });
                continue;
            }

            let Some(link) = infra.link(&a.id, &b.id) else {
                rejected.push(RejectionIR {
                    candidate,
                    code: "missing_link".into(),
                    reason: "no measured fabric edge between nodes".into(),
                });
                continue;
            };

            let (Some(latency_ms), Some(bandwidth_gbps)) =
                (link.latency_ms, link.bandwidth_gbps)
            else {
                rejected.push(RejectionIR {
                    candidate,
                    code: "unmeasured_link".into(),
                    reason: "fabric relation is known but bandwidth/latency have not been measured".into(),
                });
                continue;
            };

            if latency_ms > CROSS_NODE_MAX_LATENCY_MS
                || bandwidth_gbps < CROSS_NODE_MIN_BANDWIDTH_GBPS
            {
                rejected.push(RejectionIR {
                    candidate,
                    code: "fabric_too_slow".into(),
                    reason: format!(
                        "cross-node TP gate requires <= {:.1}ms and >= {:.0}Gbps; measured {:.1}ms / {:.1}Gbps",
                        CROSS_NODE_MAX_LATENCY_MS,
                        CROSS_NODE_MIN_BANDWIDTH_GBPS,
                        latency_ms,
                        bandwidth_gbps
                    ),
                });
                continue;
            }

            let memory = a.accelerator_memory_gb() + b.accelerator_memory_gb();
            if memory < model.required_memory_gb() {
                rejected.push(RejectionIR {
                    candidate,
                    code: "insufficient_pair_memory".into(),
                    reason: format!(
                        "pair exposes {:.1}GB but model requires {:.1}GB",
                        memory,
                        model.required_memory_gb()
                    ),
                });
                continue;
            }

            feasible.push(PlanIR {
                id: format!("{}+{}:{}:tp", a.id, b.id, runtime.id),
                placement: PlacementKind::TensorParallel,
                runtime: runtime.id.clone(),
                nodes: vec![a.id.clone(), b.id.clone()],
                required_memory_gb: model.required_memory_gb(),
                accelerator_memory_gb: memory,
                relative_compute: a.relative_compute() + b.relative_compute(),
                hourly_cost_usd: a.hourly_cost_usd + b.hourly_cost_usd,
                memory_headroom_gb: memory - model.required_memory_gb(),
                assumptions: vec![format!(
                    "cross-node TP admitted by structural fabric gate: {:.1}Gbps / {:.1}ms",
                    bandwidth_gbps, latency_ms
                )],
            });
        }
    }
}

fn pareto_frontier(plans: &[PlanIR]) -> Vec<PlanIR> {
    plans
        .iter()
        .filter(|candidate| {
            !plans.iter().any(|other| {
                if other.id == candidate.id {
                    return false;
                }

                let no_more_expensive = other.hourly_cost_usd <= candidate.hourly_cost_usd;
                let no_less_compute = other.relative_compute >= candidate.relative_compute;
                let no_less_headroom = other.memory_headroom_gb >= candidate.memory_headroom_gb;
                let strictly_better = other.hourly_cost_usd < candidate.hourly_cost_usd
                    || other.relative_compute > candidate.relative_compute
                    || other.memory_headroom_gb > candidate.memory_headroom_gb;

                no_more_expensive && no_less_compute && no_less_headroom && strictly_better
            })
        })
        .cloned()
        .collect()
}

fn explain_exclusions(infra: &InfrastructureIR, pareto: &[PlanIR]) -> Vec<ExclusionIR> {
    let used: HashSet<&str> = pareto
        .iter()
        .flat_map(|p| p.nodes.iter().map(String::as_str))
        .collect();

    infra
        .nodes
        .iter()
        .filter(|n| !used.contains(n.id.as_str()))
        .map(|node| {
            let backend = node.primary_backend();
            let reason = match backend {
                AcceleratorBackend::Metal => {
                    "not present on the current Pareto frontier; mixed-backend synchronous sharding is not admitted in v0.1"
                }
                _ => "not present on the current Pareto frontier under cost/compute/headroom objectives",
            };
            ExclusionIR {
                node: node.id.clone(),
                reason: reason.into(),
                suggested_role: "replica, batch worker, fallback endpoint, or a future placement candidate".into(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn scenario() -> ScenarioIR {
        ScenarioIR {
            infrastructure: InfrastructureIR {
                nodes: vec![
                    HardwareNodeIR {
                        id: "local".into(),
                        site: "dc".into(),
                        ram_gb: 256.0,
                        accelerators: vec![
                            AcceleratorIR {
                                id: "g0".into(),
                                backend: AcceleratorBackend::Cuda,
                                memory_gb: 24.0,
                                free_memory_gb: Some(23.0),
                                relative_compute: 5.0,
                            },
                            AcceleratorIR {
                                id: "g1".into(),
                                backend: AcceleratorBackend::Cuda,
                                memory_gb: 24.0,
                                free_memory_gb: Some(23.0),
                                relative_compute: 5.0,
                            },
                        ],
                        hourly_cost_usd: 0.0,
                    },
                    HardwareNodeIR {
                        id: "remote".into(),
                        site: "cloud".into(),
                        ram_gb: 256.0,
                        accelerators: vec![AcceleratorIR {
                            id: "h0".into(),
                            backend: AcceleratorBackend::Cuda,
                            memory_gb: 80.0,
                            free_memory_gb: Some(78.0),
                            relative_compute: 10.0,
                        }],
                        hourly_cost_usd: 3.5,
                    },
                    HardwareNodeIR {
                        id: "mac".into(),
                        site: "office".into(),
                        ram_gb: 128.0,
                        accelerators: vec![AcceleratorIR {
                            id: "m0".into(),
                            backend: AcceleratorBackend::Metal,
                            memory_gb: 128.0,
                            free_memory_gb: Some(110.0),
                            relative_compute: 3.0,
                        }],
                        hourly_cost_usd: 0.0,
                    },
                ],
                links: vec![FabricEdgeIR {
                    from: "local".into(),
                    to: "remote".into(),
                    kind: LinkKind::Wan,
                    bandwidth_gbps: Some(10.0),
                    latency_ms: Some(35.0),
                    jitter_ms: 1.0,
                    egress_cost_usd_per_gb: 0.0,
                }],
            },
            model: ModelIR {
                id: "test-70b-q4".into(),
                parameters_b: 70.0,
                active_parameters_b: None,
                weight_memory_gb: 41.0,
                kv_cache_gb: 5.0,
                is_moe: false,
                required_backends: vec![],
            },
            runtimes: vec![
                RuntimeIR {
                    id: "vllm".into(),
                    backends: vec![AcceleratorBackend::Cuda],
                    supports_tp: true,
                    supports_pp: true,
                    supports_ep: false,
                    supports_rpc: false,
                    supports_cpu_offload: false,
                },
                RuntimeIR {
                    id: "llama.cpp".into(),
                    backends: vec![
                        AcceleratorBackend::Cuda,
                        AcceleratorBackend::Metal,
                        AcceleratorBackend::Cpu,
                    ],
                    supports_tp: false,
                    supports_pp: false,
                    supports_ep: false,
                    supports_rpc: true,
                    supports_cpu_offload: true,
                },
            ],
            workload: WorkloadIR {
                context_tokens: 32768,
                concurrency: 20,
                p95_latency_ms: Some(2000),
                budget_per_day_usd: Some(200.0),
            },
        }
    }

    #[test]
    fn slow_wan_pair_is_rejected_for_tp() {
        let report = solve(&scenario());
        assert!(report
            .rejected
            .iter()
            .any(|r| r.code == "fabric_too_slow"));
    }

    #[test]
    fn metal_node_can_be_feasible_through_llama_cpp() {
        let report = solve(&scenario());
        assert!(report
            .feasible
            .iter()
            .any(|p| p.nodes == vec!["mac".to_string()] && p.runtime == "llama.cpp"));
    }

    #[test]
    fn pareto_frontier_is_not_empty() {
        let report = solve(&scenario());
        assert!(!report.pareto.is_empty());
    }
}
