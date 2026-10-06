use serde::{Deserialize, Serialize};

use crate::{benchmark::{BenchmarkBundle, BenchmarkConfig}, ir::PlanIR};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostCalibrationSample {
    pub benchmark_id: String,
    pub observed_output_tokens_per_second: f64,
    pub predicted_total_cost_per_million_output_tokens_usd: f64,
    pub observed_compute_cost_per_million_output_tokens_usd: f64,
    pub observed_total_cost_per_million_output_tokens_usd: f64,
    pub signed_error_usd_per_million_output_tokens: f64,
    pub absolute_error_usd_per_million_output_tokens: f64,
    pub absolute_percentage_error_fraction: f64,
    pub observed_to_predicted_ratio: f64,
    pub underprediction_fraction: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostCalibrationSummary {
    pub plan_id: String,
    pub execution_fingerprint: String,
    pub context_tokens: u32,
    pub concurrency: u32,
    pub benchmark_config: BenchmarkConfig,
    pub sample_count: usize,
    pub predicted_output_tokens_per_second: f64,
    pub predicted_total_cost_per_million_output_tokens_usd: f64,
    pub communication_egress_cost_per_million_tokens_usd: f64,
    pub communication_egress_is_modeled: bool,
    pub observed_mean_output_tokens_per_second: f64,
    #[serde(default)]
    pub observed_stddev_output_tokens_per_second: Option<f64>,
    pub observed_mean_total_cost_per_million_output_tokens_usd: f64,
    pub mean_signed_error_usd_per_million_output_tokens: f64,
    pub mean_absolute_error_usd_per_million_output_tokens: f64,
    pub mean_absolute_percentage_error_fraction: f64,
    pub conservative_observed_to_predicted_ratio: f64,
    pub worst_underprediction_fraction: f64,
    pub samples: Vec<CostCalibrationSample>,
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

    let compute_cost_per_million_output_tokens_usd =
        plan.hourly_cost_usd / (predicted_output_tokens_per_second * 3600.0) * 1_000_000.0;
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

pub fn calibrate_plan_cost(
    plan: &PlanIR,
    predicted_output_tokens_per_second: f64,
    bundles: &[BenchmarkBundle],
) -> Result<CostCalibrationSummary, String> {
    if bundles.is_empty() {
        return Err("cost calibration requires at least one benchmark bundle".into());
    }

    let prediction = estimate_plan_cost(plan, predicted_output_tokens_per_second)?;
    if prediction.total_declared_marginal_cost_per_million_output_tokens_usd <= 0.0 {
        return Err(
            "cost calibration requires a non-zero declared marginal cost; zero-cost plans do not have a meaningful relative cost error"
                .into(),
        );
    }

    let anchor = &bundles[0];
    anchor.validate()?;
    validate_cost_bundle_plan(plan, anchor)?;

    let execution_fingerprint = anchor.request.identity.fingerprint();
    let context_tokens = anchor.request.context_tokens;
    let concurrency = anchor.request.concurrency;
    let benchmark_config = anchor.request.config.clone();

    let mut samples = Vec::with_capacity(bundles.len());
    for bundle in bundles {
        bundle.validate()?;
        validate_cost_bundle_plan(plan, bundle)?;

        if bundle.request.identity.fingerprint() != execution_fingerprint {
            return Err(format!(
                "benchmark '{}' execution fingerprint differs from the calibration anchor",
                bundle.benchmark_id
            ));
        }
        if bundle.request.context_tokens != context_tokens {
            return Err(format!(
                "benchmark '{}' context differs from the calibration anchor",
                bundle.benchmark_id
            ));
        }
        if bundle.request.concurrency != concurrency {
            return Err(format!(
                "benchmark '{}' concurrency differs from the calibration anchor",
                bundle.benchmark_id
            ));
        }
        if bundle.request.config != benchmark_config {
            return Err(format!(
                "benchmark '{}' BenchmarkConfig differs from the calibration anchor",
                bundle.benchmark_id
            ));
        }

        let observed_output_tokens_per_second = observed_bundle_output_tokens_per_second(bundle)?;
        let observed_compute_cost_per_million_output_tokens_usd = plan.hourly_cost_usd
            / (observed_output_tokens_per_second * 3600.0)
            * 1_000_000.0;
        let observed_total_cost_per_million_output_tokens_usd =
            observed_compute_cost_per_million_output_tokens_usd
                + prediction.communication_egress_cost_per_million_tokens_usd;

        if observed_total_cost_per_million_output_tokens_usd <= 0.0 {
            return Err(format!(
                "benchmark '{}' produces a non-positive declared marginal observed cost",
                bundle.benchmark_id
            ));
        }

        let predicted_total =
            prediction.total_declared_marginal_cost_per_million_output_tokens_usd;
        let signed_error =
            predicted_total - observed_total_cost_per_million_output_tokens_usd;
        let absolute_error = signed_error.abs();
        let absolute_percentage_error_fraction =
            absolute_error / observed_total_cost_per_million_output_tokens_usd;
        let observed_to_predicted_ratio =
            observed_total_cost_per_million_output_tokens_usd / predicted_total;
        let underprediction_fraction =
            ((observed_total_cost_per_million_output_tokens_usd - predicted_total)
                / observed_total_cost_per_million_output_tokens_usd)
                .max(0.0);

        samples.push(CostCalibrationSample {
            benchmark_id: bundle.benchmark_id.clone(),
            observed_output_tokens_per_second,
            predicted_total_cost_per_million_output_tokens_usd: predicted_total,
            observed_compute_cost_per_million_output_tokens_usd,
            observed_total_cost_per_million_output_tokens_usd,
            signed_error_usd_per_million_output_tokens: signed_error,
            absolute_error_usd_per_million_output_tokens: absolute_error,
            absolute_percentage_error_fraction,
            observed_to_predicted_ratio,
            underprediction_fraction,
        });
    }

    let throughputs = samples
        .iter()
        .map(|sample| sample.observed_output_tokens_per_second)
        .collect::<Vec<_>>();
    let observed_costs = samples
        .iter()
        .map(|sample| sample.observed_total_cost_per_million_output_tokens_usd)
        .collect::<Vec<_>>();
    let signed_errors = samples
        .iter()
        .map(|sample| sample.signed_error_usd_per_million_output_tokens)
        .collect::<Vec<_>>();
    let absolute_errors = samples
        .iter()
        .map(|sample| sample.absolute_error_usd_per_million_output_tokens)
        .collect::<Vec<_>>();
    let apes = samples
        .iter()
        .map(|sample| sample.absolute_percentage_error_fraction)
        .collect::<Vec<_>>();
    let ratios = samples
        .iter()
        .map(|sample| sample.observed_to_predicted_ratio)
        .collect::<Vec<_>>();
    let underprediction = samples
        .iter()
        .map(|sample| sample.underprediction_fraction)
        .collect::<Vec<_>>();

    let mut assumptions = prediction.assumptions.clone();
    assumptions.push(
        "observed compute cost is calibrated from measured wave output throughput".into(),
    );
    assumptions.push(
        "communication egress remains modeled from PlanIR and is not an observed provider bill"
            .into(),
    );

    Ok(CostCalibrationSummary {
        plan_id: plan.id.clone(),
        execution_fingerprint,
        context_tokens,
        concurrency,
        benchmark_config,
        sample_count: samples.len(),
        predicted_output_tokens_per_second,
        predicted_total_cost_per_million_output_tokens_usd: prediction
            .total_declared_marginal_cost_per_million_output_tokens_usd,
        communication_egress_cost_per_million_tokens_usd: prediction
            .communication_egress_cost_per_million_tokens_usd,
        communication_egress_is_modeled: true,
        observed_mean_output_tokens_per_second: mean(&throughputs)?,
        observed_stddev_output_tokens_per_second: sample_stddev(&throughputs),
        observed_mean_total_cost_per_million_output_tokens_usd: mean(&observed_costs)?,
        mean_signed_error_usd_per_million_output_tokens: mean(&signed_errors)?,
        mean_absolute_error_usd_per_million_output_tokens: mean(&absolute_errors)?,
        mean_absolute_percentage_error_fraction: mean(&apes)?,
        conservative_observed_to_predicted_ratio: ratios
            .iter()
            .copied()
            .max_by(f64::total_cmp)
            .ok_or_else(|| "cost calibration contains no correction ratios".to_string())?,
        worst_underprediction_fraction: underprediction
            .iter()
            .copied()
            .max_by(f64::total_cmp)
            .unwrap_or(0.0),
        samples,
        assumptions,
    })
}

fn validate_cost_bundle_plan(plan: &PlanIR, bundle: &BenchmarkBundle) -> Result<(), String> {
    if bundle.request.executable.source_plan_id != plan.id {
        return Err(format!(
            "benchmark '{}' belongs to plan '{}' instead of cost plan '{}'",
            bundle.benchmark_id, bundle.request.executable.source_plan_id, plan.id
        ));
    }
    if bundle.request.executable.placement != plan.placement {
        return Err(format!(
            "benchmark '{}' placement {:?} does not match cost plan placement {:?}",
            bundle.benchmark_id, bundle.request.executable.placement, plan.placement
        ));
    }
    if bundle.request.executable.runtime != plan.runtime {
        return Err(format!(
            "benchmark '{}' runtime '{}' does not match cost plan runtime '{}'",
            bundle.benchmark_id, bundle.request.executable.runtime, plan.runtime
        ));
    }
    Ok(())
}

fn observed_bundle_output_tokens_per_second(bundle: &BenchmarkBundle) -> Result<f64, String> {
    let mut output_tokens = 0_u64;
    let mut total_seconds = 0.0;

    for wave in &bundle.waves {
        let tokens = wave.output_tokens.ok_or_else(|| {
            format!(
                "benchmark '{}' wave lacks output token usage; cost calibration cannot invent observed throughput",
                bundle.benchmark_id
            )
        })?;
        output_tokens += u64::from(tokens);
        total_seconds += wave.total_ms / 1000.0;
    }

    if output_tokens == 0 || total_seconds <= 0.0 {
        return Err(format!(
            "benchmark '{}' has no positive wave throughput for cost calibration",
            bundle.benchmark_id
        ));
    }

    Ok(output_tokens as f64 / total_seconds)
}

fn mean(values: &[f64]) -> Result<f64, String> {
    if values.is_empty() {
        return Err("cannot compute mean over zero cost calibration samples".into());
    }
    Ok(values.iter().sum::<f64>() / values.len() as f64)
}

fn sample_stddev(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f64>()
        / (values.len() - 1) as f64;
    Some(variance.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        benchmark::{BenchmarkConfig, BenchmarkRequestIR, RequestMeasurement, WaveMeasurement},
        compiler::{ExecutablePlanIR, ExecutionScope, ServiceContract},
        evidence::BenchmarkProvenance,
        identity::{
            ExecutionIdentity, HardwareIdentity, ModelArtifactIdentity, RuntimeIdentity,
            TopologyIdentity,
        },
        ir::{AcceleratorBackend, AcceleratorRefIR, CommunicationEstimateIR, PlacementKind},
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

    fn bundle(id: &str, output_tokens: Option<u32>, total_ms: f64) -> BenchmarkBundle {
        BenchmarkBundle {
            benchmark_id: id.into(),
            request: BenchmarkRequestIR {
                executable: ExecutablePlanIR {
                    source_plan_id: "plan-a".into(),
                    model_id: "demo".into(),
                    model_source: "/models/demo".into(),
                    placement: PlacementKind::SingleHost,
                    context_tokens: 4096,
                    runtime: "vllm".into(),
                    scope: ExecutionScope::LocalProcess,
                    program: "vllm".into(),
                    args: vec![],
                    identity_flags: vec![],
                    env: vec![],
                    working_node: "node-a".into(),
                    service: ServiceContract {
                        scheme: "http".into(),
                        host: "127.0.0.1".into(),
                        port: 18080,
                        health_path: "/health".into(),
                        chat_completions_path: "/v1/chat/completions".into(),
                    },
                    assumptions: vec![],
                },
                identity: ExecutionIdentity {
                    hardware: HardwareIdentity {
                        architecture: "x86_64".into(),
                        operating_system: "linux".into(),
                        cpu_model: None,
                        ram_mib: Some(65_536),
                        devices: vec![],
                    },
                    model: ModelArtifactIdentity {
                        model_id: "demo".into(),
                        format: "safetensors".into(),
                        quantization: "q4".into(),
                        artifact_sha256: Some("sha".into()),
                        revision: None,
                    },
                    runtime: RuntimeIdentity {
                        runtime: "vllm".into(),
                        version: "test".into(),
                        build_commit: None,
                        flags: vec![],
                    },
                    topology: TopologyIdentity { links: vec![] },
                    placement: PlacementKind::SingleHost,
                },
                context_tokens: 4096,
                concurrency: 1,
                config: BenchmarkConfig {
                    prompt: "hello".into(),
                    max_tokens: 64,
                    warmup_requests: 1,
                    measured_requests: 1,
                    request_timeout_ms: 120_000,
                    startup_timeout_ms: 300_000,
                },
            },
            measurements: vec![RequestMeasurement {
                ttft_ms: 100.0,
                total_ms,
                output_tokens,
            }],
            waves: vec![WaveMeasurement {
                request_count: 1,
                total_ms,
                output_tokens,
                peak_vram_gb: Some(20.0),
                peak_ram_gb: Some(8.0),
            }],
            provenance: BenchmarkProvenance {
                source: "test".into(),
                source_url: None,
                commit: Some("abc".into()),
                captured_at: None,
            },
        }
    }

    #[test]
    fn calibrates_cost_underprediction_from_observed_wave_throughput() {
        let summary = calibrate_plan_cost(
            &plan(),
            12.0,
            &[bundle("run-1", Some(10), 1000.0), bundle("run-2", Some(20), 2000.0)],
        )
        .unwrap();

        assert_eq!(summary.sample_count, 2);
        assert_eq!(summary.observed_mean_output_tokens_per_second, 10.0);
        assert!(summary.predicted_total_cost_per_million_output_tokens_usd < 100.0);
        assert_eq!(summary.observed_mean_total_cost_per_million_output_tokens_usd, 100.0);
        assert!(summary.mean_signed_error_usd_per_million_output_tokens < 0.0);
        assert!(summary.conservative_observed_to_predicted_ratio > 1.0);
        assert!(summary.worst_underprediction_fraction > 0.0);
        assert!(summary.communication_egress_is_modeled);
    }

    #[test]
    fn cost_calibration_refuses_missing_wave_usage() {
        let error = calibrate_plan_cost(&plan(), 12.0, &[bundle("run-1", None, 1000.0)])
            .unwrap_err();

        assert!(error.contains("cannot invent observed throughput"));
    }

    #[test]
    fn cost_calibration_rejects_execution_identity_drift() {
        let first = bundle("run-1", Some(10), 1000.0);
        let mut second = bundle("run-2", Some(10), 1000.0);
        second.request.identity.runtime.version = "different".into();

        let error = calibrate_plan_cost(&plan(), 12.0, &[first, second]).unwrap_err();

        assert!(error.contains("execution fingerprint differs"));
    }

    #[test]
    fn cost_calibration_rejects_benchmark_config_drift() {
        let first = bundle("run-1", Some(10), 1000.0);
        let mut second = bundle("run-2", Some(10), 1000.0);
        second.request.config.max_tokens = 128;

        let error = calibrate_plan_cost(&plan(), 12.0, &[first, second]).unwrap_err();

        assert!(error.contains("BenchmarkConfig differs"));
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

        assert_eq!(
            estimate.total_declared_marginal_cost_per_million_output_tokens_usd,
            0.0
        );
        assert!(estimate
            .assumptions
            .iter()
            .any(|assumption| assumption.contains("does not imply hardware ownership")));
    }
}
