use std::collections::{HashMap, HashSet};

use crate::ir::{
    AcceleratorBackend, AcceleratorIR, AcceleratorRefIR, CommunicationEstimateIR, EvidenceGapIR,
    EvidenceGapKind, ExclusionIR, InfrastructureIR, ModelIR, PlacementKind, PlacementReport, PlanIR,
    RejectionIR, RuntimeIR, ScenarioIR, WorkloadIR,
};

const TP_EFFECTIVE_BANDWIDTH_FACTOR: f64 = 0.70;

struct PlacementContext<'a> {
    model: &'a ModelIR,
    workload: &'a WorkloadIR,
    required_memory_gb: f64,
    kv_cache_gb: f64,
}

pub fn solve(scenario: &ScenarioIR) -> PlacementReport {
    let mut feasible = Vec::new();
    let mut rejected = Vec::new();
    let mut evidence_gaps = Vec::new();
    let context = PlacementContext {
        model: &scenario.model,
        workload: &scenario.workload,
        required_memory_gb: scenario.model.required_memory_gb_for(&scenario.workload),
        kv_cache_gb: scenario.model.kv_cache_gb_for(&scenario.workload),
    };

    for runtime in &scenario.runtimes {
        enumerate_single_node(
            &scenario.infrastructure,
            &context,
            runtime,
            &mut feasible,
            &mut rejected,
            &mut evidence_gaps,
        );
        enumerate_two_node_tp(
            &scenario.infrastructure,
            &context,
            runtime,
            &mut feasible,
            &mut rejected,
            &mut evidence_gaps,
        );
    }

    let pareto = pareto_frontier(&feasible);
    let excluded_nodes = explain_exclusions(&scenario.infrastructure, &pareto);

    PlacementReport {
        model: scenario.model.id.clone(),
        feasible,
        pareto,
        rejected,
        evidence_gaps,
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
    context: &PlacementContext<'_>,
    runtime: &RuntimeIR,
    feasible: &mut Vec<PlanIR>,
    rejected: &mut Vec<RejectionIR>,
    evidence_gaps: &mut Vec<EvidenceGapIR>,
) {
    let model = context.model;
    let required = context.required_memory_gb;
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
                    communication: None,
                    assumptions: memory_assumptions(
                        context,
                        vec![
                            "single-device feasibility uses the selected accelerator only".into(),
                            "v0.1 makes no latency or throughput claim".into(),
                        ],
                    ),
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
                        communication: None,
                        assumptions: memory_assumptions(
                            context,
                            vec![
                                "CPU offload feasibility uses one explicitly selected accelerator"
                                    .into(),
                                "75% of system RAM is treated as structurally available for offload"
                                    .into(),
                                "v0.1 does not predict offload throughput".into(),
                            ],
                        ),
                    });
                }
            }
        }

        if !any_single_fit && runtime.supports_tp {
            enumerate_local_tp(
                infra,
                node.id.as_str(),
                &compatible,
                context,
                runtime,
                feasible,
                rejected,
                evidence_gaps,
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
    context: &PlacementContext<'_>,
    runtime: &RuntimeIR,
    feasible: &mut Vec<PlanIR>,
    rejected: &mut Vec<RejectionIR>,
    evidence_gaps: &mut Vec<EvidenceGapIR>,
) {
    let required = context.required_memory_gb;
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

        let selected = (2..=accelerators.len()).find_map(|count| {
            let shard_required = required / count as f64;
            let candidate = &accelerators[..count];

            if candidate
                .iter()
                .all(|accelerator| accelerator.usable_memory_gb() >= shard_required)
            {
                Some(candidate.to_vec())
            } else {
                None
            }
        });

        let Some(selected) = selected else {
            rejected.push(RejectionIR {
                candidate: format!("{}@{}:{:?}", runtime.id, node_id, backend),
                code: "tp_shard_does_not_fit".into(),
                reason: format!(
                    "aggregate local accelerator memory is insufficiently balanced for an equal TP shard of {:.1}GB total",
                    required
                ),
            });
            continue;
        };

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
            let candidate = format!("{}@{}:{:?}", runtime.id, node_id, backend);
            evidence_gaps.push(EvidenceGapIR {
                kind: EvidenceGapKind::DiscoverLocalFabric,
                candidate: candidate.clone(),
                nodes: vec![node_id.to_string()],
                accelerators: vec![
                    AcceleratorRefIR {
                        node: node_id.to_string(),
                        accelerator: left.clone(),
                        backend,
                    },
                    AcceleratorRefIR {
                        node: node_id.to_string(),
                        accelerator: right.clone(),
                        backend,
                    },
                ],
                missing_fields: vec!["local_fabric_path".into()],
            });
            rejected.push(RejectionIR {
                candidate,
                code: "missing_local_fabric".into(),
                reason: format!(
                    "local TP requires a discovered accelerator path between {node_id}/{left} and {node_id}/{right}"
                ),
            });
            continue;
        }

        let tp_size = selected.len();
        let shard_required = required / tp_size as f64;
        let memory = selected
            .iter()
            .map(|accelerator| accelerator.usable_memory_gb())
            .sum();
        let bottleneck_headroom = selected
            .iter()
            .map(|accelerator| accelerator.usable_memory_gb() - shard_required)
            .fold(f64::INFINITY, f64::min);

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
            required_memory_gb: required,
            accelerator_memory_gb: memory,
            relative_compute,
            hourly_cost_usd: infra
                .node(node_id)
                .map(|node| node.hourly_cost_usd)
                .unwrap_or_default(),
            memory_headroom_gb: bottleneck_headroom,
            communication: None,
            assumptions: memory_assumptions(
                context,
                vec![
                    format!(
                        "local TP uses {} explicit {:?} accelerators with {:.1}GB equal structural shards",
                        tp_size, backend, shard_required
                    ),
                    "all selected accelerator pairs have a discovered local fabric relation".into(),
                    "TP feasibility is gated by the weakest selected device, not aggregate VRAM"
                        .into(),
                    "local fabric performance is not yet predicted in v0.1".into(),
                ],
            ),
        });
    }
}

