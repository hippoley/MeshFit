use serde::{Deserialize, Serialize};

use crate::ir::PlanIR;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CostScope {
    DeclaredMarginal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanCostEstimate {
    pub plan_id: String,
    pub scope: CostScope,
    pub predicted_output_tokens_per_second: f64,
    pub declared_compute_hourly_cost_usd: f64,
    pub compute_cost_per_million_output_tokens_usd: f64,
    pub communication_egress_cost_per_million_tokens_usd: f64,
    pub total_declared_marginal_cost_per_million_output_tokens_usd: f64,
    pub assumptions: Vec<String>,
}

pub fn estimate_plan_cost(
    plan: &PlanIR,
    predicted_output_tokens_per_second: f64,
) -> Result<PlanCostEstimate, String> {
    if predicted_output_tokens_per_second <= 0.0 {
        return Err("predicted output tokens/s must be greater than zero".into());
    }
    if plan.hourly_cost_usd < 0.0 {
        return Err("plan hourly cost cannot be negative".into());
    }
    if plan.nodes.len() > 1 && plan.communication.is_none() {
        return Err(
            "cross-node cost estimation requires a communication estimate; MeshFit will not assume zero egress"
                .into(),
        );
    }

    let compute_cost_per_million_output_tokens_usd = plan.hourly_cost_usd
        / (predicted_output_tokens_per_second * 3600.0)
        * 1_000_000.0;
    let communication_egress_cost_per_million_tokens_usd = plan
        .communication
        .as_ref()
        .map(|communication| communication.egress_cost_usd_per_million_tokens)
        .unwrap_or(0.0);

    let mut assumptions = vec![
        "cost scope is declared marginal execution cost, not full TCO".into(),
        "compute cost uses the plan's declared hourly cost and explicit predicted output throughput"
            .into(),
        "communication egress uses the PlanIR communication estimate when present".into(),
    ];

    if plan.hourly_cost_usd == 0.0 {
        assumptions.push(
            "declared compute hourly cost is zero; this does not imply hardware ownership, power, labor, depreciation, or facility cost is zero"
                .into(),
        );
    }

    Ok(PlanCostEstimate {
        plan_id: plan.id.clone(),
        scope: CostScope::DeclaredMarginal,
        predicted_output_tokens_per_second,
        declared_compute_hourly_cost_usd: plan.hourly_cost_usd,
        compute_cost_per_million_output_tokens_usd,
        communication_egress_cost_per_million_tokens_usd,
        total_declared_marginal_cost_per_million_output_tokens_usd:
            compute_cost_per_million_output_tokens_usd
                + communication_egress_cost_per_million_tokens_usd,
        assumptions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        AcceleratorBackend, AcceleratorRefIR, CommunicationEstimateIR, PlacementKind,
    };

    fn plan() -> PlanIR {
        PlanIR {
            id: "plan-a".into(),
            placement: PlacementKind::SingleHost,
            runtime: "vllm".into(),
            nodes: vec!["node-a".into()],
            accelerators: vec![AcceleratorRefIR {
                node: "node-a".into(),
                accelerator: "gpu0".into(),
                backend: AcceleratorBackend::Cuda,
            }],
            required_memory_gb: 40.0,
            accelerator_memory_gb: 80.0,
            relative_compute: 10.0,
            hourly_cost_usd: 3.6,
            memory_headroom_gb: 40.0,
            communication: None,
            assumptions: vec![],
        }
    }

    #[test]
    fn converts_declared_hourly_cost_to_cost_per_million_output_tokens() {
        let estimate = estimate_plan_cost(&plan(), 100.0).unwrap();

        assert_eq!(estimate.compute_cost_per_million_output_tokens_usd, 10.0);
        assert_eq!(
            estimate.total_declared_marginal_cost_per_million_output_tokens_usd,
            10.0
        );
    }

    #[test]
    fn includes_modeled_communication_egress() {
        let mut plan = plan();
        plan.nodes = vec!["node-a".into(), "node-b".into()];
        plan.communication = Some(CommunicationEstimateIR {
            bytes_per_token: 1_000_000.0,
            effective_bandwidth_gbps: 70.0,
            transfer_ms_per_token: 0.1,
            synchronizations_per_token: 2,
            synchronization_ms_per_token: 0.2,
            total_ms_per_token: 0.3,
            egress_cost_usd_per_million_tokens: 2.5,
            confidence: "analytical_unvalidated".into(),
        });

        let estimate = estimate_plan_cost(&plan, 100.0).unwrap();

        assert_eq!(estimate.compute_cost_per_million_output_tokens_usd, 10.0);
        assert_eq!(
            estimate.communication_egress_cost_per_million_tokens_usd,
            2.5
        );
        assert_eq!(
            estimate.total_declared_marginal_cost_per_million_output_tokens_usd,
            12.5
        );
    }

    #[test]
    fn refuses_cross_node_cost_without_communication_evidence() {
        let mut plan = plan();
        plan.nodes = vec!["node-a".into(), "node-b".into()];

        let error = estimate_plan_cost(&plan, 100.0).unwrap_err();

        assert!(error.contains("will not assume zero egress"));
    }

    #[test]
    fn zero_hourly_cost_is_explicitly_not_total_cost_of_ownership() {
        let mut plan = plan();
        plan.hourly_cost_usd = 0.0;

        let estimate = estimate_plan_cost(&plan, 100.0).unwrap();

        assert_eq!(estimate.total_declared_marginal_cost_per_million_output_tokens_usd, 0.0);
        assert!(estimate
            .assumptions
            .iter()
            .any(|assumption| assumption.contains("does not imply hardware ownership")));
    }
}
