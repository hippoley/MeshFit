use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ir::{EvidenceGapIR, EvidenceGapKind, PlacementReport};

pub use crate::ir::EvidenceGapKind as ProbeKind;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbeRecommendation {
    pub kind: ProbeKind,
    pub priority: u32,
    pub affected_candidates: Vec<String>,
    pub targets: Vec<EvidenceGapIR>,
    pub evidence_gap: String,
    pub why_it_can_change_decision: String,
    pub suggested_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbePlan {
    pub model: String,
    pub decision_blocked: bool,
    pub recommendations: Vec<ProbeRecommendation>,
    pub non_probe_rejections: Vec<String>,
}

#[derive(Debug)]
struct ProbeTemplate {
    base_priority: u32,
    evidence_gap: &'static str,
    why: &'static str,
    action: &'static str,
}

pub fn recommend_probes(report: &PlacementReport) -> ProbePlan {
    let decision_blocked = report.feasible.is_empty();
    let mut grouped: BTreeMap<EvidenceGapKind, Vec<EvidenceGapIR>> = BTreeMap::new();

    for gap in &report.evidence_gaps {
        grouped.entry(gap.kind).or_default().push(gap.clone());
    }

    let evidence_gap_candidates = report
        .evidence_gaps
        .iter()
        .map(|gap| gap.candidate.as_str())
        .collect::<BTreeSet<_>>();
    let non_probe_rejections = report
        .rejected
        .iter()
        .filter(|rejection| !evidence_gap_candidates.contains(rejection.candidate.as_str()))
        .map(|rejection| rejection.code.clone())
        .collect::<BTreeSet<_>>();

    let mut recommendations = grouped
        .into_iter()
        .map(|(kind, mut targets)| {
            let template = probe_template(kind);
            targets.sort_by(|left, right| {
                left.candidate
                    .cmp(&right.candidate)
                    .then_with(|| left.nodes.cmp(&right.nodes))
            });
            let affected_candidates = targets
                .iter()
                .map(|gap| gap.candidate.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let breadth_bonus = ((affected_candidates.len().saturating_sub(1)) as u32 * 5).min(20);
            let blocked_bonus = if decision_blocked { 10 } else { 0 };

            ProbeRecommendation {
                kind,
                priority: template.base_priority + breadth_bonus + blocked_bonus,
                affected_candidates,
                targets,
                evidence_gap: template.evidence_gap.into(),
                why_it_can_change_decision: template.why.into(),
                suggested_action: template.action.into(),
            }
        })
        .collect::<Vec<_>>();

    recommendations.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.kind.cmp(&right.kind))
    });

    ProbePlan {
        model: report.model.clone(),
        decision_blocked,
        recommendations,
        non_probe_rejections: non_probe_rejections.into_iter().collect(),
    }
}

