use std::collections::{HashMap, HashSet};

use crate::ir::{
    AcceleratorBackend, AcceleratorIR, AcceleratorRefIR, ExclusionIR, InfrastructureIR, ModelIR,
    PlacementKind, PlacementReport, PlanIR, RejectionIR, RuntimeIR, ScenarioIR,
};

const CROSS_NODE_MAX_LATENCY_MS: f64 = 2.0;
const CROSS_NODE_MIN_BANDWIDTH_GBPS: f64 = 50.0;

pub fn solve(scenario: &ScenarioIR) -> PlacementReport {
    let mut feasible = Vec::new();
    let mut rejected = Vec::new();

    for runtime in &scenario.runtimes {
        enumerate_single_node(
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

fn accelerator_allowed(accelerator: &AcceleratorIR, model: &ModelIR, runtime: &RuntimeIR) -> bool {
    runtime.supports_backend(accelerator.backend)
        && (model.required_backends.is_empty()
            || model.required_backends.contains(&accelerator.backend))
}

fn enumerate_single_node(
    infra: &InfrastructureIR,
    model: &ModelIR,
    runtime: &RuntimeIR,
    feasible: &mut Vec<PlanIR>,
    rejected: &mut Vec<RejectionIR>,
) {
    let required = model.required_memory_gb();

    for node in &infra.nodes {
        let compatible: Vec<&AcceleratorIR> = node
            .accelerators
            .iter()
            .filter(|accelerator| accelerator_allowed(accelerator, model, runtime))
            .collect();

        if compatible.is_empty() {
            rejected.push(RejectionIR {
                candidate: format!("{}@{}", runtime.id, node.id),
                code: "no_compatible_accelerator".into(),
                reason: format!(
                    "node {} has no accelerator compatible with runtime {} and model {}",
                    node.id, runtime.id, model.id
                ),
            });
            continue;
        }

        let mut any_single_fit = false;

        for accelerator in &compatible {
            let usable = accelerator.usable_memory_gb();

            if usable >= required {
                any_single_fit = true;
                feasible.push(PlanIR {
                    id: format!(
                        "{}:{}:{}:{:?}",
                        node.id,
                        accelerator.id,
                        runtime.id,
                        PlacementKind::SingleHost
                    ),
                    placement: PlacementKind::SingleHost,
                    runtime: runtime.id.clone(),
                    nodes: vec![node.id.clone()],
                    accelerators: vec![AcceleratorRefIR {
                        node: node.id.clone(),
                        accelerator: accelerator.id.clone(),
                        backend: accelerator.backend,
                    }],
                    required_memory_gb: required,
                    accelerator_memory_gb: usable,
                    relative_compute: accelerator.relative_compute,
                    hourly_cost_usd: node.hourly_cost_usd,
                    memory_headroom_gb: usable - required,
                    assumptions: vec![
                        "single-device feasibility uses the selected accelerator only".into(),
                        "v0.1 makes no latency or throughput claim".into(),
                    ],
                });
            } else if runtime.supports_cpu_offload {
                let capacity = usable + node.ram_gb * 0.75;
                if capacity >= required {
                    feasible.push(PlanIR {
                        id: format!(
                            "{}:{}:{}:{:?}",
                            node.id,
                            accelerator.id,
                            runtime.id,
                            PlacementKind::CpuOffload
                        ),
                        placement: PlacementKind::CpuOffload,
                        runtime: runtime.id.clone(),
                        nodes: vec![node.id.clone()],
                        accelerators: vec![AcceleratorRefIR {
                            node: node.id.clone(),
                            accelerator: accelerator.id.clone(),
                            backend: accelerator.backend,
                        }],
                        required_memory_gb: required,
                        accelerator_memory_gb: usable,
                        relative_compute: accelerator.relative_compute * 0.55,
                        hourly_cost_usd: node.hourly_cost_usd,
                        memory_headroom_gb: capacity - required,
                        assumptions: vec![
                            "CPU offload feasibility uses one explicitly selected accelerator"
                                .into(),
                            "75% of system RAM is treated as structurally available for offload"
                                .into(),
                            "v0.1 does not predict offload throughput".into(),
                        ],
                    });
                }
            }
        }

        if !any_single_fit && runtime.supports_tp {
            enumerate_local_tp(
                infra,
                node.id.as_str(),
                &compatible,
                model,
                runtime,
                feasible,
                rejected,
            );
        }

        if !feasible.iter().any(|plan| {
            plan.runtime == runtime.id && plan.nodes.len() == 1 && plan.nodes[0] == node.id
        }) {
            rejected.push(RejectionIR {
                candidate: format!("{}@{}", runtime.id, node.id),
                code: "insufficient_memory".into(),
                reason: format!(
                    "no compatible single-device, local-TP, or supported offload placement on node {} can fit {:.1}GB",
                    node.id, required
                ),
            });
        }
    }
}

fn enumerate_local_tp(
    infra: &InfrastructureIR,
    node_id: &str,
    compatible: &[&AcceleratorIR],
    model: &ModelIR,
    runtime: &RuntimeIR,
    feasible: &mut Vec<PlanIR>,
    rejected: &mut Vec<RejectionIR>,
) {
    let mut by_backend: HashMap<AcceleratorBackend, Vec<&AcceleratorIR>> = HashMap::new();
    for accelerator in compatible {
        by_backend
            .entry(accelerator.backend)
            .or_default()
            .push(*accelerator);
    }

    for (backend, mut accelerators) in by_backend {
        if accelerators.len() < 2 {
            continue;
        }

        accelerators.sort_by(|a, b| {
            b.usable_memory_gb()
                .total_cmp(&a.usable_memory_gb())
                .then_with(|| a.id.cmp(&b.id))
        });

        let mut selected = Vec::new();
        let mut memory = 0.0;
        for accelerator in accelerators {
            selected.push(accelerator);
            memory += accelerator.usable_memory_gb();
            if memory >= model.required_memory_gb() {
                break;
            }
        }

        if selected.len() < 2 || memory < model.required_memory_gb() {
            continue;
        }

        let missing_link = selected.iter().enumerate().find_map(|(i, left)| {
            selected.iter().skip(i + 1).find_map(|right| {
                if infra
                    .accelerator_link(node_id, &left.id, &right.id)
                    .is_none()
                {
                    Some((left.id.clone(), right.id.clone()))
                } else {
                    None
                }
            })
        });

        if let Some((left, right)) = missing_link {
            rejected.push(RejectionIR {
                candidate: format!("{}@{}:{:?}", runtime.id, node_id, backend),
                code: "missing_local_fabric".into(),
                reason: format!(
                    "local TP requires a discovered accelerator path between {node_id}/{left} and {node_id}/{right}"
                ),
            });
            continue;
        }

        let accelerator_ids = selected
            .iter()
            .map(|accelerator| accelerator.id.clone())
            .collect::<Vec<_>>();
        let relative_compute = selected
            .iter()
            .map(|accelerator| accelerator.relative_compute)
            .sum();

        feasible.push(PlanIR {
            id: format!(
                "{}:{}:{}:{:?}",
                node_id,
                accelerator_ids.join("+"),
                runtime.id,
                PlacementKind::TensorParallel
            ),
            placement: PlacementKind::TensorParallel,
            runtime: runtime.id.clone(),
            nodes: vec![node_id.to_string()],
            accelerators: accelerator_ids
                .iter()
                .map(|accelerator| AcceleratorRefIR {
                    node: node_id.to_string(),
                    accelerator: accelerator.clone(),
                    backend,
                })
                .collect(),
            required_memory_gb: model.required_memory_gb(),
            accelerator_memory_gb: memory,
            relative_compute,
            hourly_cost_usd: infra
                .node(node_id)
                .map(|node| node.hourly_cost_usd)
                .unwrap_or_default(),
            memory_headroom_gb: memory - model.required_memory_gb(),
            assumptions: vec![
                format!("local TP uses explicit {:?} accelerators", backend),
                "all selected accelerator pairs have a discovered local fabric relation".into(),
                "local fabric performance is not yet predicted in v0.1".into(),
            ],
        });
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

            let Some(link) = infra.node_link(&a.id, &b.id) else {
                rejected.push(RejectionIR {
                    candidate,
                    code: "missing_link".into(),
                    reason: "no discovered node-level fabric edge between nodes".into(),
                });
                continue;
            };

            let (Some(latency_ms), Some(bandwidth_gbps)) = (link.latency_ms, link.bandwidth_gbps)
            else {
                rejected.push(RejectionIR {
                    candidate,
                    code: "unmeasured_link".into(),
                    reason: "fabric relation is known but bandwidth/latency have not been measured"
                        .into(),
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

            let pair = best_cross_node_pair(a, b, model, runtime);
            let Some((accelerator_a, accelerator_b)) = pair else {
                rejected.push(RejectionIR {
                    candidate,
                    code: "cross_node_backend_mismatch".into(),
                    reason: "no matching runtime/model-compatible accelerator backend exists across both nodes".into(),
                });
                continue;
            };

            let memory = accelerator_a.usable_memory_gb() + accelerator_b.usable_memory_gb();
            if memory < model.required_memory_gb() {
                rejected.push(RejectionIR {
                    candidate,
                    code: "insufficient_pair_memory".into(),
                    reason: format!(
                        "selected pair exposes {:.1}GB but model requires {:.1}GB",
                        memory,
                        model.required_memory_gb()
                    ),
                });
                continue;
            }

            feasible.push(PlanIR {
                id: format!(
                    "{}/{}+{}/{}:{}:{:?}",
                    a.id,
                    accelerator_a.id,
                    b.id,
                    accelerator_b.id,
                    runtime.id,
                    PlacementKind::TensorParallel
                ),
                placement: PlacementKind::TensorParallel,
                runtime: runtime.id.clone(),
                nodes: vec![a.id.clone(), b.id.clone()],
                accelerators: vec![
                    AcceleratorRefIR {
                        node: a.id.clone(),
                        accelerator: accelerator_a.id.clone(),
                        backend: accelerator_a.backend,
                    },
                    AcceleratorRefIR {
                        node: b.id.clone(),
                        accelerator: accelerator_b.id.clone(),
                        backend: accelerator_b.backend,
                    },
                ],
                required_memory_gb: model.required_memory_gb(),
                accelerator_memory_gb: memory,
                relative_compute: accelerator_a.relative_compute + accelerator_b.relative_compute,
                hourly_cost_usd: a.hourly_cost_usd + b.hourly_cost_usd,
                memory_headroom_gb: memory - model.required_memory_gb(),
                assumptions: vec![format!(
                    "cross-node TP uses explicit devices over measured {:.1}Gbps / {:.1}ms fabric",
                    bandwidth_gbps, latency_ms
                )],
            });
        }
    }
}

fn best_cross_node_pair<'a>(
    a: &'a crate::ir::HardwareNodeIR,
    b: &'a crate::ir::HardwareNodeIR,
    model: &ModelIR,
    runtime: &RuntimeIR,
) -> Option<(&'a AcceleratorIR, &'a AcceleratorIR)> {
    let mut candidates = Vec::new();

    for accelerator_a in &a.accelerators {
        if !accelerator_allowed(accelerator_a, model, runtime) {
            continue;
        }
        for accelerator_b in &b.accelerators {
            if accelerator_a.backend != accelerator_b.backend
                || !accelerator_allowed(accelerator_b, model, runtime)
            {
                continue;
            }
            candidates.push((accelerator_a, accelerator_b));
        }
    }

    candidates.into_iter().max_by(|(a1, b1), (a2, b2)| {
        (a1.usable_memory_gb() + b1.usable_memory_gb())
            .total_cmp(&(a2.usable_memory_gb() + b2.usable_memory_gb()))
    })
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
        .flat_map(|plan| plan.nodes.iter().map(String::as_str))
        .collect();

    infra
        .nodes
        .iter()
        .filter(|node| !used.contains(node.id.as_str()))
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
                suggested_role:
                    "replica, batch worker, fallback endpoint, or a future placement candidate".into(),
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
                        accelerators: vec![
                            AcceleratorIR {
                                id: "h0".into(),
                                backend: AcceleratorBackend::Cuda,
                                memory_gb: 80.0,
                                free_memory_gb: Some(78.0),
                                relative_compute: 10.0,
                            },
                            AcceleratorIR {
                                id: "h1".into(),
                                backend: AcceleratorBackend::Cuda,
                                memory_gb: 80.0,
                                free_memory_gb: Some(77.0),
                                relative_compute: 9.5,
                            },
                        ],
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
                links: vec![
                    FabricEdgeIR {
                        from: FabricEndpointIR::Accelerator {
                            node: "local".into(),
                            accelerator: "g0".into(),
                        },
                        to: FabricEndpointIR::Accelerator {
                            node: "local".into(),
                            accelerator: "g1".into(),
                        },
                        kind: LinkKind::Nvlink,
                        bandwidth_gbps: None,
                        latency_ms: None,
                        jitter_ms: 0.0,
                        egress_cost_usd_per_gb: 0.0,
                    },
                    FabricEdgeIR {
                        from: FabricEndpointIR::Node {
                            node: "local".into(),
                        },
                        to: FabricEndpointIR::Node {
                            node: "remote".into(),
                        },
                        kind: LinkKind::Wan,
                        bandwidth_gbps: Some(10.0),
                        latency_ms: Some(35.0),
                        jitter_ms: 1.0,
                        egress_cost_usd_per_gb: 0.0,
                    },
                ],
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
    fn local_tp_uses_explicit_devices() {
        let report = solve(&scenario());
        let plan = report
            .feasible
            .iter()
            .find(|plan| {
                plan.runtime == "vllm"
                    && plan.placement == PlacementKind::TensorParallel
                    && plan.nodes == vec!["local".to_string()]
            })
            .unwrap();

        assert_eq!(plan.accelerators.len(), 2);
        assert_eq!(plan.accelerator_memory_gb, 46.0);
        assert_eq!(plan.memory_headroom_gb, 0.0);
    }

    #[test]
    fn single_gpu_plan_does_not_claim_all_host_vram() {
        let report = solve(&scenario());
        let plan = report
            .feasible
            .iter()
            .find(|plan| {
                plan.runtime == "vllm"
                    && plan.placement == PlacementKind::SingleHost
                    && plan.nodes == vec!["remote".to_string()]
                    && plan.accelerators[0].accelerator == "h0"
            })
            .unwrap();

        assert_eq!(plan.accelerators.len(), 1);
        assert_eq!(plan.accelerator_memory_gb, 78.0);
        assert_eq!(plan.memory_headroom_gb, 32.0);
    }

    #[test]
    fn slow_wan_pair_is_rejected_for_tp() {
        let report = solve(&scenario());
        assert!(report
            .rejected
            .iter()
            .any(|rejection| rejection.code == "fabric_too_slow"));
    }

    #[test]
    fn metal_node_can_be_feasible_through_llama_cpp() {
        let report = solve(&scenario());
        assert!(report.feasible.iter().any(|plan| {
            plan.nodes == vec!["mac".to_string()]
                && plan.runtime == "llama.cpp"
                && plan.accelerators.len() == 1
        }));
    }

    #[test]
    fn pareto_frontier_is_not_empty() {
        let report = solve(&scenario());
        assert!(!report.pareto.is_empty());
    }
}
