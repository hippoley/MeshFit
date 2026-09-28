use serde::{Deserialize, Serialize};

use crate::ir::{PlacementKind, PlanIR};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompileRequest {
    pub plan: PlanIR,
    pub model_path: String,
    pub model_id: String,
    pub context_tokens: u32,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    #[serde(default)]
    pub tensor_parallel_size: Option<u32>,
    #[serde(default)]
    pub gpu_layers: Option<u32>,
    #[serde(default)]
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionScope {
    LocalProcess,
    RequiresOrchestrator,
}

fn default_listen_port() -> u16 {
    18080
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceContract {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub health_path: String,
    pub chat_completions_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutablePlanIR {
    pub source_plan_id: String,
    pub model_id: String,
    pub runtime: String,
    pub scope: ExecutionScope,
    pub program: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
    pub working_node: String,
    pub service: ServiceContract,
    #[serde(default)]
    pub assumptions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompileError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CompileError {}

pub fn compile_plan(request: &CompileRequest) -> Result<ExecutablePlanIR, CompileError> {
    match request.plan.runtime.as_str() {
        "llama.cpp" => compile_llama_cpp(request),
        "vllm" => compile_vllm(request),
        other => Err(CompileError {
            code: "unsupported_runtime".into(),
            message: format!("runtime '{other}' has no MeshFit compiler yet"),
        }),
    }
}

fn compile_llama_cpp(request: &CompileRequest) -> Result<ExecutablePlanIR, CompileError> {
    if request.plan.nodes.len() != 1 {
        return Err(CompileError {
            code: "llama_cpp_multi_node_not_compiled".into(),
            message:
                "v0.3 only compiles single-node llama.cpp plans; RPC orchestration is not implemented"
                    .into(),
        });
    }

    match request.plan.placement {
        PlacementKind::SingleHost | PlacementKind::CpuOffload => {}
        _ => {
            return Err(CompileError {
                code: "unsupported_llama_cpp_placement".into(),
                message: format!(
                    "placement {:?} is not compiled by the current llama.cpp adapter",
                    request.plan.placement
                ),
            });
        }
    }

    let mut args = vec![
        "-m".into(),
        request.model_path.clone(),
        "-c".into(),
        request.context_tokens.to_string(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        request.listen_port.to_string(),
    ];

    match request.plan.placement {
        PlacementKind::SingleHost => {
            args.push("-ngl".into());
            args.push("all".into());
        }
        PlacementKind::CpuOffload => {
            let gpu_layers = request.gpu_layers.ok_or_else(|| CompileError {
                code: "missing_gpu_layers".into(),
                message: "CPU offload compilation requires an explicit llama.cpp GPU-layer count".into(),
            })?;
            args.push("-ngl".into());
            args.push(gpu_layers.to_string());
        }
        _ => unreachable!(),
    }

    args.extend(request.extra_args.clone());

    Ok(ExecutablePlanIR {
        source_plan_id: request.plan.id.clone(),
        model_id: request.model_id.clone(),
        runtime: "llama.cpp".into(),
        scope: ExecutionScope::LocalProcess,
        program: "llama-server".into(),
        args,
        env: vec![],
        working_node: request.plan.nodes[0].clone(),
        service: ServiceContract {
            scheme: "http".into(),
            host: "127.0.0.1".into(),
            port: request.listen_port,
            health_path: "/health".into(),
            chat_completions_path: "/v1/chat/completions".into(),
        },
        assumptions: vec![
            "llama-server is available on PATH and supports current -ngl semantics".into(),
            "v0.3 compiler emits a launch contract; it does not claim runtime compatibility until executed".into(),
        ],
    })
}

fn compile_vllm(request: &CompileRequest) -> Result<ExecutablePlanIR, CompileError> {
    if request.plan.nodes.is_empty() {
        return Err(CompileError {
            code: "missing_node".into(),
            message: "vLLM plan has no selected node".into(),
        });
    }

    if request.plan.nodes.len() > 1 {
        return Err(CompileError {
            code: "vllm_multi_node_requires_orchestrator".into(),
            message:
                "cross-node vLLM requires explicit orchestration; MeshFit does not emit a fake one-line launcher"
                    .into(),
        });
    }

    match request.plan.placement {
        PlacementKind::SingleHost | PlacementKind::TensorParallel => {}
        _ => {
            return Err(CompileError {
                code: "unsupported_vllm_placement".into(),
                message: format!(
                    "placement {:?} is not compiled by the current vLLM adapter",
                    request.plan.placement
                ),
            });
        }
    }

    let tp_size = match request.plan.placement {
        PlacementKind::TensorParallel => request.tensor_parallel_size.ok_or_else(|| CompileError {
            code: "missing_tensor_parallel_size".into(),
            message: "tensor-parallel compilation requires an explicit device count".into(),
        })?,
        _ => 1,
    };

    let mut args = vec![
        "serve".into(),
        request.model_path.clone(),
        "--max-model-len".into(),
        request.context_tokens.to_string(),
        "--tensor-parallel-size".into(),
        tp_size.to_string(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        request.listen_port.to_string(),
    ];
    args.extend(request.extra_args.clone());

    Ok(ExecutablePlanIR {
        source_plan_id: request.plan.id.clone(),
        model_id: request.model_id.clone(),
        runtime: "vllm".into(),
        scope: ExecutionScope::LocalProcess,
        program: "vllm".into(),
        args,
        env: vec![],
        working_node: request.plan.nodes[0].clone(),
        service: ServiceContract {
            scheme: "http".into(),
            host: "127.0.0.1".into(),
            port: request.listen_port,
            health_path: "/health".into(),
            chat_completions_path: "/v1/chat/completions".into(),
        },
        assumptions: vec![
            "vllm is available on PATH".into(),
            "tensor-parallel size is supplied explicitly from device-count evidence; MeshFit does not infer it from memory ratios".into(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(runtime: &str, placement: PlacementKind, nodes: Vec<&str>) -> PlanIR {
        PlanIR {
            id: "plan-1".into(),
            placement,
            runtime: runtime.into(),
            nodes: nodes.into_iter().map(str::to_string).collect(),
            required_memory_gb: 40.0,
            accelerator_memory_gb: 80.0,
            relative_compute: 10.0,
            hourly_cost_usd: 0.0,
            memory_headroom_gb: 40.0,
            assumptions: vec![],
        }
    }

    #[test]
    fn compiles_single_host_llama_cpp() {
        let executable = compile_plan(&CompileRequest {
            plan: plan("llama.cpp", PlacementKind::SingleHost, vec!["node-a"]),
            model_path: "/models/demo.gguf".into(),
            model_id: "demo".into(),
            context_tokens: 32768,
            listen_port: 18080,
            tensor_parallel_size: None,
            gpu_layers: None,
            extra_args: vec![],
        })
        .unwrap();

        assert_eq!(executable.program, "llama-server");
        assert!(executable.args.contains(&"/models/demo.gguf".to_string()));
    }

    #[test]
    fn cpu_offload_requires_explicit_gpu_layers() {
        let error = compile_plan(&CompileRequest {
            plan: plan("llama.cpp", PlacementKind::CpuOffload, vec!["node-a"]),
            model_path: "/models/demo.gguf".into(),
            model_id: "demo".into(),
            context_tokens: 32768,
            listen_port: 18080,
            tensor_parallel_size: None,
            gpu_layers: None,
            extra_args: vec![],
        })
        .unwrap_err();

        assert_eq!(error.code, "missing_gpu_layers");
    }

    #[test]
    fn compiles_local_vllm_tp() {
        let executable = compile_plan(&CompileRequest {
            plan: plan("vllm", PlacementKind::TensorParallel, vec!["node-a"]),
            model_path: "Qwen/Qwen3-32B".into(),
            model_id: "qwen3-32b".into(),
            context_tokens: 32768,
            listen_port: 18080,
            tensor_parallel_size: Some(2),
            gpu_layers: None,
            extra_args: vec![],
        })
        .unwrap();

        assert_eq!(executable.program, "vllm");
        assert!(executable.args.contains(&"--tensor-parallel-size".to_string()));
    }

    #[test]
    fn rejects_fake_cross_node_vllm_launcher() {
        let error = compile_plan(&CompileRequest {
            plan: plan(
                "vllm",
                PlacementKind::TensorParallel,
                vec!["node-a", "node-b"],
            ),
            model_path: "Qwen/Qwen3-32B".into(),
            model_id: "qwen3-32b".into(),
            context_tokens: 32768,
            listen_port: 18080,
            tensor_parallel_size: Some(2),
            gpu_layers: None,
            extra_args: vec![],
        })
        .unwrap_err();

        assert_eq!(error.code, "vllm_multi_node_requires_orchestrator");
    }
}
