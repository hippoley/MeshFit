use serde::{Deserialize, Serialize};

use crate::identity::ExecutionIdentity;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    TtftMs,
    TpotMs,
    DecodeTokensPerSecond,
    PrefillTokensPerSecond,
    ThroughputTokensPerSecond,
    PeakVramGb,
    PeakRamGb,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricObservation {
    pub metric: MetricKind,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkProvenance {
    pub source: String,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub binary_sha256: Option<String>,
    #[serde(default)]
    pub captured_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkRecord {
    pub id: String,
    pub identity: ExecutionIdentity,
    pub context_tokens: u32,
    pub concurrency: u32,
    pub observations: Vec<MetricObservation>,
    pub provenance: BenchmarkProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EvidenceStore {
    #[serde(default)]
    pub records: Vec<BenchmarkRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PredictionStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceRef {
    pub benchmark_id: String,
    pub execution_fingerprint: String,
    pub source: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PredictionInterval {
    pub confidence_level: f64,
    pub lower: f64,
    pub upper: f64,
    pub method: String,
    pub sample_count: usize,
    pub lower_truncated_at_zero: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Prediction {
    pub metric: MetricKind,
    pub status: PredictionStatus,
    pub execution_fingerprint: String,
    pub mean: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    #[serde(default)]
    pub sample_standard_deviation: Option<f64>,
    #[serde(default)]
    pub coefficient_of_variation: Option<f64>,
    #[serde(default)]
    pub prediction_interval_95: Option<PredictionInterval>,
    pub sample_count: usize,
    pub confidence: f64,
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PredictionQuery {
    pub identity: ExecutionIdentity,
    pub context_tokens: u32,
    pub concurrency: u32,
    pub metric: MetricKind,
}

impl EvidenceStore {
    pub fn predict_exact(&self, query: &PredictionQuery) -> Prediction {
        let query_fingerprint = query.identity.fingerprint();
        let mut values = Vec::new();
        let mut evidence = Vec::new();

        for record in &self.records {
            if !matches_query(record, query, &query_fingerprint) {
                continue;
            }

            let record_fingerprint = record.identity.fingerprint();

            for observation in &record.observations {
                if observation.metric == query.metric {
                    values.push(observation.value);
                    evidence.push(EvidenceRef {
                        benchmark_id: record.id.clone(),
                        execution_fingerprint: record_fingerprint.clone(),
                        source: record.provenance.source.clone(),
                        source_url: record.provenance.source_url.clone(),
                    });
                }
            }
        }

        if values.is_empty() {
            return Prediction {
                metric: query.metric,
                status: PredictionStatus::Unavailable,
                execution_fingerprint: query_fingerprint,
                mean: None,
                min: None,
                max: None,
                sample_standard_deviation: None,
                coefficient_of_variation: None,
                prediction_interval_95: None,
                sample_count: 0,
                confidence: 0.0,
                evidence,
                explanation:
                    "no exact benchmark evidence matches execution identity/context/concurrency"
                        .into(),
            };
        }

        let sample_count = values.len();
        let sum: f64 = values.iter().sum();
        let mean = sum / sample_count as f64;
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let sample_standard_deviation = sample_standard_deviation(&values);
        let coefficient_of_variation = sample_standard_deviation
            .filter(|_| mean != 0.0)
            .map(|stddev| stddev / mean.abs());
        let prediction_interval_95 = next_observation_prediction_interval_95(&values);

        Prediction {
            metric: query.metric,
            status: PredictionStatus::Available,
            execution_fingerprint: query_fingerprint,
            mean: Some(mean),
            min: Some(min),
            max: Some(max),
            sample_standard_deviation,
            coefficient_of_variation,
            prediction_interval_95,
            sample_count,
            confidence: evidence_confidence(sample_count),
            evidence,
            explanation: "exact execution-identity empirical prediction; no cross-hardware extrapolation applied".into(),
        }
    }
}

fn sample_standard_deviation(values: &[f64]) -> Option<f64> {
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

fn student_t_975(df: usize) -> f64 {
    const TABLE: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045, 2.042,
    ];

    if df == 0 {
        f64::NAN
    } else if df <= TABLE.len() {
        TABLE[df - 1]
    } else {
        1.96
    }
}

fn next_observation_prediction_interval_95(values: &[f64]) -> Option<PredictionInterval> {
    let stddev = sample_standard_deviation(values)?;
    let sample_count = values.len();
    let mean = values.iter().sum::<f64>() / sample_count as f64;
    let t = student_t_975(sample_count - 1);
    let margin = t * stddev * (1.0 + 1.0 / sample_count as f64).sqrt();
    let raw_lower = mean - margin;
    let lower_truncated_at_zero = raw_lower < 0.0;

    Some(PredictionInterval {
        confidence_level: 0.95,
        lower: raw_lower.max(0.0),
        upper: mean + margin,
        method: "student_t_next_observation".into(),
        sample_count,
        lower_truncated_at_zero,
    })
}

fn matches_query(
    record: &BenchmarkRecord,
    query: &PredictionQuery,
    query_fingerprint: &str,
) -> bool {
    record.identity.fingerprint() == query_fingerprint
        && record.context_tokens == query.context_tokens
        && record.concurrency == query.concurrency
}

fn evidence_confidence(sample_count: usize) -> f64 {
    match sample_count {
        0 => 0.0,
        1 => 0.45,
        2 => 0.60,
        3..=4 => 0.72,
        5..=9 => 0.82,
        _ => 0.90,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        identity::{
            DeviceIdentity, HardwareIdentity, LinkIdentity, ModelArtifactIdentity, RuntimeIdentity,
            TopologyIdentity,
        },
        ir::{AcceleratorBackend, LinkKind, PlacementKind},
    };

    fn identity() -> ExecutionIdentity {
        ExecutionIdentity {
            hardware: HardwareIdentity {
                architecture: "x86_64".into(),
                operating_system: "linux".into(),
                cpu_model: Some("EPYC".into()),
                ram_mib: Some(262_144),
                devices: vec![
                    DeviceIdentity {
                        vendor: "nvidia".into(),
                        model: "H100".into(),
                        backend: AcceleratorBackend::Cuda,
                        memory_mib: 81_920,
                        driver_version: Some("580.65".into()),
                    },
                    DeviceIdentity {
                        vendor: "nvidia".into(),
                        model: "H100".into(),
                        backend: AcceleratorBackend::Cuda,
                        memory_mib: 81_920,
                        driver_version: Some("580.65".into()),
                    },
                ],
            },
            model: ModelArtifactIdentity {
                model_id: "qwen-72b-q4".into(),
                format: "safetensors".into(),
                quantization: "awq4".into(),
                artifact_sha256: Some("model-sha".into()),
                revision: Some("main".into()),
            },
            runtime: RuntimeIdentity {
                runtime: "vllm".into(),
                version: "0.10.2".into(),
                build_commit: None,
                flags: vec!["--tensor-parallel-size=2".into()],
            },
            topology: TopologyIdentity {
                links: vec![LinkIdentity {
                    from: "gpu0".into(),
                    to: "gpu1".into(),
                    kind: LinkKind::Nvlink,
                    bandwidth_mbps: Some(900_000),
                    latency_micros: Some(5),
                }],
            },
            placement: PlacementKind::TensorParallel,
        }
    }

    fn store() -> EvidenceStore {
        EvidenceStore {
            records: vec![
                BenchmarkRecord {
                    id: "run-1".into(),
                    identity: identity(),
                    context_tokens: 32768,
                    concurrency: 20,
                    observations: vec![MetricObservation {
                        metric: MetricKind::DecodeTokensPerSecond,
                        value: 68.0,
                    }],
                    provenance: BenchmarkProvenance {
                        source: "local-bench".into(),
                        source_url: None,
                        commit: Some("abc".into()),
                        binary_sha256: None,
                        captured_at: None,
                    },
                },
                BenchmarkRecord {
                    id: "run-2".into(),
                    identity: identity(),
                    context_tokens: 32768,
                    concurrency: 20,
                    observations: vec![MetricObservation {
                        metric: MetricKind::DecodeTokensPerSecond,
                        value: 72.0,
                    }],
                    provenance: BenchmarkProvenance {
                        source: "local-bench".into(),
                        source_url: None,
                        commit: Some("def".into()),
                        binary_sha256: None,
                        captured_at: None,
                    },
                },
            ],
        }
    }

    #[test]
    fn exact_evidence_returns_empirical_range() {
        let prediction = store().predict_exact(&PredictionQuery {
            identity: identity(),
            context_tokens: 32768,
            concurrency: 20,
            metric: MetricKind::DecodeTokensPerSecond,
        });

        assert_eq!(prediction.status, PredictionStatus::Available);
        assert_eq!(prediction.sample_count, 2);
        assert_eq!(prediction.mean, Some(70.0));
        assert_eq!(prediction.min, Some(68.0));
        assert_eq!(prediction.max, Some(72.0));
        assert!(prediction.sample_standard_deviation.is_some());
        assert!(prediction.coefficient_of_variation.is_some());
        let interval = prediction.prediction_interval_95.as_ref().unwrap();
        assert_eq!(interval.confidence_level, 0.95);
        assert_eq!(interval.method, "student_t_next_observation");
        assert_eq!(interval.sample_count, 2);
        assert!(interval.lower < 68.0);
        assert!(interval.upper > 72.0);
        assert_eq!(prediction.evidence.len(), 2);
    }

    #[test]
    fn one_sample_does_not_invent_prediction_interval() {
        let mut store = store();
        store.records.truncate(1);

        let prediction = store.predict_exact(&PredictionQuery {
            identity: identity(),
            context_tokens: 32768,
            concurrency: 20,
            metric: MetricKind::DecodeTokensPerSecond,
        });

        assert_eq!(prediction.sample_count, 1);
        assert_eq!(prediction.sample_standard_deviation, None);
        assert_eq!(prediction.coefficient_of_variation, None);
        assert_eq!(prediction.prediction_interval_95, None);
    }

    #[test]
    fn t_interval_is_wide_for_only_two_samples() {
        let interval = next_observation_prediction_interval_95(&[68.0, 72.0]).unwrap();

        assert!(interval.upper - interval.lower > 40.0);
        assert!(!interval.lower_truncated_at_zero);
    }

    #[test]
    fn runtime_change_breaks_exact_match() {
        let mut changed = identity();
        changed.runtime.version = "0.11.0".into();

        let prediction = store().predict_exact(&PredictionQuery {
            identity: changed,
            context_tokens: 32768,
            concurrency: 20,
            metric: MetricKind::DecodeTokensPerSecond,
        });

        assert_eq!(prediction.status, PredictionStatus::Unavailable);
        assert_eq!(prediction.sample_count, 0);
        assert_eq!(prediction.mean, None);
    }
}
