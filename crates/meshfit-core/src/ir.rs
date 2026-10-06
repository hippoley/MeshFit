use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AcceleratorBackend {
    Cpu,
    Cuda,
    Rocm,
    Metal,
    Xpu,
    Npu,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AcceleratorIR {
    pub id: String,
    pub backend: AcceleratorBackend,
    pub memory_gb: f64,
    #[serde(default)]
    pub free_memory_gb: Option<f64>,
    #[serde(default)]
    pub relative_compute: f64,
}

impl AcceleratorIR {
    pub fn usable_memory_gb(&self) -> f64 {
        self.free_memory_gb.unwrap_or(self.memory_gb)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardwareNodeIR {
    pub id: String,
    pub site: String,
    pub ram_gb: f64,
    #[serde(default)]
    pub accelerators: Vec<AcceleratorIR>,
    #[serde(default)]
    pub hourly_cost_usd: f64,
}

impl HardwareNodeIR {
    pub fn accelerator_memory_gb(&self) -> f64 {
        self.accelerators
            .iter()
            .map(AcceleratorIR::usable_memory_gb)
            .sum()
    }

    pub fn relative_compute(&self) -> f64 {
        self.accelerators.iter().map(|a| a.relative_compute).sum()
    }

    pub fn primary_backend(&self) -> AcceleratorBackend {
        self.accelerators
            .first()
            .map(|a| a.backend)
            .unwrap_or(AcceleratorBackend::Cpu)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    Pcie,
    Nvlink,
    Ethernet,
    Infiniband,
    Thunderbolt,
    Wan,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum FabricEndpointIR {
    Node { node: String },
    Accelerator { node: String, accelerator: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FabricEdgeIR {
    pub from: FabricEndpointIR,
    pub to: FabricEndpointIR,
    pub kind: LinkKind,
    #[serde(default)]
    pub bandwidth_gbps: Option<f64>,
    #[serde(default)]
    pub latency_ms: Option<f64>,
    #[serde(default)]
    pub jitter_ms: f64,
    #[serde(default)]
    pub egress_cost_usd_per_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InfrastructureIR {
    pub nodes: Vec<HardwareNodeIR>,
    #[serde(default)]
    pub links: Vec<FabricEdgeIR>,
}

impl InfrastructureIR {
    pub fn node(&self, id: &str) -> Option<&HardwareNodeIR> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn accelerator(&self, node_id: &str, accelerator_id: &str) -> Option<&AcceleratorIR> {
        self.node(node_id)?
            .accelerators
            .iter()
            .find(|accelerator| accelerator.id == accelerator_id)
    }

    pub fn accelerator_link(
        &self,
        node_id: &str,
        accelerator_a: &str,
        accelerator_b: &str,
    ) -> Option<&FabricEdgeIR> {
        self.links.iter().find(|link| {
            let endpoint = |node: &str, accelerator: &str| FabricEndpointIR::Accelerator {
                node: node.to_string(),
                accelerator: accelerator.to_string(),
            };
            let a = endpoint(node_id, accelerator_a);
            let b = endpoint(node_id, accelerator_b);
            (link.from == a && link.to == b) || (link.from == b && link.to == a)
        })
    }

    pub fn node_link(&self, a: &str, b: &str) -> Option<&FabricEdgeIR> {
        self.links.iter().find(|link| {
            let forward = matches!(
                (&link.from, &link.to),
                (
                    FabricEndpointIR::Node { node: from },
                    FabricEndpointIR::Node { node: to }
                ) if from == a && to == b
            );
            let reverse = matches!(
                (&link.from, &link.to),
                (
                    FabricEndpointIR::Node { node: from },
                    FabricEndpointIR::Node { node: to }
                ) if from == b && to == a
            );
            forward || reverse
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum KvCacheModelIR {
    BytesPerToken {
        bytes_per_token: u64,
    },
    Transformer {
        layers: u32,
        kv_heads: u32,
        head_dim: u32,
        bytes_per_element: u32,
    },
}

impl KvCacheModelIR {
    pub fn bytes_per_token(&self) -> f64 {
        match self {
            Self::BytesPerToken { bytes_per_token } => *bytes_per_token as f64,
            Self::Transformer {
                layers,
                kv_heads,
                head_dim,
                bytes_per_element,
            } => {
                2.0 * *layers as f64
                    * *kv_heads as f64
                    * *head_dim as f64
                    * *bytes_per_element as f64
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum TpCommunicationModelIR {
    BytesPerToken {
        bytes_per_token: u64,
        synchronizations_per_token: u32,
    },
    Transformer {
        layers: u32,
        hidden_size: u32,
        bytes_per_element: u32,
        collectives_per_layer: u32,
    },
}

impl TpCommunicationModelIR {
    pub fn base_bytes_per_token(&self) -> f64 {
        match self {
            Self::BytesPerToken {
                bytes_per_token, ..
            } => *bytes_per_token as f64,
            Self::Transformer {
                layers,
                hidden_size,
                bytes_per_element,
                collectives_per_layer,
            } => {
                *layers as f64
                    * *hidden_size as f64
                    * *bytes_per_element as f64
                    * *collectives_per_layer as f64
            }
        }
    }

    pub fn synchronizations_per_token(&self) -> u32 {
        match self {
            Self::BytesPerToken {
                synchronizations_per_token,
                ..
            } => *synchronizations_per_token,
            Self::Transformer {
                layers,
                collectives_per_layer,
                ..
            } => layers.saturating_mul(*collectives_per_layer),
        }
    }

    pub fn bytes_per_token_for_tp_size(&self, tp_size: usize) -> f64 {
        if tp_size <= 1 {
            return 0.0;
        }

        let ring_factor = 2.0 * (tp_size as f64 - 1.0) / tp_size as f64;
        self.base_bytes_per_token() * ring_factor
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelIR {
    pub id: String,
    pub parameters_b: f64,
    #[serde(default)]
    pub active_parameters_b: Option<f64>,
    pub weight_memory_gb: f64,
    #[serde(default)]
    pub kv_cache_gb: f64,
    #[serde(default)]
    pub kv_cache_model: Option<KvCacheModelIR>,
    #[serde(default)]
    pub tp_communication_model: Option<TpCommunicationModelIR>,
    #[serde(default)]
    pub is_moe: bool,
    #[serde(default)]
    pub required_backends: Vec<AcceleratorBackend>,
}

impl ModelIR {
    pub fn kv_cache_gb_for(&self, workload: &WorkloadIR) -> f64 {
        let Some(model) = &self.kv_cache_model else {
            return self.kv_cache_gb;
        };

        let bytes = model.bytes_per_token()
            * workload.context_tokens as f64
            * workload.active_sequences() as f64;
        bytes / 1_000_000_000.0
    }

    pub fn required_memory_gb_for(&self, workload: &WorkloadIR) -> f64 {
        self.weight_memory_gb + self.kv_cache_gb_for(workload)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeIR {
    pub id: String,
    #[serde(default)]
    pub backends: Vec<AcceleratorBackend>,
    #[serde(default)]
    pub supports_tp: bool,
    #[serde(default)]
    pub supports_pp: bool,
    #[serde(default)]
    pub supports_ep: bool,
    #[serde(default)]
    pub supports_rpc: bool,
    #[serde(default)]
    pub supports_cpu_offload: bool,
}

impl RuntimeIR {
    pub fn supports_backend(&self, backend: AcceleratorBackend) -> bool {
        self.backends.contains(&backend)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkloadIR {
    pub context_tokens: u32,
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    #[serde(default)]
    pub max_active_sequences: Option<u32>,
    #[serde(default)]
    pub max_tp_communication_ms_per_token: Option<f64>,
    #[serde(default)]
    pub p95_latency_ms: Option<u64>,
    #[serde(default)]
    pub budget_per_day_usd: Option<f64>,
}

impl WorkloadIR {
    pub fn active_sequences(&self) -> u32 {
        self.max_active_sequences.unwrap_or(self.concurrency).max(1)
    }
}

fn default_concurrency() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlacementKind {
    SingleHost,
    TensorParallel,
    PipelineParallel,
    ExpertParallel,
    Replica,
    CpuOffload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AcceleratorRefIR {
    pub node: String,
    pub accelerator: String,
    pub backend: AcceleratorBackend,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommunicationEstimateIR {
    pub bytes_per_token: f64,
    pub effective_bandwidth_gbps: f64,
    pub transfer_ms_per_token: f64,
    pub synchronizations_per_token: u32,
    pub synchronization_ms_per_token: f64,
    pub total_ms_per_token: f64,
    pub egress_cost_usd_per_million_tokens: f64,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanIR {
    pub id: String,
    pub placement: PlacementKind,
    pub runtime: String,
    pub nodes: Vec<String>,
    #[serde(default)]
    pub accelerators: Vec<AcceleratorRefIR>,
    pub required_memory_gb: f64,
    pub accelerator_memory_gb: f64,
    pub relative_compute: f64,
    pub hourly_cost_usd: f64,
    pub memory_headroom_gb: f64,
    #[serde(default)]
    pub communication: Option<CommunicationEstimateIR>,
    #[serde(default)]
    pub assumptions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RejectionIR {
    pub candidate: String,
    pub code: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExclusionIR {
    pub node: String,
    pub reason: String,
    pub suggested_role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlacementReport {
    pub model: String,
    pub feasible: Vec<PlanIR>,
    pub pareto: Vec<PlanIR>,
    pub rejected: Vec<RejectionIR>,
    pub excluded_nodes: Vec<ExclusionIR>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlacementTargetIR {
    pub model: ModelIR,
    pub runtimes: Vec<RuntimeIR>,
    pub workload: WorkloadIR,
}

impl PlacementTargetIR {
    pub fn into_scenario(self, infrastructure: InfrastructureIR) -> ScenarioIR {
        ScenarioIR {
            infrastructure,
            model: self.model,
            runtimes: self.runtimes,
            workload: self.workload,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioIR {
    pub infrastructure: InfrastructureIR,
    pub model: ModelIR,
    pub runtimes: Vec<RuntimeIR>,
    pub workload: WorkloadIR,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workload(
        context_tokens: u32,
        concurrency: u32,
        max_active_sequences: Option<u32>,
    ) -> WorkloadIR {
        WorkloadIR {
            context_tokens,
            concurrency,
            max_active_sequences,
            max_tp_communication_ms_per_token: None,
            p95_latency_ms: None,
            budget_per_day_usd: None,
        }
    }

    fn model(kv_cache_model: Option<KvCacheModelIR>) -> ModelIR {
        ModelIR {
            id: "kv-test".into(),
            parameters_b: 1.0,
            active_parameters_b: None,
            weight_memory_gb: 1.0,
            kv_cache_gb: 3.0,
            kv_cache_model,
            tp_communication_model: None,
            is_moe: false,
            required_backends: vec![],
        }
    }

    #[test]
    fn workload_aware_kv_scales_with_context_and_active_sequences() {
        let model = model(Some(KvCacheModelIR::BytesPerToken {
            bytes_per_token: 100_000,
        }));

        let base = model.kv_cache_gb_for(&workload(1_000, 1, None));
        let doubled_context = model.kv_cache_gb_for(&workload(2_000, 1, None));
        let four_active = model.kv_cache_gb_for(&workload(1_000, 20, Some(4)));

        assert!((doubled_context - base * 2.0).abs() < 1e-12);
        assert!((four_active - base * 4.0).abs() < 1e-12);
    }

    #[test]
    fn transformer_kv_formula_counts_k_and_v() {
        let kv = KvCacheModelIR::Transformer {
            layers: 2,
            kv_heads: 4,
            head_dim: 8,
            bytes_per_element: 2,
        };

        assert_eq!(kv.bytes_per_token(), 256.0);
    }

    #[test]
    fn active_sequence_limit_overrides_request_concurrency() {
        let workload = workload(4096, 20, Some(3));
        assert_eq!(workload.active_sequences(), 3);
    }

    #[test]
    fn tp_communication_transformer_profile_derives_bytes_and_syncs() {
        let profile = TpCommunicationModelIR::Transformer {
            layers: 2,
            hidden_size: 8,
            bytes_per_element: 2,
            collectives_per_layer: 2,
        };

        assert_eq!(profile.base_bytes_per_token(), 64.0);
        assert_eq!(profile.synchronizations_per_token(), 4);
        assert_eq!(profile.bytes_per_token_for_tp_size(2), 64.0);
        assert_eq!(profile.bytes_per_token_for_tp_size(4), 96.0);
    }

    #[test]
    fn legacy_kv_fallback_remains_fixed() {
        let model = model(None);

        assert_eq!(
            model.kv_cache_gb_for(&workload(4_096, 1, None)),
            model.kv_cache_gb_for(&workload(32_768, 20, None))
        );
        assert_eq!(model.kv_cache_gb_for(&workload(4_096, 1, None)), 3.0);
    }
}