fn enumerate_two_node_tp(
    infra: &InfrastructureIR,
    context: &PlacementContext<'_>,
    runtime: &RuntimeIR,
    feasible: &mut Vec<PlanIR>,
    rejected: &mut Vec<RejectionIR>,
    evidence_gaps: &mut Vec<EvidenceGapIR>,
) {
    let model = context.model;
    let required = context.required_memory_gb;
    if !runtime.supports_tp {
        return;
    }

    for i in 0..infra.nodes.len() {
        for j in (i + 1)..infra.nodes.len() {
            let a = &infra.nodes[i];
            let b = &infra.nodes[j];
            let candidate = format!("{}@{}+{}", runtime.id, a.id, b.id);

            let Some(link) = infra.node_link(&a.id, &b.id) else {
                evidence_gaps.push(EvidenceGapIR {
                    kind: EvidenceGapKind::DiscoverPeerLink,
                    candidate: candidate.clone(),
                    nodes: vec![a.id.clone(), b.id.clone()],
                    accelerators: vec![],
                    missing_fields: vec!["peer_link".into()],
                });
                rejected.push(RejectionIR {
                    candidate,
                    code: "missing_link".into(),
                    reason: "no discovered node-level fabric edge between nodes".into(),
                });
                continue;
            };

            let (Some(latency_ms), Some(bandwidth_gbps)) = (link.latency_ms, link.bandwidth_gbps)
            else {
                let mut missing_fields = Vec::new();
                if link.latency_ms.is_none() {
                    missing_fields.push("latency_ms".into());
                }
                if link.bandwidth_gbps.is_none() {
                    missing_fields.push("bandwidth_gbps".into());
                }
                evidence_gaps.push(EvidenceGapIR {
                    kind: EvidenceGapKind::MeasurePeerLink,
                    candidate: candidate.clone(),
                    nodes: vec![a.id.clone(), b.id.clone()],
                    accelerators: vec![],
                    missing_fields,
                });
                rejected.push(RejectionIR {
                    candidate,
                    code: "unmeasured_link".into(),
                    reason: "fabric relation is known but bandwidth/latency have not been measured"
                        .into(),
                });
                continue;
            };

            let pair = best_cross_node_pair(a, b, model, runtime);
            let Some((accelerator_a, accelerator_b)) = pair else {
                rejected.push(RejectionIR {
                    candidate,
                    code: "cross_node_backend_mismatch".into(),
                    reason: "no matching runtime/model-compatible accelerator backend exists across both nodes".into(),
                });
                continue;
            };

            let Some(communication_profile) = &model.tp_communication_model else {
                evidence_gaps.push(EvidenceGapIR {
                    kind: EvidenceGapKind::SupplyTpCommunicationProfile,
                    candidate: candidate.clone(),
                    nodes: vec![a.id.clone(), b.id.clone()],
                    accelerators: vec![],
                    missing_fields: vec!["model.tp_communication_model".into()],
                });
                rejected.push(RejectionIR {
                    candidate,
                    code: "missing_tp_communication_profile".into(),
                    reason: "cross-node TP requires an explicit model communication profile; MeshFit will not infer one silently".into(),
                });
                continue;
            };

            let Some(communication_budget_ms) = context.workload.max_tp_communication_ms_per_token
            else {
                evidence_gaps.push(EvidenceGapIR {
                    kind: EvidenceGapKind::SupplyTpCommunicationBudget,
                    candidate: candidate.clone(),
                    nodes: vec![a.id.clone(), b.id.clone()],
                    accelerators: vec![],
                    missing_fields: vec!["workload.max_tp_communication_ms_per_token".into()],
                });
                rejected.push(RejectionIR {
                    candidate,
                    code: "missing_tp_communication_budget".into(),
                    reason: "cross-node TP requires a workload communication budget in ms/token before it can be admitted".into(),
                });
                continue;
            };

            let communication = estimate_tp_communication(
                communication_profile,
                2,
                bandwidth_gbps,
                latency_ms,
                link.egress_cost_usd_per_gb,
            );

            if communication.total_ms_per_token > communication_budget_ms {
                rejected.push(RejectionIR {
                    candidate,
                    code: "tp_communication_budget_exceeded".into(),
                    reason: format!(
                        "estimated TP communication tax is {:.3}ms/token, above the workload budget of {:.3}ms/token ({:.3}ms transfer + {:.3}ms synchronization)",
                        communication.total_ms_per_token,
                        communication_budget_ms,
                        communication.transfer_ms_per_token,
                        communication.synchronization_ms_per_token
                    ),
                });
                continue;
            }

            let memory = accelerator_a.usable_memory_gb() + accelerator_b.usable_memory_gb();
            let shard_required = required / 2.0;
            let bottleneck_headroom = accelerator_a
                .usable_memory_gb()
                .min(accelerator_b.usable_memory_gb())
                - shard_required;

            if bottleneck_headroom < 0.0 {
                rejected.push(RejectionIR {
                    candidate,
                    code: "tp_shard_does_not_fit".into(),
                    reason: format!(
                        "cross-node TP requires each device to hold an equal {:.1}GB shard; selected devices expose {:.1}GB and {:.1}GB",
                        shard_required,
                        accelerator_a.usable_memory_gb(),
                        accelerator_b.usable_memory_gb()
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
                required_memory_gb: required,
                accelerator_memory_gb: memory,
                relative_compute: accelerator_a.relative_compute + accelerator_b.relative_compute,
                hourly_cost_usd: a.hourly_cost_usd + b.hourly_cost_usd,
                memory_headroom_gb: bottleneck_headroom,
                communication: Some(communication.clone()),
                assumptions: memory_assumptions(
                    context,
                    vec![
                        format!(
                            "cross-node TP uses explicit devices over measured {:.1}Gbps / {:.1}ms fabric",
                            bandwidth_gbps, latency_ms
                        ),
                        format!(
                            "analytical TP communication tax {:.3}ms/token is within the {:.3}ms/token workload budget",
                            communication.total_ms_per_token, communication_budget_ms
                        ),
                        format!(
                            "cross-node TP uses {:.1}GB equal structural shards and is gated by the weaker device",
                            shard_required
                        ),
                    ],
                ),
            });
        }
    }
}

fn estimate_tp_communication(
    profile: &crate::ir::TpCommunicationModelIR,
    tp_size: usize,
    bandwidth_gbps: f64,
    latency_ms: f64,
    egress_cost_usd_per_gb: f64,
) -> CommunicationEstimateIR {
    let bytes_per_token = profile.bytes_per_token_for_tp_size(tp_size);
    let synchronizations_per_token = profile.synchronizations_per_token();
    let effective_bandwidth_gbps = bandwidth_gbps * TP_EFFECTIVE_BANDWIDTH_FACTOR;
    let effective_bytes_per_second = effective_bandwidth_gbps * 1_000_000_000.0 / 8.0;
    let transfer_ms_per_token = bytes_per_token / effective_bytes_per_second * 1000.0;
    let synchronization_ms_per_token = latency_ms * synchronizations_per_token as f64;
    let total_ms_per_token = transfer_ms_per_token + synchronization_ms_per_token;
    let egress_cost_usd_per_million_tokens =
        bytes_per_token * 1_000_000.0 / 1_000_000_000.0 * egress_cost_usd_per_gb;

    CommunicationEstimateIR {
        bytes_per_token,
        effective_bandwidth_gbps,
        transfer_ms_per_token,
        synchronizations_per_token,
        synchronization_ms_per_token,
        total_ms_per_token,
        egress_cost_usd_per_million_tokens,
        confidence: "analytical_unvalidated".into(),
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

fn memory_assumptions(context: &PlacementContext<'_>, mut extra: Vec<String>) -> Vec<String> {
    let model = context.model;
    let workload = context.workload;
    let kv_cache_gb = context.kv_cache_gb;
    let mut assumptions = vec![format!(
        "memory = {:.1}GB weights + {:.1}GB KV = {:.1}GB total",
        model.weight_memory_gb, kv_cache_gb, context.required_memory_gb
    )];

    if model.kv_cache_model.is_some() {
        assumptions.push(format!(
            "KV cache is workload-aware at {} context tokens and {} active sequences",
            workload.context_tokens,
            workload.active_sequences()
        ));
    } else {
        assumptions.push(
            "KV cache uses legacy fixed kv_cache_gb fallback and is not workload-scaled".into(),
        );
    }

    assumptions.append(&mut extra);
    assumptions
}

fn communication_objective(plan: &PlanIR) -> Option<f64> {
    match plan.placement {
        PlacementKind::SingleHost | PlacementKind::CpuOffload | PlacementKind::Replica => Some(0.0),
        _ => plan
            .communication
            .as_ref()
            .map(|estimate| estimate.total_ms_per_token),
    }
}

fn communication_dominance(other: &PlanIR, candidate: &PlanIR) -> (bool, bool) {
    match (
        communication_objective(other),
        communication_objective(candidate),
    ) {
        (Some(other_ms), Some(candidate_ms)) => (other_ms <= candidate_ms, other_ms < candidate_ms),
        (Some(_), None) => (true, true),
        (None, Some(_)) => (false, false),
        (None, None) => (true, false),
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
                let (no_more_communication, better_communication) =
                    communication_dominance(other, candidate);
                let strictly_better = other.hourly_cost_usd < candidate.hourly_cost_usd
                    || other.relative_compute > candidate.relative_compute
                    || other.memory_headroom_gb > candidate.memory_headroom_gb
                    || better_communication;

                no_more_expensive
                    && no_less_compute
                    && no_less_headroom
                    && no_more_communication
                    && strictly_better
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
                kv_cache_model: None,
                tp_communication_model: Some(TpCommunicationModelIR::BytesPerToken {
                    bytes_per_token: 1_000_000,
                    synchronizations_per_token: 2,
                }),
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
                max_active_sequences: None,
                max_tp_communication_ms_per_token: Some(2.0),
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
    fn workload_aware_kv_can_change_single_gpu_feasibility() {
        let mut low = scenario();
        low.model.weight_memory_gb = 42.0;
        low.model.kv_cache_gb = 0.0;
        low.model.kv_cache_model = Some(KvCacheModelIR::BytesPerToken {
            bytes_per_token: 500_000,
        });
        low.workload.context_tokens = 4_096;
        low.workload.concurrency = 1;
        low.workload.max_active_sequences = Some(1);

        let low_report = solve(&low);
        assert!(low_report.feasible.iter().any(|plan| {
            plan.runtime == "vllm"
                && plan.placement == PlacementKind::SingleHost
                && plan.nodes == vec!["remote".to_string()]
        }));

        let mut high = low.clone();
        high.workload.context_tokens = 32_768;
        high.workload.concurrency = 4;
        high.workload.max_active_sequences = Some(4);

        let high_report = solve(&high);
        assert!(!high_report.feasible.iter().any(|plan| {
            plan.runtime == "vllm"
                && plan.placement == PlacementKind::SingleHost
                && plan.nodes == vec!["remote".to_string()]
        }));
    }

    #[test]
    fn local_tp_rejects_unbalanced_devices_even_when_aggregate_memory_fits() {
        let mut scenario = scenario();
        scenario.model.weight_memory_gb = 100.0;
        scenario.model.kv_cache_gb = 0.0;
        scenario.model.kv_cache_model = None;
        scenario.infrastructure.nodes[0].accelerators[0].free_memory_gb = Some(80.0);
        scenario.infrastructure.nodes[0].accelerators[1].free_memory_gb = Some(24.0);

        let report = solve(&scenario);

        assert!(!report.feasible.iter().any(|plan| {
            plan.runtime == "vllm"
                && plan.placement == PlacementKind::TensorParallel
                && plan.nodes == vec!["local".to_string()]
        }));
        assert!(report.rejected.iter().any(|rejection| {
            rejection.code == "tp_shard_does_not_fit"
                && rejection.candidate.starts_with("vllm@local")
        }));
    }

    #[test]
    fn cross_node_tp_rejects_unbalanced_pair_even_when_aggregate_memory_fits() {
        let mut scenario = scenario();
        scenario.model.weight_memory_gb = 100.0;
        scenario.model.kv_cache_gb = 0.0;
        scenario.model.kv_cache_model = None;

        scenario.infrastructure.nodes[0].accelerators[0].free_memory_gb = Some(80.0);
        scenario.infrastructure.nodes[0].accelerators[1].free_memory_gb = Some(20.0);
        scenario.infrastructure.nodes[1].accelerators[0].free_memory_gb = Some(24.0);
        scenario.infrastructure.nodes[1].accelerators[1].free_memory_gb = Some(23.0);

        let link = scenario
            .infrastructure
            .node_link("local", "remote")
            .unwrap()
            .clone();
        let index = scenario
            .infrastructure
            .links
            .iter()
            .position(|candidate| candidate == &link)
            .unwrap();
        scenario.infrastructure.links[index].bandwidth_gbps = Some(100.0);
        scenario.infrastructure.links[index].latency_ms = Some(0.5);

        let report = solve(&scenario);

        assert!(!report.feasible.iter().any(|plan| {
            plan.runtime == "vllm"
                && plan.placement == PlacementKind::TensorParallel
                && plan.nodes.len() == 2
        }));
        assert!(report.rejected.iter().any(|rejection| {
            rejection.code == "tp_shard_does_not_fit"
                && rejection.candidate.starts_with("vllm@local+remote")
        }));
    }

    #[test]
    fn cross_node_tp_requires_explicit_communication_profile() {
        let mut scenario = scenario();
        scenario.model.tp_communication_model = None;
        scenario.infrastructure.links[1].bandwidth_gbps = Some(100.0);
        scenario.infrastructure.links[1].latency_ms = Some(0.1);

        let report = solve(&scenario);

        assert!(report.rejected.iter().any(|rejection| {
            rejection.code == "missing_tp_communication_profile"
                && rejection.candidate.starts_with("vllm@local+remote")
        }));
    }

    #[test]
    fn cross_node_tp_requires_explicit_communication_budget() {
        let mut scenario = scenario();
        scenario.workload.max_tp_communication_ms_per_token = None;
        scenario.infrastructure.links[1].bandwidth_gbps = Some(100.0);
        scenario.infrastructure.links[1].latency_ms = Some(0.1);

        let report = solve(&scenario);

        assert!(report.rejected.iter().any(|rejection| {
            rejection.code == "missing_tp_communication_budget"
                && rejection.candidate.starts_with("vllm@local+remote")
        }));
    }

    #[test]
    fn fast_cross_node_tp_carries_structured_communication_estimate() {
        let mut scenario = scenario();
        scenario.infrastructure.links[1].bandwidth_gbps = Some(100.0);
        scenario.infrastructure.links[1].latency_ms = Some(0.1);

        let report = solve(&scenario);
        let plan = report
            .feasible
            .iter()
            .find(|plan| {
                plan.runtime == "vllm"
                    && plan.placement == PlacementKind::TensorParallel
                    && plan.nodes.len() == 2
            })
            .unwrap();

        let communication = plan.communication.as_ref().unwrap();
        assert_eq!(communication.synchronizations_per_token, 2);
        assert!(communication.bytes_per_token > 0.0);
        assert!(communication.transfer_ms_per_token > 0.0);
        assert!(communication.synchronization_ms_per_token > 0.0);
        assert!(communication.total_ms_per_token < 2.0);
        assert_eq!(communication.confidence, "analytical_unvalidated");
    }

    #[test]
    fn slow_wan_pair_is_rejected_for_tp() {
        let report = solve(&scenario());
        assert!(report
            .rejected
            .iter()
            .any(|rejection| rejection.code == "tp_communication_budget_exceeded"));
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

    fn communication_estimate(total_ms_per_token: f64) -> CommunicationEstimateIR {
        CommunicationEstimateIR {
            bytes_per_token: 1_000_000.0,
            effective_bandwidth_gbps: 70.0,
            transfer_ms_per_token: total_ms_per_token / 2.0,
            synchronizations_per_token: 1,
            synchronization_ms_per_token: total_ms_per_token / 2.0,
            total_ms_per_token,
            egress_cost_usd_per_million_tokens: 0.0,
            confidence: "analytical_unvalidated".into(),
        }
    }

    fn pareto_plan(id: &str, communication: Option<CommunicationEstimateIR>) -> PlanIR {
        PlanIR {
            id: id.into(),
            placement: PlacementKind::TensorParallel,
            runtime: "vllm".into(),
            nodes: vec!["a".into(), "b".into()],
            accelerators: vec![],
            required_memory_gb: 40.0,
            accelerator_memory_gb: 80.0,
            relative_compute: 10.0,
            hourly_cost_usd: 1.0,
            memory_headroom_gb: 20.0,
            communication,
            assumptions: vec![],
        }
    }

    #[test]
    fn pareto_prefers_lower_known_communication_tax_when_other_objectives_match() {
        let fast = pareto_plan("fast", Some(communication_estimate(0.2)));
        let slow = pareto_plan("slow", Some(communication_estimate(1.5)));

        let frontier = pareto_frontier(&[fast.clone(), slow]);

        assert_eq!(frontier, vec![fast]);
    }

    #[test]
    fn known_communication_evidence_dominates_unknown_when_other_objectives_match() {
        let known = pareto_plan("known", Some(communication_estimate(0.8)));
        let unknown = pareto_plan("unknown", None);

        let frontier = pareto_frontier(&[known.clone(), unknown]);

        assert_eq!(frontier, vec![known]);
    }

    #[test]
    fn pareto_frontier_is_not_empty() {
        let report = solve(&scenario());
        assert!(!report.pareto.is_empty());
    }
}
