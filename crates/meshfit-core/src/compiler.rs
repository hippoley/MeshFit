use serde::{Deserialize, Serialize};

use crate::ir::{AcceleratorBackend, AcceleratorRefIR, PlacementKind, PlanIR};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompileRequest {
    pub plan: PlanIR,
    pub model_path: String,
    pub model_id: String,
    pub context_tokens: u32,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
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
    pub model_source: String,
    pub placement: PlacementKind,
    pub context_tokens: u32,
    pub runtime: String,
    pub scope: ExecutionScope,
    pub program: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub identity_flags: Vec<String>,
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
    validate_device_refs(&request.plan)?;

    match request.plan.runtime.as_str() {
        "llama.cpp" => compile_llama_cpp(request),
        "vllm" => compile_vllm(request),
        other => Err(CompileError {
            code: "unsupported_runtime".into(),
            message: format!("runtime '{other}' has no MeshFit compiler yet"),
        }),
    }
}

fn validate_device_refs(plan: &PlanIR) -> Result<(), CompileError> {
    if plan.accelerators.is_empty() {
        return Err(CompileError {
            code: "missing_accelerator_selection".into(),
            message: "executable compilation requires PlanIR to select concrete accelerators"
                .into(),
        });
    }

    for accelerator in &plan.accelerators {
        if !plan.nodes.contains(&accelerator.node) {
            return Err(CompileError {
                code: "accelerator_node_mismatch".into(),
                message: format!(
                    "selected accelerator {}/{} is not on a selected plan node",
                    accelerator.node, accelerator.accelerator
                ),
            });
        }
    }

    Ok(())
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

    if request.plan.accelerators.len() != 1 {
        return Err(CompileError {
            code: "llama_cpp_multi_device_not_compiled".into(),
            message: "v0.3 llama.cpp compiler only binds one explicitly selected accelerator"
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

    let selected = &request.plan.accelerators[0];
    if selected.backend != AcceleratorBackend::Cuda {
        return Err(CompileError {
            code: "llama_cpp_device_binding_not_implemented".into(),
            message: format!(
                "explicit llama.cpp device binding for {:?} is not implemented yet",
                selected.backend
            ),
        });
    }
    let cuda_device = cuda_device_name(selected)?;

    let mut args = vec![
        "-m".into(),
        request.model_path.clone(),
        "-c".into(),
        request.context_tokens.to_string(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        request.listen_port.to_string(),
        "--alias".into(),
        request.model_id.clone(),
        "--device".into(),
        cuda_device.clone(),
    ];

    let gpu_layers = match request.plan.placement {
        PlacementKind::SingleHost => "all".to_string(),
        PlacementKind::CpuOffload => request
            .gpu_layers
            .ok_or_else(|| CompileError {
                code: "missing_gpu_layers".into(),
                message: "CPU offload compilation requires an explicit llama.cpp GPU-layer count"
                    .into(),
            })?
            .to_string(),
        _ => unreachable!(),
    };
    args.push("-ngl".into());
    args.push(gpu_layers.clone());
    args.extend(request.extra_args.clone());

    let mut identity_flags = vec![
        "-c".into(),
        request.context_tokens.to_string(),
        "--device".into(),
        cuda_device,
        "-ngl".into(),
        gpu_layers,
    ];
    identity_flags.extend(request.extra_args.clone());

    Ok(ExecutablePlanIR {
        source_plan_id: request.plan.id.clone(),
        model_id: request.model_id.clone(),
        model_source: request.model_path.clone(),
        placement: request.plan.placement,
        context_tokens: request.context_tokens,
        runtime: "llama.cpp".into(),
        scope: ExecutionScope::LocalProcess,
        program: "llama-server".into(),
        args,
        identity_flags,
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
            "llama-server is available on PATH and supports current --device/-ngl semantics".into(),
            "selected CUDA device is bound explicitly from PlanIR".into(),
        ],
    })
}

fn compile_vllm(request: &CompileRequest) -> Result<ExecutablePlanIR, CompileError> {
    if request.plan.nodes.len() != 1 {
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

    if request
        .plan
        .accelerators
        .iter()
        .any(|accelerator| accelerator.backend != AcceleratorBackend::Cuda)
    {
        return Err(CompileError {
            code: "vllm_non_cuda_device_not_compiled".into(),
            message: "v0.3 vLLM compiler currently binds CUDA accelerator selections only".into(),
        });
    }

    let device_indices = request
        .plan
        .accelerators
        .iter()
        .map(cuda_device_index)
        .collect::<Result<Vec<_>, _>>()?;

    let expected_count = match request.plan.placement {
        PlacementKind::SingleHost => 1,
        PlacementKind::TensorParallel => {
            if device_indices.len() < 2 {
                return Err(CompileError {
                    code: "tp_requires_multiple_devices".into(),
                    message: "TensorParallel plan selected fewer than two accelerators".into(),
                });
            }
            device_indices.len()
        }
        _ => unreachable!(),
    };

    if request.plan.placement == PlacementKind::SingleHost && device_indices.len() != 1 {
        return Err(CompileError {
            code: "single_host_requires_one_device".into(),
            message: "SingleHost vLLM plan must select exactly one accelerator".into(),
        });
    }

    let visible_devices = device_indices
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");

    let mut args = vec![
        "serve".into(),
        request.model_path.clone(),
        "--max-model-len".into(),
        request.context_tokens.to_string(),
        "--tensor-parallel-size".into(),
        expected_count.to_string(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        request.listen_port.to_string(),
        "--served-model-name".into(),
        request.model_id.clone(),
    ];
    args.extend(request.extra_args.clone());

    let device_binding = format!("CUDA_VISIBLE_DEVICES={visible_devices}");
    let mut identity_flags = vec![
        "--max-model-len".into(),
        request.context_tokens.to_string(),
        "--tensor-parallel-size".into(),
        expected_count.to_string(),
        device_binding.clone(),
    ];
    identity_flags.extend(request.extra_args.clone());

    Ok(ExecutablePlanIR {
        source_plan_id: request.plan.id.clone(),
        model_id: request.model_id.clone(),
        model_source: request.model_path.clone(),
        placement: request.plan.placement,
        context_tokens: request.context_tokens,
        runtime: "vllm".into(),
        scope: ExecutionScope::LocalProcess,
        program: "vllm".into(),
        args,
        identity_flags,
        env: vec![("CUDA_VISIBLE_DEVICES".into(), visible_devices)],
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
            "selected CUDA devices are bound through CUDA_VISIBLE_DEVICES".into(),
            "tensor-parallel size equals the number of explicitly selected accelerators".into(),
        ],
    })
}

fn cuda_device_index(accelerator: &AcceleratorRefIR) -> Result<u32, CompileError> {
    accelerator
        .accelerator
        .strip_prefix("gpu")
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| CompileError {
            code: "unmappable_cuda_device_id".into(),
            message: format!(
                "accelerator id '{}' cannot be mapped to a CUDA ordinal; discovery ids must use gpuN",
                accelerator.accelerator
            ),
        })
}

fn cuda_device_name(accelerator: &AcceleratorRefIR) -> Result<String, CompileError> {
    Ok(format!("CUDA{}", cuda_device_index(accelerator)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accelerator(node: &str, id: &str) -> AcceleratorRefIR {
        AcceleratorRefIR {
            node: node.into(),
            accelerator: id.into(),
            backend: AcceleratorBackend::Cuda,
        }
    }

    fn plan(
        runtime: &str,
        placement: PlacementKind,
        nodes: Vec<&str>,
        accelerators: Vec<AcceleratorRefIR>,
    ) -> PlanIR {
        PlanIR {
            id: "plan-1".into(),
            placement,
            runtime: runtime.into(),
            nodes: nodes.into_iter().map(str::to_string).collect(),
            accelerators,
            required_memory_gb: 40.0,
            accelerator_memory_gb: 80.0,
            relative_compute: 10.0,
            hourly_cost_usd: 0.0,
            memory_headroom_gb: 40.0,
            assumptions: vec![],
        }
    }

    #[test]
    fn compiles_single_host_llama_cpp_with_explicit_device() {
        let executable = compile_plan(&CompileRequest {
            plan: plan(
                "llama.cpp",
                PlacementKind::SingleHost,
                vec!["node-a"],
                vec![accelerator("node-a", "gpu1")],
            ),
            model_path: "/models/demo.gguf".into(),
            model_id: "demo".into(),
            context_tokens: 32768,
            listen_port: 18080,
            gpu_layers: None,
            extra_args: vec![],
        })
        .unwrap();

        assert!(executable
            .args
            .windows(2)
            .any(|pair| pair == ["--device", "CUDA1"]));
    }

    #[test]
    fn cpu_offload_requires_explicit_gpu_layers() {
        let error = compile_plan(&CompileRequest {
            plan: plan(
                "llama.cpp",
                PlacementKind::CpuOffload,
                vec!["node-a"],
                vec![accelerator("node-a", "gpu0")],
            ),
            model_path: "/models/demo.gguf".into(),
            model_id: "demo".into(),
            context_tokens: 32768,
            listen_port: 18080,
            gpu_layers: None,
            extra_args: vec![],
        })
        .unwrap_err();

        assert_eq!(error.code, "missing_gpu_layers");
    }

    #[test]
    fn compiles_local_vllm_tp_from_selected_devices() {
        let executable = compile_plan(&CompileRequest {
            plan: plan(
                "vllm",
                PlacementKind::TensorParallel,
                vec!["node-a"],
                vec![accelerator("node-a", "gpu0"), accelerator("node-a", "gpu2")],
            ),
            model_path: "Qwen/Qwen3-32B".into(),
            model_id: "qwen3-32b".into(),
            context_tokens: 32768,
            listen_port: 18080,
            gpu_layers: None,
            extra_args: vec![],
        })
        .unwrap();

        assert_eq!(
            executable.env,
            vec![("CUDA_VISIBLE_DEVICES".into(), "0,2".into())]
        );
        assert!(executable
            .args
            .windows(2)
            .any(|pair| pair == ["--tensor-parallel-size", "2"]));
    }

    #[test]
    fn rejects_fake_cross_node_vllm_launcher() {
        let error = compile_plan(&CompileRequest {
            plan: plan(
                "vllm",
                PlacementKind::TensorParallel,
                vec!["node-a", "node-b"],
                vec![accelerator("node-a", "gpu0"), accelerator("node-b", "gpu0")],
            ),
            model_path: "Qwen/Qwen3-32B".into(),
            model_id: "qwen3-32b".into(),
            context_tokens: 32768,
            listen_port: 18080,
            gpu_layers: None,
            extra_args: vec![],
        })
        .unwrap_err();

        assert_eq!(error.code, "vllm_multi_node_requires_orchestrator");
    }
}