fn probe_template(kind: EvidenceGapKind) -> ProbeTemplate {
    match kind {
        ProbeKind::MeasurePeerLink => ProbeTemplate {
            base_priority: 100,
            evidence_gap: "Peer relation exists but bandwidth and/or latency are unmeasured.",
            why: "A measured RTT/bandwidth pair can move cross-node TP from unknown to admissible or conclusively reject it.",
            action: "Run meshfit probe <peer> --bandwidth from the source host and merge the result into the snapshot.",
        },
        ProbeKind::DiscoverPeerLink => ProbeTemplate {
            base_priority: 90,
            evidence_gap: "No discovered node-level fabric edge exists for a rejected cross-node candidate.",
            why: "Discovering a real peer path is required before MeshFit can reason about cross-node communication at all.",
            action: "Discover or declare the peer link, then measure it with meshfit probe <peer> --bandwidth.",
        },
        ProbeKind::DiscoverLocalFabric => ProbeTemplate {
            base_priority: 85,
            evidence_gap: "Selected local accelerators have no discovered PCIe/NVLink relation.",
            why: "Local TP is deliberately rejected until MeshFit knows the selected devices are connected by a real local fabric path.",
            action: "Re-run local discovery with accelerator topology available (for NVIDIA, ensure nvidia-smi topo -m works).",
        },
        ProbeKind::SupplyTpCommunicationProfile => ProbeTemplate {
            base_priority: 75,
            evidence_gap: "The model has no explicit tensor-parallel communication profile.",
            why: "Without bytes/token and synchronization behavior, MeshFit cannot estimate communication tax and will not silently guess.",
            action: "Add tp_communication_model to the placement target using measured bytes/token or a documented transformer-shape approximation.",
        },
        ProbeKind::SupplyTpCommunicationBudget => ProbeTemplate {
            base_priority: 65,
            evidence_gap: "The workload has no maximum TP communication budget in ms/token.",
            why: "MeshFit needs an explicit decision boundary before an analytical communication estimate can admit cross-node TP.",
            action: "Set workload.max_tp_communication_ms_per_token from the workload SLO or benchmark-derived budget.",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{EvidenceGapIR, EvidenceGapKind, PlacementReport, RejectionIR};

    fn report(
        rejections: Vec<(&str, &str)>,
        gaps: Vec<EvidenceGapIR>,
        has_feasible: bool,
    ) -> PlacementReport {
        let feasible = if has_feasible {
            vec![crate::ir::PlanIR {
                id: "fallback".into(),
                placement: crate::ir::PlacementKind::SingleHost,
                runtime: "vllm".into(),
                nodes: vec!["a".into()],
                accelerators: vec![],
                required_memory_gb: 1.0,
                accelerator_memory_gb: 2.0,
                relative_compute: 1.0,
                hourly_cost_usd: 0.0,
                memory_headroom_gb: 1.0,
                communication: None,
                assumptions: vec![],
            }]
        } else {
            vec![]
        };

        PlacementReport {
            model: "demo".into(),
            feasible,
            pareto: vec![],
            rejected: rejections
                .into_iter()
                .map(|(candidate, code)| RejectionIR {
                    candidate: candidate.into(),
                    code: code.into(),
                    reason: code.into(),
                })
                .collect(),
            evidence_gaps: gaps,
            excluded_nodes: vec![],
        }
    }

    fn gap(kind: EvidenceGapKind, candidate: &str, nodes: &[&str]) -> EvidenceGapIR {
        EvidenceGapIR {
            kind,
            candidate: candidate.into(),
            nodes: nodes.iter().map(|node| (*node).to_string()).collect(),
            accelerators: vec![],
            missing_fields: vec!["test".into()],
        }
    }

    #[test]
    fn recommends_only_evidence_gaps_not_structural_failures() {
        let plan = recommend_probes(&report(
            vec![
                ("vllm@a+b", "unmeasured_link"),
                ("vllm@a", "insufficient_memory"),
                ("vllm@b", "tp_shard_does_not_fit"),
            ],
            vec![gap(
                EvidenceGapKind::MeasurePeerLink,
                "vllm@a+b",
                &["a", "b"],
            )],
            false,
        ));

        assert_eq!(plan.recommendations.len(), 1);
        assert_eq!(plan.recommendations[0].kind, ProbeKind::MeasurePeerLink);
        assert_eq!(
            plan.non_probe_rejections,
            vec!["insufficient_memory", "tp_shard_does_not_fit"]
        );
    }

    #[test]
    fn deduplicates_probe_kind_and_rewards_candidate_breadth() {
        let plan = recommend_probes(&report(
            vec![
                ("vllm@a+b", "unmeasured_link"),
                ("vllm@a+c", "unmeasured_link"),
                ("vllm@a+d", "missing_tp_communication_budget"),
            ],
            vec![
                gap(EvidenceGapKind::MeasurePeerLink, "vllm@a+b", &["a", "b"]),
                gap(EvidenceGapKind::MeasurePeerLink, "vllm@a+c", &["a", "c"]),
                gap(
                    EvidenceGapKind::SupplyTpCommunicationBudget,
                    "vllm@a+d",
                    &["a", "d"],
                ),
            ],
            false,
        ));

        assert_eq!(plan.recommendations.len(), 2);
        assert_eq!(plan.recommendations[0].kind, ProbeKind::MeasurePeerLink);
        assert_eq!(plan.recommendations[0].affected_candidates.len(), 2);
        assert_eq!(plan.recommendations[0].targets[0].nodes[0], "a");
        assert!(plan.recommendations[0].priority > plan.recommendations[1].priority);
    }

    #[test]
    fn blocked_decision_gets_priority_boost() {
        let blocked = recommend_probes(&report(
            vec![("vllm@a+b", "unmeasured_link")],
            vec![gap(
                EvidenceGapKind::MeasurePeerLink,
                "vllm@a+b",
                &["a", "b"],
            )],
            false,
        ));
        let not_blocked = recommend_probes(&report(
            vec![("vllm@a+b", "unmeasured_link")],
            vec![gap(
                EvidenceGapKind::MeasurePeerLink,
                "vllm@a+b",
                &["a", "b"],
            )],
            true,
        ));

        assert_eq!(
            blocked.recommendations[0].priority,
            not_blocked.recommendations[0].priority + 10
        );
    }
}
