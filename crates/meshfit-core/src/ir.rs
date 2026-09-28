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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FabricEdgeIR {
    pub from: String,
    pub to: String,
    pub kind: LinkKind,
    pub bandwidth_gbps: f64,
    pub latency_ms: f64,
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

    pub fn link(&self, a: &str, b: &str) -> Option<&FabricEdgeIR> {
        self.links
            .iter()
            .find(|l| (l.from == a && l.to == b) || (l.from == b && l.to == a))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelIR {
    pub id: String,
    pub parameters_b: f64,
    #[serde(default)]
    pub active_parameters_b: Option<f64>,
    pub weight_memory_gb: f64,
    pub kv_cache_gb: f64,
    #[serde(default)]
    pub is_moe: bool,
    #[serde(default)]
    pub required_backends: Vec<AcceleratorBackend>,
}

impl ModelIR {
    pub fn required_memory_gb(&self) -> f64 {
        self.weight_memory_gb + self.kv_cache_gb
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
    pub p95_latency_ms: Option<u64>,
    #[serde(default)]
    pub budget_per_day_usd: Option<f64>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanIR {
    pub id: String,
    pub placement: PlacementKind,
    pub runtime: String,
    pub nodes: Vec<String>,
    pub required_memory_gb: f64,
    pub accelerator_memory_gb: f64,
    pub relative_compute: f64,
    pub hourly_cost_usd: f64,
    pub memory_headroom_gb: f64,
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
pub struct ScenarioIR {
    pub infrastructure: InfrastructureIR,
    pub model: ModelIR,
    pub runtimes: Vec<RuntimeIR>,
    pub workload: WorkloadIR,
}
