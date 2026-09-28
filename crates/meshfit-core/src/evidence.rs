use serde::{Deserialize, Serialize};

use crate::ir::PlacementKind;

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
    pub captured_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkRecord {
    pub id: String,
    pub hardware_fingerprint: String,
    pub model_id: String,
    pub runtime: String,
    pub placement: PlacementKind,
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
    pub source: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Prediction {
    pub metric: MetricKind,
    pub status: PredictionStatus,
    pub mean: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub sample_count: usize,
    pub confidence: f64,
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PredictionQuery {
    pub hardware_fingerprint: String,
    pub model_id: String,
    pub runtime: String,
    pub placement: PlacementKind,
    pub context_tokens: u32,
    pub concurrency: u32,
    pub metric: MetricKind,
}

impl EvidenceStore {
    pub fn predict_exact(&self, query: &PredictionQuery) -> Prediction {
        let mut values = Vec::new();
        let mut evidence = Vec::new();

        for record in &self.records {
            if !matches_query(record, query) {
                continue;
            }

            for observation in &record.observations {
                if observation.metric == query.metric {
                    values.push(observation.value);
                    evidence.push(EvidenceRef {
                        benchmark_id: record.id.clone(),
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
                mean: None,
                min: None,
                max: None,
                sample_count: 0,
                confidence: 0.0,
                evidence,
                explanation: "no exact benchmark evidence matches hardware/model/runtime/placement/context/concurrency".into(),
            };
        }

        let sample_count = values.len();
        let sum: f64 = values.iter().sum();
        let mean = sum / sample_count as f64;
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        Prediction {
            metric: query.metric,
            status: PredictionStatus::Available,
            mean: Some(mean),
            min: Some(min),
            max: Some(max),
            sample_count,
            confidence: evidence_confidence(sample_count),
            evidence,
            explanation: "exact-match empirical prediction; no cross-hardware extrapolation applied".into(),
        }
    }
}

fn matches_query(record: &BenchmarkRecord, query: &PredictionQuery) -> bool {
    record.hardware_fingerprint == query.hardware_fingerprint
        && record.model_id == query.model_id
        && record.runtime == query.runtime
        && record.placement == query.placement
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

    fn store() -> EvidenceStore {
        EvidenceStore {
            records: vec![
                BenchmarkRecord {
                    id: "run-1".into(),
                    hardware_fingerprint: "2xh100-nvlink".into(),
                    model_id: "qwen-72b-q4".into(),
                    runtime: "vllm".into(),
                    placement: PlacementKind::TensorParallel,
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
                        captured_at: None,
                    },
                },
                BenchmarkRecord {
                    id: "run-2".into(),
                    hardware_fingerprint: "2xh100-nvlink".into(),
                    model_id: "qwen-72b-q4".into(),
                    runtime: "vllm".into(),
                    placement: PlacementKind::TensorParallel,
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
                        captured_at: None,
                    },
                },
            ],
        }
    }

    #[test]
    fn exact_evidence_returns_empirical_range() {
        let prediction = store().predict_exact(&PredictionQuery {
            hardware_fingerprint: "2xh100-nvlink".into(),
            model_id: "qwen-72b-q4".into(),
            runtime: "vllm".into(),
            placement: PlacementKind::TensorParallel,
            context_tokens: 32768,
            concurrency: 20,
            metric: MetricKind::DecodeTokensPerSecond,
        });

        assert_eq!(prediction.status, PredictionStatus::Available);
        assert_eq!(prediction.sample_count, 2);
        assert_eq!(prediction.mean, Some(70.0));
        assert_eq!(prediction.min, Some(68.0));
        assert_eq!(prediction.max, Some(72.0));
    }

    #[test]
    fn missing_evidence_does_not_invent_prediction() {
        let prediction = store().predict_exact(&PredictionQuery {
            hardware_fingerprint: "unknown".into(),
            model_id: "qwen-72b-q4".into(),
            runtime: "vllm".into(),
            placement: PlacementKind::TensorParallel,
            context_tokens: 32768,
            concurrency: 20,
            metric: MetricKind::DecodeTokensPerSecond,
        });

        assert_eq!(prediction.status, PredictionStatus::Unavailable);
        assert_eq!(prediction.sample_count, 0);
        assert_eq!(prediction.mean, None);
    }
}
