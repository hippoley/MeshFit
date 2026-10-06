use serde::{Deserialize, Serialize};

use crate::{
    compiler::ExecutablePlanIR,
    evidence::{BenchmarkProvenance, BenchmarkRecord, MetricKind, MetricObservation},
    identity::ExecutionIdentity,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkConfig {
    pub prompt: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_warmup_requests")]
    pub warmup_requests: u32,
    #[serde(default = "default_measured_requests")]
    pub measured_requests: u32,
    #[serde(default = "default_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_startup_timeout_ms")]
    pub startup_timeout_ms: u64,
}

fn default_max_tokens() -> u32 {
    64
}

fn default_warmup_requests() -> u32 {
    1
}

fn default_measured_requests() -> u32 {
    3
}

fn default_timeout_ms() -> u64 {
    120_000
}

fn default_startup_timeout_ms() -> u64 {
    300_000
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkRequestIR {
    pub executable: ExecutablePlanIR,
    pub identity: ExecutionIdentity,
    pub context_tokens: u32,
    pub concurrency: u32,
    pub config: BenchmarkConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestMeasurement {
    pub ttft_ms: f64,
    pub total_ms: f64,
    #[serde(default)]
    pub output_tokens: Option<u32>,
    #[serde(default)]
    pub peak_vram_gb: Option<f64>,
    #[serde(default)]
    pub peak_ram_gb: Option<f64>,
}

impl RequestMeasurement {
    pub fn decode_tokens_per_second(&self) -> Option<f64> {
        let output_tokens = self.output_tokens?;
        if output_tokens <= 1 || self.total_ms <= self.ttft_ms {
            return None;
        }

        let decode_seconds = (self.total_ms - self.ttft_ms) / 1000.0;
        if decode_seconds <= 0.0 {
            return None;
        }

        Some((output_tokens - 1) as f64 / decode_seconds)
    }

    pub fn tpot_ms(&self) -> Option<f64> {
        let output_tokens = self.output_tokens?;
        if output_tokens <= 1 || self.total_ms < self.ttft_ms {
            return None;
        }

        Some((self.total_ms - self.ttft_ms) / (output_tokens - 1) as f64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkBundle {
    pub benchmark_id: String,
    pub request: BenchmarkRequestIR,
    #[serde(default)]
    pub measurements: Vec<RequestMeasurement>,
    pub provenance: BenchmarkProvenance,
}

impl BenchmarkBundle {
    pub fn validate(&self) -> Result<(), String> {
        if self.request.executable.context_tokens != self.request.context_tokens {
            return Err("benchmark context does not match executable plan context".into());
        }
        if self.request.executable.runtime != self.request.identity.runtime.runtime {
            return Err("benchmark executable runtime does not match execution identity".into());
        }
        if self.request.executable.model_id != self.request.identity.model.model_id {
            return Err("benchmark executable model does not match execution identity".into());
        }
        if self.request.executable.placement != self.request.identity.placement {
            return Err("benchmark executable placement does not match execution identity".into());
        }
        if self.request.executable.identity_flags != self.request.identity.runtime.flags {
            return Err("benchmark executable identity flags do not match runtime identity flags".into());
        }
        if self.request.identity.model.artifact_sha256.is_none()
            && self.request.identity.model.revision.is_none()
        {
            return Err("benchmark model identity lacks artifact hash or revision".into());
        }

        if self.measurements.is_empty() {
            return Err("benchmark bundle contains no measured requests".into());
        }

        if self.request.config.measured_requests as usize != self.measurements.len() {
            return Err(format!(
                "expected {} measured requests but bundle contains {}",
                self.request.config.measured_requests,
                self.measurements.len()
            ));
        }

        for (idx, measurement) in self.measurements.iter().enumerate() {
            if measurement.ttft_ms < 0.0 {
                return Err(format!("measurement {idx} has negative TTFT"));
            }
            if measurement.total_ms <= 0.0 {
                return Err(format!("measurement {idx} has non-positive total duration"));
            }
            if measurement.total_ms < measurement.ttft_ms {
                return Err(format!(
                    "measurement {idx} total duration is lower than TTFT"
                ));
            }
            if measurement.output_tokens == Some(0) {
                return Err(format!("measurement {idx} has zero output tokens"));
            }
        }

        Ok(())
    }

    pub fn to_benchmark_records(&self) -> Result<Vec<BenchmarkRecord>, String> {
        self.validate()?;

        let mut records = Vec::new();

        for (idx, measurement) in self.measurements.iter().enumerate() {
            let mut observations = vec![MetricObservation {
                metric: MetricKind::TtftMs,
                value: measurement.ttft_ms,
            }];

            if let Some(output_tokens) = measurement.output_tokens {
                observations.push(MetricObservation {
                    metric: MetricKind::ThroughputTokensPerSecond,
                    value: output_tokens as f64 / (measurement.total_ms / 1000.0),
                });
            }

            if let Some(tpot_ms) = measurement.tpot_ms() {
                observations.push(MetricObservation {
                    metric: MetricKind::TpotMs,
                    value: tpot_ms,
                });
            }

            if let Some(decode_tps) = measurement.decode_tokens_per_second() {
                observations.push(MetricObservation {
                    metric: MetricKind::DecodeTokensPerSecond,
                    value: decode_tps,
                });
            }

            if let Some(peak_vram_gb) = measurement.peak_vram_gb {
                observations.push(MetricObservation {
                    metric: MetricKind::PeakVramGb,
                    value: peak_vram_gb,
                });
            }

            if let Some(peak_ram_gb) = measurement.peak_ram_gb {
                observations.push(MetricObservation {
                    metric: MetricKind::PeakRamGb,
                    value: peak_ram_gb,
                });
            }

            records.push(BenchmarkRecord {
                id: format!("{}-{}", self.benchmark_id, idx + 1),
                identity: self.request.identity.clone(),
                context_tokens: self.request.context_tokens,
                concurrency: self.request.concurrency,
                observations,
                provenance: self.provenance.clone(),
            });
        }

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        compiler::{ExecutionScope, ServiceContract},
        identity::{
            HardwareIdentity, ModelArtifactIdentity, RuntimeIdentity, TopologyIdentity,
        },
        ir::PlacementKind,
    };

    fn request() -> BenchmarkRequestIR {
        BenchmarkRequestIR {
            executable: ExecutablePlanIR {
                source_plan_id: "plan-1".into(),
                model_id: "demo".into(),
                model_source: "/models/demo.gguf".into(),
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
                    format: "gguf".into(),
                    quantization: "q4".into(),
                    artifact_sha256: Some("abc".into()),
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
        }
    }

    #[test]
    fn derives_tpot_and_decode_rate() {
        let measurement = RequestMeasurement {
            ttft_ms: 100.0,
            total_ms: 1100.0,
            output_tokens: Some(11),
            peak_vram_gb: None,
            peak_ram_gb: None,
        };

        assert_eq!(measurement.tpot_ms(), Some(100.0));
        assert_eq!(measurement.decode_tokens_per_second(), Some(10.0));
    }

    #[test]
    fn missing_usage_does_not_invent_token_rate() {
        let measurement = RequestMeasurement {
            ttft_ms: 100.0,
            total_ms: 1100.0,
            output_tokens: None,
            peak_vram_gb: None,
            peak_ram_gb: None,
        };

        assert_eq!(measurement.tpot_ms(), None);
        assert_eq!(measurement.decode_tokens_per_second(), None);
    }

    #[test]
    fn converts_bundle_into_provenance_bound_records() {
        let bundle = BenchmarkBundle {
            benchmark_id: "bench-1".into(),
            request: request(),
            measurements: vec![RequestMeasurement {
                ttft_ms: 100.0,
                total_ms: 1100.0,
                output_tokens: Some(11),
                peak_vram_gb: Some(12.5),
                peak_ram_gb: Some(8.0),
            }],
            provenance: BenchmarkProvenance {
                source: "meshfit-benchmark".into(),
                source_url: None,
                commit: Some("abc".into()),
                captured_at: None,
            },
        };

        let records = bundle.to_benchmark_records().unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0]
            .observations
            .iter()
            .any(|o| o.metric == MetricKind::DecodeTokensPerSecond));
    }

    #[test]
    fn rejects_identity_flag_mismatch() {
        let mut bundle = BenchmarkBundle {
            benchmark_id: "bench-flags".into(),
            request: request(),
            measurements: vec![RequestMeasurement {
                ttft_ms: 100.0,
                total_ms: 1100.0,
                output_tokens: Some(11),
                peak_vram_gb: None,
                peak_ram_gb: None,
            }],
            provenance: BenchmarkProvenance {
                source: "meshfit-benchmark".into(),
                source_url: None,
                commit: None,
                captured_at: None,
            },
        };
        bundle.request.executable.identity_flags = vec!["--tensor-parallel-size".into(), "2".into()];

        assert!(bundle.validate().is_err());
    }

    #[test]
    fn rejects_empty_benchmark_bundle() {
        let bundle = BenchmarkBundle {
            benchmark_id: "bench-empty".into(),
            request: request(),
            measurements: vec![],
            provenance: BenchmarkProvenance {
                source: "meshfit-benchmark".into(),
                source_url: None,
                commit: None,
                captured_at: None,
            },
        };

        assert!(bundle.validate().is_err());
    }
}
