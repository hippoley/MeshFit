use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

use crate::{
    benchmark::{BenchmarkBundle, BenchmarkConfig, BenchmarkRequestIR, RequestMeasurement},
    compiler::{ExecutablePlanIR, ExecutionScope},
    discovery::{discover_local, topology_identity_from_discovery, LocalDiscovery},
    evidence::BenchmarkProvenance,
    identity::{ExecutionIdentity, ModelArtifactIdentity, RuntimeIdentity},
    runtime_discovery::discover_runtimes,
};

pub fn build_benchmark_request_from_facts(
    executable: ExecutablePlanIR,
    local: LocalDiscovery,
    model: ModelArtifactIdentity,
    mut runtime: RuntimeIdentity,
    context_tokens: u32,
    concurrency: u32,
    config: BenchmarkConfig,
) -> Result<BenchmarkRequestIR, String> {
    if executable.working_node != local.node.id {
        return Err(format!(
            "executable targets node '{}' but discovery describes '{}'",
            executable.working_node, local.node.id
        ));
    }

    if executable.model_id != model.model_id {
        return Err(format!(
            "executable model '{}' does not match artifact identity '{}'",
            executable.model_id, model.model_id
        ));
    }

    if executable.runtime != runtime.runtime {
        return Err(format!(
            "executable runtime '{}' does not match discovered runtime '{}'",
            executable.runtime, runtime.runtime
        ));
    }

    runtime.flags = executable.identity_flags.clone();

    let identity = ExecutionIdentity {
        hardware: local.hardware_identity.clone(),
        model,
        runtime,
        topology: topology_identity_from_discovery(&local),
        placement: executable.placement,
    };

    Ok(BenchmarkRequestIR {
        executable,
        identity,
        context_tokens,
        concurrency,
        config,
    })
}

pub fn prepare_local_benchmark_request(
    executable: ExecutablePlanIR,
    model: ModelArtifactIdentity,
    context_tokens: u32,
    concurrency: u32,
    config: BenchmarkConfig,
) -> Result<BenchmarkRequestIR, String> {
    let local = discover_local();
    let discovered_runtimes = discover_runtimes();
    let runtime = discovered_runtimes
        .runtimes
        .into_iter()
        .find(|runtime| runtime.runtime == executable.runtime)
        .ok_or_else(|| {
            format!(
                "runtime '{}' is not installed or could not be discovered on PATH",
                executable.runtime
            )
        })?;

    build_benchmark_request_from_facts(
        executable,
        local,
        model,
        runtime,
        context_tokens,
        concurrency,
        config,
    )
}

pub fn validate_local_benchmark_request(
    request: &BenchmarkRequestIR,
    local_node_id: &str,
) -> Result<(), String> {
    if request.executable.scope != ExecutionScope::LocalProcess {
        return Err("benchmark runner only supports LocalProcess executable plans".into());
    }

    if request.executable.working_node != local_node_id {
        return Err(format!(
            "plan targets node '{}' but runner is on '{}'",
            request.executable.working_node, local_node_id
        ));
    }

    if request.executable.runtime != request.identity.runtime.runtime {
        return Err("executable runtime does not match ExecutionIdentity runtime".into());
    }

    if request.executable.model_id != request.identity.model.model_id {
        return Err("executable model id does not match ExecutionIdentity model id".into());
    }

    if request.executable.placement != request.identity.placement {
        return Err("executable placement does not match ExecutionIdentity placement".into());
    }

    if request.executable.service.scheme != "http" {
        return Err("v0.3 local benchmark runner only supports HTTP services".into());
    }

    if request.executable.service.host != "127.0.0.1"
        && request.executable.service.host != "localhost"
    {
        return Err("v0.3 local benchmark runner only permits loopback service endpoints".into());
    }

    Ok(())
}

pub fn run_local_benchmark(
    request: BenchmarkRequestIR,
    local_node_id: &str,
) -> Result<BenchmarkBundle, String> {
    validate_local_benchmark_request(&request, local_node_id)?;

    let mut command = Command::new(&request.executable.program);
    command
        .args(&request.executable.args)
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    for (key, value) in &request.executable.env {
        command.env(key, value);
    }

    let mut child = command.spawn().map_err(|e| {
        format!(
            "launch {} failed: {e}",
            request.executable.program
        )
    })?;

    let result = run_against_child(&request, &mut child);

    let _ = child.kill();
    let _ = child.wait();

    result
}

fn run_against_child(
    request: &BenchmarkRequestIR,
    child: &mut Child,
) -> Result<BenchmarkBundle, String> {
    wait_for_health(request, child)?;

    for _ in 0..request.config.warmup_requests {
        let _ = run_streaming_request(request)?;
    }

    let mut measurements = Vec::new();
    for _ in 0..request.config.measured_requests {
        measurements.push(run_streaming_request(request)?);
    }

    let unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system clock error: {e}"))?
        .as_secs();

    let benchmark_id = format!(
        "{}-{}",
        sanitize_id(&request.executable.source_plan_id),
        unix_seconds
    );

    Ok(BenchmarkBundle {
        benchmark_id,
        request: request.clone(),
        measurements,
        provenance: BenchmarkProvenance {
            source: "meshfit-local-runner".into(),
            source_url: None,
            commit: None,
            captured_at: Some(format!("unix:{unix_seconds}")),
        },
    })
}

