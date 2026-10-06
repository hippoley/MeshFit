use serde::{Deserialize, Serialize};

use crate::{
    benchmark::BenchmarkBundle,
    ir::{PlacementKind, PlanIR},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCalibrationDirection {
    UnderPrediction,
    OverPrediction,
    Exact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryCalibrationSample {
    pub benchmark_id: String,
    pub predicted_required_memory_gb: f64,
    pub observed_peak_vram_gb: f64,
    pub signed_error_gb: f64,
    pub absolute_error_gb: f64,
    pub absolute_percentage_error_fraction: f64,
    pub observed_to_predicted_ratio: f64,
    pub direction: MemoryCalibrationDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryCalibrationSummary {
    pub plan_id: String,
    pub placement: PlacementKind,
    pub sample_count: usize,
    pub predicted_required_memory_gb: f64,
    pub observed_mean_peak_vram_gb: f64,
    pub observed_max_peak_vram_gb: f64,
    #[serde(default)]
    pub observed_stddev_peak_vram_gb: Option<f64>,
    pub mean_signed_error_gb: f64,
    pub mean_absolute_error_gb: f64,
    pub mean_absolute_percentage_error_fraction: f64,
    pub mean_observed_to_predicted_ratio: f64,
    pub conservative_observed_to_predicted_ratio: f64,
    pub worst_underprediction_gb: f64,
    pub samples: Vec<MemoryCalibrationSample>,
}

pub fn calibrate_plan_memory(
    plan: &PlanIR,
    bundles: &[BenchmarkBundle],
) -> Result<MemoryCalibrationSummary, String> {
    if bundles.is_empty() {
        return Err("memory calibration requires at least one benchmark bundle".into());
    }

    if plan.required_memory_gb <= 0.0 {
        return Err("plan required_memory_gb must be greater than zero".into());
    }

    match plan.placement {
        PlacementKind::SingleHost | PlacementKind::TensorParallel => {}
        other => {
            return Err(format!(
                "memory calibration does not yet support placement {:?}; only single_host and tensor_parallel have a comparable total VRAM observation contract",
                other
            ));
        }
    }

    let mut samples = Vec::with_capacity(bundles.len());

    for bundle in bundles {
        bundle.validate()?;

        if bundle.request.executable.source_plan_id != plan.id {
            return Err(format!(
                "benchmark '{}' belongs to plan '{}' instead of calibration plan '{}'",
                bundle.benchmark_id, bundle.request.executable.source_plan_id, plan.id
            ));
        }

        if bundle.request.executable.placement != plan.placement {
            return Err(format!(
                "benchmark '{}' placement {:?} does not match plan placement {:?}",
                bundle.benchmark_id, bundle.request.executable.placement, plan.placement
            ));
        }

        let observed_peak_vram_gb = bundle
            .waves
            .iter()
            .filter_map(|wave| wave.peak_vram_gb)
            .max_by(f64::total_cmp)
            .ok_or_else(|| {
                format!(
                    "benchmark '{}' contains no observed peak VRAM; calibration cannot invent it",
                    bundle.benchmark_id
                )
            })?;

        if observed_peak_vram_gb <= 0.0 {
            return Err(format!(
                "benchmark '{}' has non-positive peak VRAM",
                bundle.benchmark_id
            ));
        }

        let signed_error_gb = plan.required_memory_gb - observed_peak_vram_gb;
        let absolute_error_gb = signed_error_gb.abs();
        let absolute_percentage_error_fraction = absolute_error_gb / observed_peak_vram_gb;
        let observed_to_predicted_ratio = observed_peak_vram_gb / plan.required_memory_gb;
        let direction = if signed_error_gb < 0.0 {
            MemoryCalibrationDirection::UnderPrediction
        } else if signed_error_gb > 0.0 {
            MemoryCalibrationDirection::OverPrediction
        } else {
            MemoryCalibrationDirection::Exact
        };

        samples.push(MemoryCalibrationSample {
            benchmark_id: bundle.benchmark_id.clone(),
            predicted_required_memory_gb: plan.required_memory_gb,
            observed_peak_vram_gb,
            signed_error_gb,
            absolute_error_gb,
            absolute_percentage_error_fraction,
            observed_to_predicted_ratio,
            direction,
        });
    }

    let observed = samples
        .iter()
        .map(|sample| sample.observed_peak_vram_gb)
        .collect::<Vec<_>>();
    let signed_errors = samples
        .iter()
        .map(|sample| sample.signed_error_gb)
        .collect::<Vec<_>>();
    let absolute_errors = samples
        .iter()
        .map(|sample| sample.absolute_error_gb)
        .collect::<Vec<_>>();
    let apes = samples
        .iter()
        .map(|sample| sample.absolute_percentage_error_fraction)
        .collect::<Vec<_>>();
    let ratios = samples
        .iter()
        .map(|sample| sample.observed_to_predicted_ratio)
        .collect::<Vec<_>>();

    let observed_mean_peak_vram_gb = mean(&observed)?;
    let observed_max_peak_vram_gb = observed
        .iter()
        .copied()
        .max_by(f64::total_cmp)
        .ok_or_else(|| "memory calibration contains no observed samples".to_string())?;

    Ok(MemoryCalibrationSummary {
        plan_id: plan.id.clone(),
        placement: plan.placement,
        sample_count: samples.len(),
        predicted_required_memory_gb: plan.required_memory_gb,
        observed_mean_peak_vram_gb,
        observed_max_peak_vram_gb,
        observed_stddev_peak_vram_gb: sample_stddev(&observed),
        mean_signed_error_gb: mean(&signed_errors)?,
        mean_absolute_error_gb: mean(&absolute_errors)?,
        mean_absolute_percentage_error_fraction: mean(&apes)?,
        mean_observed_to_predicted_ratio: mean(&ratios)?,
        conservative_observed_to_predicted_ratio: observed_max_peak_vram_gb
            / plan.required_memory_gb,
        worst_underprediction_gb: (observed_max_peak_vram_gb - plan.required_memory_gb).max(0.0),
        samples,
    })
}

fn mean(values: &[f64]) -> Result<f64, String> {
    if values.is_empty() {
        return Err("cannot compute mean of empty values".into());
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
        ir::{AcceleratorBackend, AcceleratorRefIR},
    };

    fn plan() -> PlanIR {
        PlanIR {
            id: "node-b:gpu0:vllm:SingleHost".into(),
            placement: PlacementKind::SingleHost,
            runtime: "vllm".into(),
            nodes: vec!["node-b".into()],
            accelerators: vec![AcceleratorRefIR {
                node: "node-b".into(),
                accelerator: "gpu0".into(),
                backend: AcceleratorBackend::Cuda,
            }],
            required_memory_gb: 20.0,
            accelerator_memory_gb: 78.0,
            relative_compute: 10.0,
            hourly_cost_usd: 3.5,
            memory_headroom_gb: 58.0,
            communication: None,
            assumptions: vec![],
        }
    }

    fn bundle(id: &str, plan_id: &str, peak_vram_gb: Option<f64>) -> BenchmarkBundle {
        BenchmarkBundle {
            benchmark_id: id.into(),
            request: BenchmarkRequestIR {
                executable: ExecutablePlanIR {
                    source_plan_id: plan_id.into(),
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
                    working_node: "node-b".into(),
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
                total_ms: 1000.0,
                output_tokens: Some(10),
            }],
            waves: vec![WaveMeasurement {
                request_count: 1,
                total_ms: 1000.0,
                output_tokens: Some(10),
                peak_vram_gb,
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
    fn reports_underprediction_and_conservative_ratio() {
        let summary = calibrate_plan_memory(
            &plan(),
            &[
                bundle("run-1", "node-b:gpu0:vllm:SingleHost", Some(22.0)),
                bundle("run-2", "node-b:gpu0:vllm:SingleHost", Some(24.0)),
            ],
        )
        .unwrap();

        assert_eq!(summary.sample_count, 2);
        assert_eq!(summary.observed_mean_peak_vram_gb, 23.0);
        assert_eq!(summary.observed_max_peak_vram_gb, 24.0);
        assert_eq!(summary.mean_signed_error_gb, -3.0);
        assert_eq!(summary.worst_underprediction_gb, 4.0);
        assert_eq!(summary.conservative_observed_to_predicted_ratio, 1.2);
        assert_eq!(
            summary.samples[0].direction,
            MemoryCalibrationDirection::UnderPrediction
        );
    }

    #[test]
    fn rejects_bundle_from_another_plan() {
        let error = calibrate_plan_memory(
            &plan(),
            &[bundle("run-1", "other-plan", Some(22.0))],
        )
        .unwrap_err();

        assert!(error.contains("instead of calibration plan"));
    }

    #[test]
    fn refuses_to_invent_missing_vram_observation() {
        let error = calibrate_plan_memory(
            &plan(),
            &[bundle("run-1", "node-b:gpu0:vllm:SingleHost", None)],
        )
        .unwrap_err();

        assert!(error.contains("contains no observed peak VRAM"));
    }

    #[test]
    fn cpu_offload_is_not_treated_as_total_vram_calibration() {
        let mut plan = plan();
        plan.placement = PlacementKind::CpuOffload;

        let error = calibrate_plan_memory(
            &plan,
            &[bundle("run-1", "node-b:gpu0:vllm:SingleHost", Some(22.0))],
        )
        .unwrap_err();

        assert!(error.contains("does not yet support placement"));
    }
}