fn wait_for_health(request: &BenchmarkRequestIR, child: &mut Child) -> Result<(), String> {
    let timeout = Duration::from_millis(request.config.startup_timeout_ms);
    let start = Instant::now();
    let url = format!(
        "{}://{}:{}{}",
        request.executable.service.scheme,
        request.executable.service.host,
        request.executable.service.port,
        request.executable.service.health_path
    );

    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("check runtime process: {e}"))?
        {
            return Err(format!(
                "runtime process exited before health became ready: {status}"
            ));
        }

        let health = Command::new("curl")
            .args(["-fsS", "--max-time", "2", &url])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        if matches!(health, Ok(status) if status.success()) {
            return Ok(());
        }

        if start.elapsed() >= timeout {
            return Err(format!(
                "service health endpoint did not become ready within {} ms",
                request.config.startup_timeout_ms
            ));
        }

        thread::sleep(Duration::from_millis(250));
    }
}

fn run_streaming_request(request: &BenchmarkRequestIR) -> Result<RequestMeasurement, String> {
    let url = format!(
        "{}://{}:{}{}",
        request.executable.service.scheme,
        request.executable.service.host,
        request.executable.service.port,
        request.executable.service.chat_completions_path
    );

    let payload = json!({
        "model": request.executable.model_id,
        "messages": [
            {
                "role": "user",
                "content": request.config.prompt.clone()
            }
        ],
        "max_tokens": request.config.max_tokens,
        "stream": true,
        "stream_options": {
            "include_usage": true
        }
    })
    .to_string();

    let max_time_seconds = request
        .config
        .request_timeout_ms
        .saturating_add(999)
        / 1000;

    let max_time_seconds_text = max_time_seconds.to_string();
    let start = Instant::now();
    let mut curl = Command::new("curl")
        .args([
            "-N",
            "-sS",
            "--fail",
            "--max-time",
            &max_time_seconds_text,
            "-H",
            "Content-Type: application/json",
            "-d",
            &payload,
            &url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("launch curl benchmark request: {e}"))?;

    let stdout = curl
        .stdout
        .take()
        .ok_or_else(|| "curl stdout pipe unavailable".to_string())?;
    let reader = BufReader::new(stdout);

    let mut ttft_ms = None;
    let mut output_tokens = None;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("read streaming response: {e}"))?;
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }

        let Ok(value) = serde_json::from_str::<Value>(data) else {
            continue;
        };

        if ttft_ms.is_none() && has_non_empty_content_delta(&value) {
            ttft_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
        }

        if let Some(tokens) = value
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_u64)
        {
            output_tokens = u32::try_from(tokens).ok();
        }
    }

    let status = curl
        .wait()
        .map_err(|e| format!("wait for curl benchmark request: {e}"))?;

    if !status.success() {
        return Err(format!("benchmark request failed with status {status}"));
    }

    let total_ms = start.elapsed().as_secs_f64() * 1000.0;
    let ttft_ms = ttft_ms.ok_or_else(|| {
        "streaming response completed without a non-empty content delta; TTFT unavailable".to_string()
    })?;

    Ok(RequestMeasurement {
        ttft_ms,
        total_ms,
        output_tokens,
        peak_vram_gb: None,
        peak_ram_gb: None,
    })
}

pub fn has_non_empty_content_delta(value: &Value) -> bool {
    value
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
        .is_some_and(|content| !content.is_empty())
}

fn sanitize_id(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        benchmark::{BenchmarkConfig, BenchmarkRequestIR},
        compiler::{ExecutablePlanIR, ServiceContract},
        identity::{
            ExecutionIdentity, HardwareIdentity, ModelArtifactIdentity, RuntimeIdentity,
            TopologyIdentity,
        },
        ir::PlacementKind,
    };

    fn request() -> BenchmarkRequestIR {
        BenchmarkRequestIR {
            executable: ExecutablePlanIR {
                source_plan_id: "plan-1".into(),
                model_id: "demo".into(),
                placement: PlacementKind::SingleHost,
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
                    quantization: "awq4".into(),
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
    fn validates_node_runtime_model_and_placement_identity() {
        assert!(validate_local_benchmark_request(&request(), "node-a").is_ok());

        let error = validate_local_benchmark_request(&request(), "node-b").unwrap_err();
        assert!(error.contains("targets node"));
    }

    #[test]
    fn rejects_non_loopback_service() {
        let mut changed = request();
        changed.executable.service.host = "0.0.0.0".into();
        assert!(validate_local_benchmark_request(&changed, "node-a").is_err());
    }

    #[test]
    fn detects_first_content_delta() {
        let value = json!({
            "choices": [
                {
                    "delta": {
                        "content": "hello"
                    }
                }
            ]
        });

        assert!(has_non_empty_content_delta(&value));
    }

    #[test]
    fn sanitizes_benchmark_ids() {
        assert_eq!(sanitize_id("node:vllm:SingleHost"), "node-vllm-SingleHost");
    }
}
