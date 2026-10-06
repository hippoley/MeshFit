use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
};

use serde::{Deserialize, Serialize};

use meshfit_core::{
    compare_benchmarks, compile_plan, discover_local, discover_runtimes, inspect_model_artifact,
    prepare_local_benchmark_request, probe_peer, run_local_benchmark, solve, BenchmarkBundle,
    BenchmarkCandidate, BenchmarkComparisonReport, BenchmarkComparisonRequest, BenchmarkConfig,
    BenchmarkRequestIR, ComparisonObjective, CompileRequest, EvidenceStore, ExecutablePlanIR,
    InfrastructureSnapshot, LinkKind, LocalDiscovery, ModelArtifactIdentity, PeerProbeResult,
    PlacementKind, PlacementReport, PlacementTargetIR, PlanIR, Prediction, PredictionQuery,
    ScenarioIR, SnapshotManifest,
};

const BENCHMARK_LISTEN_PORT: u16 = 18080;

fn default_benchmark_listen_port() -> u16 {
    BENCHMARK_LISTEN_PORT
}

fn parse_benchmark_listen_port(args: &[String]) -> Result<u16, String> {
    let port = option_value(args, "--listen-port")?
        .map(|value| {
            value
                .parse::<u16>()
                .map_err(|e| format!("invalid --listen-port '{value}': {e}"))
        })
        .transpose()?
        .unwrap_or(BENCHMARK_LISTEN_PORT);

    if port == 0 {
        return Err("--listen-port must be between 1 and 65535".to_string());
    }

    Ok(port)
}

#[derive(Debug, Deserialize)]
struct BenchmarkComparisonManifest {
    benchmark_id: String,
    meshfit_candidate: String,
    objective: ComparisonObjective,
    candidates: Vec<BenchmarkCandidateManifest>,
}

#[derive(Debug, Deserialize)]
struct BenchmarkCandidateManifest {
    name: String,
    strategy: String,
    #[serde(default)]
    hourly_cost_usd: Option<f64>,
    bundles: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BenchmarkPlanSet {
    candidates: Vec<BenchmarkPlanCandidate>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BenchmarkPlanCandidate {
    name: String,
    strategy: String,
    plan_id: String,
    placement: PlacementKind,
    runtime: String,
    nodes: Vec<String>,
    relative_compute: f64,
    hourly_cost_usd: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BenchmarkExecutionKit {
    benchmark_id: String,
    ready: bool,
    snapshot: String,
    target: String,
    model_path: String,
    model_identity: String,
    concurrency: u32,
    measured_requests_per_run: u32,
    runs_per_candidate: u32,
    #[serde(default = "default_benchmark_listen_port")]
    listen_port: u16,
    candidates: Vec<BenchmarkExecutionCandidate>,
    comparison_manifest: BenchmarkExecutionComparison,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BenchmarkExecutionCandidate {
    name: String,
    plan_id: String,
    nodes: Vec<String>,
    benchmark_host: String,
    runtime: String,
    result_dir: String,
    executable_path: String,
    compile_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    compile_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compile_command: Option<String>,
    run_commands: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BenchmarkExecutionComparison {
    benchmark_id: String,
    meshfit_candidate: String,
    objective: String,
    candidates: Vec<BenchmarkExecutionComparisonCandidate>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BenchmarkExecutionComparisonCandidate {
    name: String,
    strategy: String,
    hourly_cost_usd: f64,
    bundles: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BenchmarkWorklist {
    benchmark_id: String,
    kit_ready: bool,
    host_filter: Option<String>,
    total_slots: usize,
    valid_slots: usize,
    pending_slots: usize,
    locked_slots: usize,
    invalid_slots: usize,
    hosts: Vec<BenchmarkHostWork>,
}

#[derive(Debug, Serialize)]
struct BenchmarkHostWork {
    host: String,
    total_slots: usize,
    valid_slots: usize,
    pending_slots: usize,
    locked_slots: usize,
    invalid_slots: usize,
    slots: Vec<BenchmarkWorkSlot>,
}

#[derive(Debug, Serialize)]
struct BenchmarkWorkSlot {
    candidate: String,
    runtime: String,
    plan_id: String,
    run_number: usize,
    bundle_path: String,
    state: String,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct BenchmarkKitStatus {
    benchmark_id: String,
    kit_ready: bool,
    complete: bool,
    expected_bundles: usize,
    valid_bundles: usize,
    candidates: Vec<BenchmarkCandidateStatus>,
    issues: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BenchmarkCandidateStatus {
    name: String,
    benchmark_host: String,
    plan_id: String,
    expected_runs: usize,
    valid_runs: usize,
    missing_bundles: Vec<String>,
    invalid_bundles: Vec<BenchmarkInvalidBundle>,
}

#[derive(Debug, Serialize)]
struct BenchmarkInvalidBundle {
    path: String,
    error: String,
}

#[derive(Debug)]
struct BenchmarkRunSlotLock {
    path: PathBuf,
}

impl Drop for BenchmarkRunSlotLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Serialize)]
struct BenchmarkRunOnePlan {
    benchmark_id: String,
    kit_ready: bool,
    candidate: String,
    plan_id: String,
    runtime: String,
    benchmark_host: String,
    observed_host: String,
    host_match: bool,
    preflight_ready: bool,
    preflight_issues: Vec<String>,
    model_path: String,
    model_path_overridden: bool,
    model_artifact_verified: bool,
    listen_port: u16,
    run_number: usize,
    executable_path: String,
    bundle_path: String,
    compile_required: bool,
    bundle_exists: bool,
}

#[derive(Debug, Serialize)]
struct BenchmarkRunCandidatePlan {
    benchmark_id: String,
    candidate: String,
    benchmark_host: String,
    preflight_ready: bool,
    preflight_issues: Vec<String>,
    model_path: String,
    model_path_overridden: bool,
    model_artifact_verified: bool,
    listen_port: u16,
    total_runs: usize,
    existing_valid_runs: Vec<usize>,
    pending_runs: Vec<usize>,
    resume: bool,
    overwrite: bool,
}

#[derive(Debug, Serialize)]
struct BenchmarkRunHostPlan {
    benchmark_id: String,
    host: String,
    ready: bool,
    issues: Vec<String>,
    model_path_override: Option<String>,
    listen_port: u16,
    resume: bool,
    overwrite: bool,
    candidates: Vec<BenchmarkRunCandidatePlan>,
}

#[derive(Debug)]
struct BenchmarkModelPathCheck {
    path: String,
    status: String,
    overridden: bool,
    verified: bool,
    issue: Option<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BenchmarkPreflight {
    benchmark_id: String,
    candidate: String,
    ready: bool,
    expected_host: String,
    observed_host: String,
    host_match: bool,
    runtime: String,
    runtime_found: bool,
    model_path: String,
    model_path_status: String,
    model_path_overridden: bool,
    model_artifact_verified: bool,
    listen_port: u16,
    listen_port_available: bool,
    existing_bundles: Vec<String>,
    issues: Vec<String>,
    warnings: Vec<String>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("meshfit: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("help");

    match command {
        "help" | "--help" | "-h" => print_help(),
        "discover" => {
            let snapshot = discover_local();
            let yaml = serde_yaml::to_string(&snapshot).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "probe" => {
            let peer = args
                .get(2)
                .ok_or_else(|| "usage: meshfit probe <peer> [--bandwidth]".to_string())?;
            let measure_bandwidth = args.iter().any(|arg| arg == "--bandwidth");
            let result = probe_peer(peer, measure_bandwidth);
            let yaml = serde_yaml::to_string(&result).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "runtimes" => {
            let result = discover_runtimes();
            let yaml = serde_yaml::to_string(&result).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "inspect-model" => {
            let path = args
                .get(2)
                .ok_or_else(|| "usage: meshfit inspect-model <path> <model-id> <format> <quantization> [revision]".to_string())?;
            let model_id = args.get(3).ok_or_else(|| "missing model-id".to_string())?;
            let format = args.get(4).ok_or_else(|| "missing format".to_string())?;
            let quantization = args
                .get(5)
                .ok_or_else(|| "missing quantization".to_string())?;
            let revision = args.get(6).cloned();

            let identity = inspect_model_artifact(
                path,
                model_id.clone(),
                format.clone(),
                quantization.clone(),
                revision,
            )?;
            let yaml = serde_yaml::to_string(&identity).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "snapshot-manifest" => {
            let manifest_path = args
                .get(2)
                .ok_or_else(|| "usage: meshfit snapshot-manifest <manifest.yaml>".to_string())?;
            let manifest_raw = fs::read_to_string(manifest_path)
                .map_err(|e| format!("read {manifest_path}: {e}"))?;
            let manifest: SnapshotManifest = serde_yaml::from_str(&manifest_raw)
                .map_err(|e| format!("parse {manifest_path}: {e}"))?;
            let base_dir = Path::new(manifest_path)
                .parent()
                .unwrap_or_else(|| Path::new("."));

            let mut discoveries = Vec::new();
            for discovery_file in &manifest.discovery_files {
                let path = base_dir.join(discovery_file);
                let raw = fs::read_to_string(&path)
                    .map_err(|e| format!("read {}: {e}", path.display()))?;
                let discovery: LocalDiscovery = serde_yaml::from_str(&raw)
                    .map_err(|e| format!("parse {}: {e}", path.display()))?;
                discoveries.push(discovery);
            }

            let mut snapshot =
                InfrastructureSnapshot::from_discoveries(discoveries).map_err(|e| e.to_string())?;

            for probe_spec in &manifest.probes {
                let path = base_dir.join(&probe_spec.probe_file);
                let raw = fs::read_to_string(&path)
                    .map_err(|e| format!("read {}: {e}", path.display()))?;
                let probe: PeerProbeResult = serde_yaml::from_str(&raw)
                    .map_err(|e| format!("parse {}: {e}", path.display()))?;
                snapshot
                    .add_peer_probe(
                        &probe_spec.from_node,
                        &probe_spec.to_node,
                        &probe,
                        probe_spec.kind,
                    )
                    .map_err(|e| e.to_string())?;
            }

            let yaml = serde_yaml::to_string(&snapshot).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "snapshot" => {
            let local_path = args
                .get(2)
                .ok_or_else(|| "usage: meshfit snapshot <local-discovery.yaml> <peer-discovery.yaml> [probe.yaml]".to_string())?;
            let peer_path = args
                .get(3)
                .ok_or_else(|| "usage: meshfit snapshot <local-discovery.yaml> <peer-discovery.yaml> [probe.yaml]".to_string())?;

            let local_raw =
                fs::read_to_string(local_path).map_err(|e| format!("read {local_path}: {e}"))?;
            let peer_raw =
                fs::read_to_string(peer_path).map_err(|e| format!("read {peer_path}: {e}"))?;
            let local: LocalDiscovery =
                serde_yaml::from_str(&local_raw).map_err(|e| format!("parse {local_path}: {e}"))?;
            let peer: LocalDiscovery =
                serde_yaml::from_str(&peer_raw).map_err(|e| format!("parse {peer_path}: {e}"))?;

            let local_id = local.node.id.clone();
            let peer_id = peer.node.id.clone();
            let mut snapshot = InfrastructureSnapshot::from_discoveries(vec![local, peer])
                .map_err(|e| e.to_string())?;

            if let Some(probe_path) = args.get(4) {
                let probe_raw = fs::read_to_string(probe_path)
                    .map_err(|e| format!("read {probe_path}: {e}"))?;
                let probe: PeerProbeResult = serde_yaml::from_str(&probe_raw)
                    .map_err(|e| format!("parse {probe_path}: {e}"))?;
                snapshot
                    .add_peer_probe(&local_id, &peer_id, &probe, LinkKind::Ethernet)
                    .map_err(|e| e.to_string())?;
            }

            let yaml = serde_yaml::to_string(&snapshot).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "benchmark-auto" => {
            let executable_path = args.get(2).ok_or_else(|| {
                "usage: meshfit benchmark-auto <executable.yaml> <model-identity.yaml> [--concurrency N] [--measured-requests N] [--prompt TEXT]"
                    .to_string()
            })?;
            let model_identity_path = args
                .get(3)
                .ok_or_else(|| "missing model-identity.yaml".to_string())?;

            let concurrency = option_value(&args[4..], "--concurrency")?
                .map(|value| {
                    value
                        .parse::<u32>()
                        .map_err(|e| format!("invalid --concurrency '{value}': {e}"))
                })
                .transpose()?
                .unwrap_or(1);

            let prompt = option_value(&args[4..], "--prompt")?
                .map(str::to_string)
                .unwrap_or_else(|| "Explain MeshFit in one sentence.".to_string());

            let measured_requests = option_value(&args[4..], "--measured-requests")?
                .map(|value| {
                    value
                        .parse::<u32>()
                        .map_err(|e| format!("invalid --measured-requests '{value}': {e}"))
                })
                .transpose()?
                .unwrap_or_else(|| default_measured_requests(concurrency));

            if measured_requests == 0 || measured_requests % concurrency != 0 {
                return Err(format!(
                    "--measured-requests must be greater than zero and divisible by concurrency ({concurrency})"
                ));
            }

            let executable_raw = fs::read_to_string(executable_path)
                .map_err(|e| format!("read {executable_path}: {e}"))?;
            let model_raw = fs::read_to_string(model_identity_path)
                .map_err(|e| format!("read {model_identity_path}: {e}"))?;
            let executable: ExecutablePlanIR = serde_yaml::from_str(&executable_raw)
                .map_err(|e| format!("parse {executable_path}: {e}"))?;
            let model: ModelArtifactIdentity = serde_yaml::from_str(&model_raw)
                .map_err(|e| format!("parse {model_identity_path}: {e}"))?;

            let request = prepare_local_benchmark_request(
                executable,
                model,
                concurrency,
                BenchmarkConfig {
                    prompt,
                    max_tokens: 64,
                    warmup_requests: 1,
                    measured_requests,
                    request_timeout_ms: 120_000,
                    startup_timeout_ms: 300_000,
                },
            )?;

            let bundle = run_local_benchmark(request)?;
            let yaml = serde_yaml::to_string(&bundle).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "benchmark-local" => {
            let request_path = args
                .get(2)
                .ok_or_else(|| "usage: meshfit benchmark-local <request.yaml>".to_string())?;
            let raw = fs::read_to_string(request_path)
                .map_err(|e| format!("read {request_path}: {e}"))?;
            let request: BenchmarkRequestIR =
                serde_yaml::from_str(&raw).map_err(|e| format!("parse {request_path}: {e}"))?;
            let bundle = run_local_benchmark(request)?;
            let yaml = serde_yaml::to_string(&bundle).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "benchmark-candidates" => {
            let snapshot_path = args.get(2).ok_or_else(|| {
                "usage: meshfit benchmark-candidates <snapshot.yaml> <target.yaml> <meshfit-plan-id> [--require-distinct]"
                    .to_string()
            })?;
            let target_path = args
                .get(3)
                .ok_or_else(|| "missing target.yaml".to_string())?;
            let meshfit_plan_id = args
                .get(4)
                .ok_or_else(|| "missing meshfit-plan-id".to_string())?;

            let snapshot_raw = fs::read_to_string(snapshot_path)
                .map_err(|e| format!("read {snapshot_path}: {e}"))?;
            let target_raw =
                fs::read_to_string(target_path).map_err(|e| format!("read {target_path}: {e}"))?;
            let snapshot: InfrastructureSnapshot = serde_yaml::from_str(&snapshot_raw)
                .map_err(|e| format!("parse {snapshot_path}: {e}"))?;
            let target: PlacementTargetIR = serde_yaml::from_str(&target_raw)
                .map_err(|e| format!("parse {target_path}: {e}"))?;

            let report = solve(
                &target
                    .clone()
                    .into_scenario(snapshot.infrastructure.clone()),
            );
            let selected = select_benchmark_plans(&report, meshfit_plan_id)?;
            let warnings = benchmark_plan_warnings(&selected);

            let set = BenchmarkPlanSet {
                candidates: selected
                    .into_iter()
                    .map(|(name, strategy, plan)| benchmark_plan_candidate(name, strategy, plan))
                    .collect(),
                warnings,
            };

            let require_distinct = args.iter().any(|arg| arg == "--require-distinct");
            if require_distinct && !set.warnings.is_empty() {
                return Err(format!(
                    "Benchmark 001 candidate set is not publishable: {}",
                    set.warnings.join(" ")
                ));
            }

            let yaml = serde_yaml::to_string(&set).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "benchmark-kit" => {
            let snapshot_path = args.get(2).ok_or_else(|| {
                "usage: meshfit benchmark-kit <snapshot.yaml> <target.yaml> <meshfit-plan-id> <model-path> <model-identity.yaml> [--listen-port N] [--require-ready] [--write-dir DIR]"
                    .to_string()
            })?;
            let target_path = args
                .get(3)
                .ok_or_else(|| "missing target.yaml".to_string())?;
            let meshfit_plan_id = args
                .get(4)
                .ok_or_else(|| "missing meshfit-plan-id".to_string())?;
            let model_path = args
                .get(5)
                .ok_or_else(|| "missing model-path".to_string())?;
            let model_identity_path = args
                .get(6)
                .ok_or_else(|| "missing model-identity.yaml".to_string())?;
            let listen_port = parse_benchmark_listen_port(&args[7..])?;

            let model_identity_raw = fs::read_to_string(model_identity_path)
                .map_err(|e| format!("read {model_identity_path}: {e}"))?;
            let model_identity: ModelArtifactIdentity =
                serde_yaml::from_str(&model_identity_raw)
                    .map_err(|e| format!("parse {model_identity_path}: {e}"))?;

            let snapshot_raw = fs::read_to_string(snapshot_path)
                .map_err(|e| format!("read {snapshot_path}: {e}"))?;
            let target_raw =
                fs::read_to_string(target_path).map_err(|e| format!("read {target_path}: {e}"))?;
            let snapshot: InfrastructureSnapshot = serde_yaml::from_str(&snapshot_raw)
                .map_err(|e| format!("parse {snapshot_path}: {e}"))?;
            let target: PlacementTargetIR = serde_yaml::from_str(&target_raw)
                .map_err(|e| format!("parse {target_path}: {e}"))?;

            if model_identity.model_id != target.model.id {
                return Err(format!(
                    "model identity '{}' does not match target model '{}'",
                    model_identity.model_id, target.model.id
                ));
            }
            if model_identity.artifact_sha256.is_none() && model_identity.revision.is_none() {
                return Err(
                    "model identity must include artifact_sha256 or revision for Benchmark 001"
                        .to_string(),
                );
            }

            let report = solve(
                &target
                    .clone()
                    .into_scenario(snapshot.infrastructure.clone()),
            );
            let selected = select_benchmark_plans(&report, meshfit_plan_id)?;
            let mut warnings = benchmark_plan_warnings(&selected);

            let concurrency = target.workload.concurrency;
            let measured_requests_per_run = default_measured_requests(concurrency);
            let benchmark_id = "benchmark-001".to_string();
            let runs_per_candidate = 2_u32;

            let mut execution_candidates = Vec::new();
            let mut comparison_candidates = Vec::new();

            for (name, strategy, plan) in selected {
                let result_dir = format!("results/{name}");
                let executable_path = format!("artifacts/{name}/executable.yaml");
                let bundles = (1..=runs_per_candidate)
                    .map(|run| format!("{result_dir}/run-{run:02}.yaml"))
                    .collect::<Vec<_>>();

                let compile_request = CompileRequest {
                    plan: plan.clone(),
                    model_path: model_path.clone(),
                    model_id: target.model.id.clone(),
                    context_tokens: target.workload.context_tokens,
                    listen_port,
                    gpu_layers: None,
                    extra_args: vec![],
                };
                let compile_result = compile_plan(&compile_request);

                let (compile_ready, compile_error, compile_command, run_commands) =
                    match compile_result {
                        Ok(_) => {
                            let compile_command = format!(
                                "mkdir -p artifacts/{name} {result_dir} && meshfit compile-snapshot {} {} {} {} --listen-port {} > {}",
                                snapshot_path,
                                target_path,
                                plan.id,
                                model_path,
                                listen_port,
                                executable_path
                            );
                            let run_commands = bundles
                                .iter()
                                .map(|bundle| {
                                    format!(
                                        "meshfit benchmark-auto {} {} --concurrency {} --measured-requests {} > {}",
                                        executable_path,
                                        model_identity_path,
                                        concurrency,
                                        measured_requests_per_run,
                                        bundle
                                    )
                                })
                                .collect::<Vec<_>>();
                            (true, None, Some(compile_command), run_commands)
                        }
                        Err(error) => {
                            let message = error.to_string();
                            warnings.push(format!(
                                "candidate '{name}' plan '{}' is not compile-ready: {message}",
                                plan.id
                            ));
                            (false, Some(message), None, Vec::new())
                        }
                    };

                let benchmark_host = plan
                    .nodes
                    .first()
                    .cloned()
                    .ok_or_else(|| format!("candidate '{name}' has no execution node"))?;

                execution_candidates.push(BenchmarkExecutionCandidate {
                    name: name.to_string(),
                    plan_id: plan.id.clone(),
                    nodes: plan.nodes.clone(),
                    benchmark_host,
                    runtime: plan.runtime.clone(),
                    result_dir,
                    executable_path,
                    compile_ready,
                    compile_error,
                    compile_command,
                    run_commands,
                });

                comparison_candidates.push(BenchmarkExecutionComparisonCandidate {
                    name: name.to_string(),
                    strategy: strategy.to_string(),
                    hourly_cost_usd: plan.hourly_cost_usd,
                    bundles,
                });
            }

            let ready = warnings.is_empty()
                && execution_candidates
                    .iter()
                    .all(|candidate| candidate.compile_ready);

            let kit = BenchmarkExecutionKit {
                benchmark_id: benchmark_id.clone(),
                ready,
                snapshot: snapshot_path.clone(),
                target: target_path.clone(),
                model_path: model_path.clone(),
                model_identity: model_identity_path.clone(),
                concurrency,
                measured_requests_per_run,
                runs_per_candidate,
                listen_port,
                candidates: execution_candidates,
                comparison_manifest: BenchmarkExecutionComparison {
                    benchmark_id,
                    meshfit_candidate: "meshfit".to_string(),
                    objective: "p95_ttft_ms".to_string(),
                    candidates: comparison_candidates,
                },
                warnings,
            };

            if args.iter().any(|arg| arg == "--require-ready") && !kit.ready {
                return Err(format!(
                    "Benchmark 001 execution kit is not ready: {}",
                    if kit.warnings.is_empty() {
                        "one or more candidates failed compiler preflight".to_string()
                    } else {
                        kit.warnings.join(" ")
                    }
                ));
            }

            if let Some(write_dir) = option_value(&args[7..], "--write-dir")? {
                let materialized = materialize_benchmark_kit(
                    &kit,
                    Path::new(write_dir),
                    &snapshot_raw,
                    &target_raw,
                    &model_identity_raw,
                )?;
                println!("{}", materialized.display());
            } else {
                let yaml = serde_yaml::to_string(&kit).map_err(|e| e.to_string())?;
                print!("{yaml}");
            }
        }
        "benchmark-run-one" => {
            let kit_dir = args.get(2).ok_or_else(|| {
                "usage: meshfit benchmark-run-one <kit-dir> <candidate> <run-number> [--host NODE] [--model-path PATH] [--dry-run] [--overwrite]"
                    .to_string()
            })?;
            let candidate_name = args.get(3).ok_or_else(|| "missing candidate".to_string())?;
            let run_number = args
                .get(4)
                .ok_or_else(|| "missing run-number".to_string())?
                .parse::<usize>()
                .map_err(|e| format!("invalid run-number: {e}"))?;
            let declared_host = option_value(&args[5..], "--host")?;
            let model_path_override = option_value(&args[5..], "--model-path")?;
            let dry_run = args.iter().any(|arg| arg == "--dry-run");
            let overwrite = args.iter().any(|arg| arg == "--overwrite");
            let kit_dir = Path::new(kit_dir);

            let plan = inspect_benchmark_run_one_plan(
                kit_dir,
                candidate_name,
                run_number,
                declared_host,
                model_path_override,
            )?;
            if dry_run {
                let yaml = serde_yaml::to_string(&plan).map_err(|e| e.to_string())?;
                print!("{yaml}");
            } else {
                let bundle_path = execute_benchmark_run_one(
                    kit_dir,
                    candidate_name,
                    run_number,
                    declared_host,
                    model_path_override,
                    overwrite,
                )?;
                println!("{}", bundle_path.display());
            }
        }
        "benchmark-run-candidate" => {
            let kit_dir = args.get(2).ok_or_else(|| {
                "usage: meshfit benchmark-run-candidate <kit-dir> <candidate> [--host NODE] [--model-path PATH] [--dry-run] [--resume|--overwrite]"
                    .to_string()
            })?;
            let candidate_name = args.get(3).ok_or_else(|| "missing candidate".to_string())?;
            let declared_host = option_value(&args[4..], "--host")?;
            let model_path_override = option_value(&args[4..], "--model-path")?;
            let dry_run = args.iter().any(|arg| arg == "--dry-run");
            let resume = args.iter().any(|arg| arg == "--resume");
            let overwrite = args.iter().any(|arg| arg == "--overwrite");
            if resume && overwrite {
                return Err("--resume and --overwrite are mutually exclusive".to_string());
            }

            let kit_dir = Path::new(kit_dir);
            let plan = plan_benchmark_candidate_runs(
                kit_dir,
                candidate_name,
                declared_host,
                model_path_override,
                resume,
                overwrite,
            )?;

            if dry_run {
                let yaml = serde_yaml::to_string(&plan).map_err(|e| e.to_string())?;
                print!("{yaml}");
            } else {
                if !plan.preflight_ready {
                    return Err(format!(
                        "Benchmark 001 preflight failed for '{}': {}",
                        candidate_name,
                        if plan.preflight_issues.is_empty() {
                            "unknown preflight failure".to_string()
                        } else {
                            plan.preflight_issues.join(" ")
                        }
                    ));
                }

                for run_number in &plan.pending_runs {
                    execute_benchmark_run_one(
                        kit_dir,
                        candidate_name,
                        *run_number,
                        declared_host,
                        model_path_override,
                        overwrite,
                    )?;
                }

                let status = inspect_benchmark_kit(kit_dir)?;
                let candidate_status = status
                    .candidates
                    .iter()
                    .find(|candidate| candidate.name == *candidate_name)
                    .ok_or_else(|| {
                        format!(
                            "candidate '{}' disappeared from benchmark status after execution",
                            candidate_name
                        )
                    })?;
                if candidate_status.valid_runs != candidate_status.expected_runs {
                    return Err(format!(
                        "candidate '{}' remains incomplete after execution: {}/{} valid runs",
                        candidate_name, candidate_status.valid_runs, candidate_status.expected_runs
                    ));
                }
                println!(
                    "{}: {}/{} valid runs",
                    candidate_name, candidate_status.valid_runs, candidate_status.expected_runs
                );
            }
        }
        "benchmark-run-host" => {
            let kit_dir = args.get(2).ok_or_else(|| {
                "usage: meshfit benchmark-run-host <kit-dir> (--host NODE | --current-host) [--model-path PATH] [--dry-run] [--resume|--overwrite]"
                    .to_string()
            })?;
            let explicit_host = option_value(&args[3..], "--host")?.map(str::to_string);
            let current_host = args.iter().any(|arg| arg == "--current-host");
            if explicit_host.is_some() == current_host {
                return Err("exactly one of --host NODE or --current-host is required".to_string());
            }
            let host = explicit_host.unwrap_or_else(|| discover_local().node.id);
            let model_path_override = option_value(&args[3..], "--model-path")?.map(str::to_string);
            let dry_run = args.iter().any(|arg| arg == "--dry-run");
            let resume = args.iter().any(|arg| arg == "--resume");
            let overwrite = args.iter().any(|arg| arg == "--overwrite");
            if resume && overwrite {
                return Err("--resume and --overwrite are mutually exclusive".to_string());
            }

            let kit_dir = Path::new(kit_dir);
            let plan = plan_benchmark_host_runs(
                kit_dir,
                &host,
                model_path_override.as_deref(),
                resume,
                overwrite,
            )?;
            if dry_run {
                let yaml = serde_yaml::to_string(&plan).map_err(|e| e.to_string())?;
                print!("{yaml}");
            } else {
                let (valid_slots, total_slots) = execute_benchmark_host_plan(kit_dir, &plan)?;
                println!("{}: {}/{} valid slots", plan.host, valid_slots, total_slots);
            }
        }
        "benchmark-worklist" => {
            let kit_dir = args.get(2).ok_or_else(|| {
                "usage: meshfit benchmark-worklist <kit-dir> [--host NODE | --current-host] [--pending-only]"
                    .to_string()
            })?;
            let explicit_host = option_value(&args[3..], "--host")?.map(str::to_string);
            let current_host = args.iter().any(|arg| arg == "--current-host");
            if explicit_host.is_some() && current_host {
                return Err("--host and --current-host are mutually exclusive".to_string());
            }
            let host_filter = if current_host {
                Some(discover_local().node.id)
            } else {
                explicit_host
            };
            let pending_only = args.iter().any(|arg| arg == "--pending-only");
            let worklist = inspect_benchmark_worklist(
                Path::new(kit_dir),
                host_filter.as_deref(),
                pending_only,
            )?;
            let yaml = serde_yaml::to_string(&worklist).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "benchmark-status" => {
            let kit_dir = args.get(2).ok_or_else(|| {
                "usage: meshfit benchmark-status <kit-dir> [--require-complete]".to_string()
            })?;
            let status = inspect_benchmark_kit(Path::new(kit_dir))?;

            if args.iter().any(|arg| arg == "--require-complete") && !status.complete {
                return Err(format!(
                    "Benchmark 001 execution is incomplete: {}/{} valid bundles",
                    status.valid_bundles, status.expected_bundles
                ));
            }

            let yaml = serde_yaml::to_string(&status).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "benchmark-preflight" => {
            let kit_dir = args.get(2).ok_or_else(|| {
                "usage: meshfit benchmark-preflight <kit-dir> <candidate> [--host NODE] [--model-path PATH] [--allow-existing] [--require-ready]".to_string()
            })?;
            let candidate = args
                .get(3)
                .ok_or_else(|| "missing candidate name".to_string())?;
            let declared_host = option_value(&args[4..], "--host")?;
            let model_path_override = option_value(&args[4..], "--model-path")?;
            let allow_existing = args.iter().any(|arg| arg == "--allow-existing");
            let preflight = inspect_benchmark_preflight(
                Path::new(kit_dir),
                candidate,
                declared_host,
                model_path_override,
                allow_existing,
            )?;

            if args.iter().any(|arg| arg == "--require-ready") && !preflight.ready {
                return Err(format!(
                    "Benchmark 001 preflight failed for '{}': {}",
                    candidate,
                    if preflight.issues.is_empty() {
                        "unknown preflight failure".to_string()
                    } else {
                        preflight.issues.join(" ")
                    }
                ));
            }

            let yaml = serde_yaml::to_string(&preflight).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "benchmark-finalize" => {
            let kit_dir = args.get(2).ok_or_else(|| {
                "usage: meshfit benchmark-finalize <kit-dir> [--markdown] [--require-publishable]"
                    .to_string()
            })?;
            let report = finalize_benchmark_kit(Path::new(kit_dir))?;

            if args.iter().any(|arg| arg == "--require-publishable") && !report.publishable {
                return Err(format!(
                    "Benchmark 001 evidence is not publishable: {}",
                    report.evidence_status
                ));
            }

            if args.iter().any(|arg| arg == "--markdown") {
                print!("{}", report.to_markdown());
            } else {
                let yaml = serde_yaml::to_string(&report).map_err(|e| e.to_string())?;
                print!("{yaml}");
            }
        }
        "compare-benchmarks" => {
            let manifest_path = args.get(2).ok_or_else(|| {
                "usage: meshfit compare-benchmarks <comparison.yaml> [--markdown] [--require-publishable]"
                    .to_string()
            })?;
            let report = load_benchmark_comparison_report(Path::new(manifest_path))?;

            if args.iter().any(|arg| arg == "--require-publishable") && !report.publishable {
                return Err(format!(
                    "Benchmark 001 evidence is not publishable: {}",
                    report.evidence_status
                ));
            }

            if args.iter().any(|arg| arg == "--markdown") {
                print!("{}", report.to_markdown());
            } else {
                let yaml = serde_yaml::to_string(&report).map_err(|e| e.to_string())?;
                print!("{yaml}");
            }
        }
        "evidence-from-benchmark" => {
            let bundle_path = args.get(2).ok_or_else(|| {
                "usage: meshfit evidence-from-benchmark <bundle.yaml>".to_string()
            })?;
            let raw =
                fs::read_to_string(bundle_path).map_err(|e| format!("read {bundle_path}: {e}"))?;
            let bundle: BenchmarkBundle =
                serde_yaml::from_str(&raw).map_err(|e| format!("parse {bundle_path}: {e}"))?;
            let records = bundle.to_benchmark_records()?;
            let store = EvidenceStore { records };
            let yaml = serde_yaml::to_string(&store).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "compile" => {
            let request_path = args
                .get(2)
                .ok_or_else(|| "usage: meshfit compile <request.yaml>".to_string())?;
            let raw = fs::read_to_string(request_path)
                .map_err(|e| format!("read {request_path}: {e}"))?;
            let request: CompileRequest =
                serde_yaml::from_str(&raw).map_err(|e| format!("parse {request_path}: {e}"))?;
            let executable = compile_plan(&request).map_err(|e| e.to_string())?;
            let yaml = serde_yaml::to_string(&executable).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "compile-snapshot" => {
            let snapshot_path = args
                .get(2)
                .ok_or_else(|| "usage: meshfit compile-snapshot <snapshot.yaml> <target.yaml> <plan-id> <model-path> [gpu-layers] [--listen-port N] [--listen-port N]".to_string())?;
            let target_path = args
                .get(3)
                .ok_or_else(|| "missing target.yaml".to_string())?;
            let plan_id = args.get(4).ok_or_else(|| "missing plan-id".to_string())?;
            let model_path = args
                .get(5)
                .ok_or_else(|| "missing model-path".to_string())?;
            let trailing = &args[6..];
            let gpu_layers = trailing
                .first()
                .filter(|value| !value.starts_with("--"))
                .map(|value| {
                    value
                        .parse::<u32>()
                        .map_err(|e| format!("invalid gpu-layers '{value}': {e}"))
                })
                .transpose()?;
            let listen_port = parse_benchmark_listen_port(trailing)?;

            let snapshot_raw = fs::read_to_string(snapshot_path)
                .map_err(|e| format!("read {snapshot_path}: {e}"))?;
            let target_raw =
                fs::read_to_string(target_path).map_err(|e| format!("read {target_path}: {e}"))?;
            let snapshot: InfrastructureSnapshot = serde_yaml::from_str(&snapshot_raw)
                .map_err(|e| format!("parse {snapshot_path}: {e}"))?;
            let target: PlacementTargetIR = serde_yaml::from_str(&target_raw)
                .map_err(|e| format!("parse {target_path}: {e}"))?;

            let scenario = target
                .clone()
                .into_scenario(snapshot.infrastructure.clone());
            let report = solve(&scenario);
            let plan = report
                .feasible
                .iter()
                .find(|plan| &plan.id == plan_id)
                .cloned()
                .ok_or_else(|| {
                    format!("plan-id '{plan_id}' is not feasible in the current snapshot")
                })?;

            let request = CompileRequest {
                plan,
                model_path: model_path.clone(),
                model_id: target.model.id.clone(),
                context_tokens: target.workload.context_tokens,
                listen_port,
                gpu_layers,
                extra_args: vec![],
            };

            let executable = compile_plan(&request).map_err(|e| e.to_string())?;
            let yaml = serde_yaml::to_string(&executable).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "plan-snapshot" => {
            let snapshot_path = args.get(2).ok_or_else(|| {
                "usage: meshfit plan-snapshot <snapshot.yaml> <target.yaml>".to_string()
            })?;
            let target_path = args.get(3).ok_or_else(|| {
                "usage: meshfit plan-snapshot <snapshot.yaml> <target.yaml>".to_string()
            })?;

            let snapshot_raw = fs::read_to_string(snapshot_path)
                .map_err(|e| format!("read {snapshot_path}: {e}"))?;
            let target_raw =
                fs::read_to_string(target_path).map_err(|e| format!("read {target_path}: {e}"))?;
            let snapshot: InfrastructureSnapshot = serde_yaml::from_str(&snapshot_raw)
                .map_err(|e| format!("parse {snapshot_path}: {e}"))?;
            let target: PlacementTargetIR = serde_yaml::from_str(&target_raw)
                .map_err(|e| format!("parse {target_path}: {e}"))?;

            let scenario = target.into_scenario(snapshot.infrastructure);
            let report = solve(&scenario);
            print_report(&report);
        }
        "plan" => {
            let path = args
                .get(2)
                .ok_or_else(|| "usage: meshfit plan <scenario.yaml>".to_string())?;
            let raw = fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            let scenario: ScenarioIR =
                serde_yaml::from_str(&raw).map_err(|e| format!("parse {path}: {e}"))?;
            let report = solve(&scenario);
            print_report(&report);
        }
        "predict" => {
            let evidence_path = args
                .get(2)
                .ok_or_else(|| "usage: meshfit predict <evidence.yaml> <query.yaml>".to_string())?;
            let query_path = args
                .get(3)
                .ok_or_else(|| "usage: meshfit predict <evidence.yaml> <query.yaml>".to_string())?;
            let evidence_raw = fs::read_to_string(evidence_path)
                .map_err(|e| format!("read {evidence_path}: {e}"))?;
            let query_raw =
                fs::read_to_string(query_path).map_err(|e| format!("read {query_path}: {e}"))?;
            let evidence: EvidenceStore = serde_yaml::from_str(&evidence_raw)
                .map_err(|e| format!("parse {evidence_path}: {e}"))?;
            let query: PredictionQuery =
                serde_yaml::from_str(&query_raw).map_err(|e| format!("parse {query_path}: {e}"))?;
            let prediction = evidence.predict_exact(&query);
            print_prediction(&prediction);
        }
        other => return Err(format!("unknown command '{other}'")),
    }

    Ok(())
}

fn load_benchmark_comparison_report(
    manifest_path: &Path,
) -> Result<BenchmarkComparisonReport, String> {
    let raw = fs::read_to_string(manifest_path)
        .map_err(|e| format!("read {}: {e}", manifest_path.display()))?;
    let manifest: BenchmarkComparisonManifest = serde_yaml::from_str(&raw)
        .map_err(|e| format!("parse {}: {e}", manifest_path.display()))?;
    let base_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));

    let mut candidates = Vec::new();
    for candidate in manifest.candidates {
        let mut bundles = Vec::new();
        for bundle_file in candidate.bundles {
            let path = base_dir.join(&bundle_file);
            let bundle_raw =
                fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
            let bundle: BenchmarkBundle = serde_yaml::from_str(&bundle_raw)
                .map_err(|e| format!("parse {}: {e}", path.display()))?;
            bundles.push(bundle);
        }

        candidates.push(BenchmarkCandidate {
            name: candidate.name,
            strategy: candidate.strategy,
            hourly_cost_usd: candidate.hourly_cost_usd,
            bundles,
        });
    }

    compare_benchmarks(&BenchmarkComparisonRequest {
        benchmark_id: manifest.benchmark_id,
        meshfit_candidate: manifest.meshfit_candidate,
        objective: manifest.objective,
        candidates,
    })
}

fn finalize_benchmark_kit(kit_dir: &Path) -> Result<BenchmarkComparisonReport, String> {
    let status = inspect_benchmark_kit(kit_dir)?;
    if !status.complete {
        let invalid = status
            .candidates
            .iter()
            .map(|candidate| candidate.invalid_bundles.len())
            .sum::<usize>();
        return Err(format!(
            "Benchmark 001 is incomplete: {}/{} valid bundles, {} invalid bundle(s). Run benchmark-worklist or benchmark-status before finalizing.",
            status.valid_bundles, status.expected_bundles, invalid
        ));
    }

    load_benchmark_comparison_report(&kit_dir.join("comparison.yaml"))
}

fn benchmark_execution_lock_target(listen_port: u16) -> PathBuf {
    env::temp_dir().join(format!("meshfit-benchmark-port-{listen_port}"))
}

fn benchmark_listen_port_available(listen_port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", listen_port)).is_ok()
}

fn acquire_benchmark_file_lock(
    target_path: &Path,
    kind: &str,
) -> Result<BenchmarkRunSlotLock, String> {
    let lock_path = PathBuf::from(format!("{}.lock", target_path.display()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(_) => Ok(BenchmarkRunSlotLock { path: lock_path }),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(format!(
            "{kind} '{}' is already locked by another benchmark process; if no process is active, remove '{}'",
            target_path.display(),
            lock_path.display()
        )),
        Err(error) => Err(format!(
            "create {kind} lock '{}': {error}",
            lock_path.display()
        )),
    }
}

fn commit_benchmark_bundle(
    temp_path: &Path,
    bundle_path: &Path,
    overwrite: bool,
) -> Result<(), String> {
    if !bundle_path.exists() {
        return fs::rename(temp_path, bundle_path).map_err(|e| {
            format!(
                "move {} to {}: {e}",
                temp_path.display(),
                bundle_path.display()
            )
        });
    }

    if !overwrite {
        return Err(format!(
            "bundle '{}' already exists; refusing replacement without --overwrite",
            bundle_path.display()
        ));
    }

    let backup_path = bundle_path.with_extension("yaml.meshfit-backup");
    if backup_path.exists() {
        return Err(format!(
            "backup '{}' already exists; refusing to overwrite evidence until it is resolved",
            backup_path.display()
        ));
    }

    fs::rename(bundle_path, &backup_path).map_err(|e| {
        format!(
            "move existing evidence {} to backup {}: {e}",
            bundle_path.display(),
            backup_path.display()
        )
    })?;

    match fs::rename(temp_path, bundle_path) {
        Ok(()) => {
            fs::remove_file(&backup_path).map_err(|e| {
                format!(
                    "replacement committed to '{}' but backup '{}' could not be removed: {e}",
                    bundle_path.display(),
                    backup_path.display()
                )
            })?;
            Ok(())
        }
        Err(commit_error) => match fs::rename(&backup_path, bundle_path) {
            Ok(()) => Err(format!(
                "replacement of '{}' failed: {commit_error}; previous evidence was restored",
                bundle_path.display()
            )),
            Err(restore_error) => Err(format!(
                "replacement of '{}' failed: {commit_error}; restoring backup '{}' also failed: {restore_error}",
                bundle_path.display(),
                backup_path.display()
            )),
        },
    }
}

fn acquire_benchmark_run_slot(bundle_path: &Path) -> Result<BenchmarkRunSlotLock, String> {
    acquire_benchmark_file_lock(bundle_path, "run slot")
}

fn acquire_benchmark_compile_lock(executable_path: &Path) -> Result<BenchmarkRunSlotLock, String> {
    acquire_benchmark_file_lock(executable_path, "executable")
}

fn inspect_benchmark_run_one_plan(
    kit_dir: &Path,
    candidate_name: &str,
    run_number: usize,
    declared_host: Option<&str>,
    model_path_override: Option<&str>,
) -> Result<BenchmarkRunOnePlan, String> {
    let kit = load_benchmark_execution_kit(kit_dir)?;
    let candidate = kit
        .candidates
        .iter()
        .find(|candidate| candidate.name == candidate_name)
        .ok_or_else(|| format!("unknown benchmark candidate '{candidate_name}'"))?;
    let comparison = kit
        .comparison_manifest
        .candidates
        .iter()
        .find(|item| item.name == candidate.name)
        .ok_or_else(|| {
            format!(
                "candidate '{}' is missing from comparison manifest",
                candidate.name
            )
        })?;
    if run_number == 0 || run_number > comparison.bundles.len() {
        return Err(format!(
            "run-number must be between 1 and {} for candidate '{}'",
            comparison.bundles.len(),
            candidate.name
        ));
    }

    let preflight = inspect_benchmark_preflight(
        kit_dir,
        candidate_name,
        declared_host,
        model_path_override,
        true,
    )?;
    let executable_path = kit_dir.join(&candidate.executable_path);
    let bundle_path = kit_dir.join(&comparison.bundles[run_number - 1]);
    let compile_required = !executable_matches_contract(&executable_path, &preflight.model_path, kit.listen_port)?;

    Ok(BenchmarkRunOnePlan {
        benchmark_id: kit.benchmark_id,
        kit_ready: kit.ready,
        candidate: candidate.name.clone(),
        plan_id: candidate.plan_id.clone(),
        runtime: candidate.runtime.clone(),
        benchmark_host: candidate.benchmark_host.clone(),
        observed_host: preflight.observed_host,
        host_match: preflight.host_match,
        preflight_ready: preflight.ready,
        preflight_issues: preflight.issues,
        model_path: preflight.model_path,
        model_path_overridden: preflight.model_path_overridden,
        model_artifact_verified: preflight.model_artifact_verified,
        listen_port: preflight.listen_port,
        run_number,
        executable_path: executable_path.display().to_string(),
        bundle_path: bundle_path.display().to_string(),
        compile_required,
        bundle_exists: bundle_path.is_file(),
    })
}

fn validate_existing_candidate_bundle(
    bundle_path: &Path,
    expected_plan_id: &str,
) -> Result<(), String> {
    let raw = fs::read_to_string(bundle_path)
        .map_err(|e| format!("read existing bundle {}: {e}", bundle_path.display()))?;
    let bundle: BenchmarkBundle = serde_yaml::from_str(&raw)
        .map_err(|e| format!("parse existing bundle {}: {e}", bundle_path.display()))?;
    bundle
        .validate()
        .map_err(|e| format!("invalid existing bundle {}: {e}", bundle_path.display()))?;
    if bundle.request.executable.source_plan_id != expected_plan_id {
        return Err(format!(
            "existing bundle '{}' belongs to plan '{}' instead of expected plan '{}'",
            bundle_path.display(),
            bundle.request.executable.source_plan_id,
            expected_plan_id
        ));
    }
    Ok(())
}

fn plan_benchmark_candidate_runs(
    kit_dir: &Path,
    candidate_name: &str,
    declared_host: Option<&str>,
    model_path_override: Option<&str>,
    resume: bool,
    overwrite: bool,
) -> Result<BenchmarkRunCandidatePlan, String> {
    let kit = load_benchmark_execution_kit(kit_dir)?;
    let candidate = kit
        .candidates
        .iter()
        .find(|candidate| candidate.name == candidate_name)
        .ok_or_else(|| format!("unknown benchmark candidate '{candidate_name}'"))?;
    let comparison = kit
        .comparison_manifest
        .candidates
        .iter()
        .find(|item| item.name == candidate.name)
        .ok_or_else(|| {
            format!(
                "candidate '{}' is missing from comparison manifest",
                candidate.name
            )
        })?;

    let preflight = inspect_benchmark_preflight(
        kit_dir,
        candidate_name,
        declared_host,
        model_path_override,
        true,
    )?;
    let mut existing_valid_runs = Vec::new();
    let mut pending_runs = Vec::new();

    for (index, bundle_rel) in comparison.bundles.iter().enumerate() {
        let run_number = index + 1;
        let bundle_path = kit_dir.join(bundle_rel);
        if !bundle_path.exists() {
            pending_runs.push(run_number);
            continue;
        }
        if overwrite {
            pending_runs.push(run_number);
            continue;
        }
        if resume {
            validate_existing_candidate_bundle(&bundle_path, &candidate.plan_id)?;
            existing_valid_runs.push(run_number);
            continue;
        }
        return Err(format!(
            "bundle '{}' already exists before candidate execution; use --resume for validated evidence or --overwrite for explicit replacement",
            bundle_path.display()
        ));
    }

    Ok(BenchmarkRunCandidatePlan {
        benchmark_id: kit.benchmark_id,
        candidate: candidate.name.clone(),
        benchmark_host: candidate.benchmark_host.clone(),
        preflight_ready: preflight.ready,
        preflight_issues: preflight.issues,
        model_path: preflight.model_path,
        model_path_overridden: preflight.model_path_overridden,
        model_artifact_verified: preflight.model_artifact_verified,
        listen_port: preflight.listen_port,
        total_runs: comparison.bundles.len(),
        existing_valid_runs,
        pending_runs,
        resume,
        overwrite,
    })
}

fn plan_benchmark_host_runs(
    kit_dir: &Path,
    host: &str,
    model_path_override: Option<&str>,
    resume: bool,
    overwrite: bool,
) -> Result<BenchmarkRunHostPlan, String> {
    let kit = load_benchmark_execution_kit(kit_dir)?;
    let assigned = kit
        .candidates
        .iter()
        .filter(|candidate| candidate.benchmark_host == host)
        .map(|candidate| candidate.name.clone())
        .collect::<Vec<_>>();

    if assigned.is_empty() {
        return Err(format!(
            "benchmark host '{host}' has no assigned candidates in this kit"
        ));
    }

    let mut issues = Vec::new();
    let mut candidates = Vec::new();

    for candidate_name in assigned {
        match plan_benchmark_candidate_runs(
            kit_dir,
            &candidate_name,
            Some(host),
            model_path_override,
            resume,
            overwrite,
        ) {
            Ok(plan) => {
                if !plan.preflight_ready {
                    if plan.preflight_issues.is_empty() {
                        issues.push(format!(
                            "candidate '{}' is not preflight-ready",
                            candidate_name
                        ));
                    } else {
                        for issue in &plan.preflight_issues {
                            issues.push(format!("candidate '{}': {issue}", candidate_name));
                        }
                    }
                }
                candidates.push(plan);
            }
            Err(error) => {
                issues.push(format!("candidate '{}': {error}", candidate_name));
            }
        }
    }

    Ok(BenchmarkRunHostPlan {
        benchmark_id: kit.benchmark_id,
        host: host.to_string(),
        ready: issues.is_empty(),
        issues,
        model_path_override: model_path_override.map(str::to_string),
        listen_port: kit.listen_port,
        resume,
        overwrite,
        candidates,
    })
}

fn execute_benchmark_host_plan(
    kit_dir: &Path,
    plan: &BenchmarkRunHostPlan,
) -> Result<(usize, usize), String> {
    if !plan.ready {
        return Err(format!(
            "Benchmark 001 host preflight failed for '{}': {}",
            plan.host,
            if plan.issues.is_empty() {
                "unknown host preflight failure".to_string()
            } else {
                plan.issues.join(" ")
            }
        ));
    }

    for candidate_plan in &plan.candidates {
        for run_number in &candidate_plan.pending_runs {
            execute_benchmark_run_one(
                kit_dir,
                &candidate_plan.candidate,
                *run_number,
                Some(plan.host.as_str()),
                plan.model_path_override.as_deref(),
                plan.overwrite,
            )?;
        }
    }

    let worklist = inspect_benchmark_worklist(kit_dir, Some(&plan.host), false)?;
    if worklist.valid_slots != worklist.total_slots
        || worklist.pending_slots != 0
        || worklist.locked_slots != 0
        || worklist.invalid_slots != 0
    {
        return Err(format!(
            "host '{}' remains incomplete after execution: {}/{} valid slots, {} pending, {} locked, {} invalid",
            plan.host,
            worklist.valid_slots,
            worklist.total_slots,
            worklist.pending_slots,
            worklist.locked_slots,
            worklist.invalid_slots
        ));
    }

    Ok((worklist.valid_slots, worklist.total_slots))
}

fn execute_benchmark_run_one(
    kit_dir: &Path,
    candidate_name: &str,
    run_number: usize,
    declared_host: Option<&str>,
    model_path_override: Option<&str>,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let plan = inspect_benchmark_run_one_plan(
        kit_dir,
        candidate_name,
        run_number,
        declared_host,
        model_path_override,
    )?;
    if !plan.kit_ready {
        return Err("Benchmark 001 execution kit is not ready; refusing real benchmark run".into());
    }
    if !plan.preflight_ready {
        return Err(format!(
            "Benchmark 001 preflight failed for '{}': {}",
            candidate_name,
            if plan.preflight_issues.is_empty() {
                "unknown preflight failure".to_string()
            } else {
                plan.preflight_issues.join(" ")
            }
        ));
    }

    let kit = load_benchmark_execution_kit(kit_dir)?;
    let candidate = kit
        .candidates
        .iter()
        .find(|candidate| candidate.name == candidate_name)
        .ok_or_else(|| format!("unknown benchmark candidate '{candidate_name}'"))?;
    let bundle_path = PathBuf::from(&plan.bundle_path);
    let executable_path = PathBuf::from(&plan.executable_path);
    let execution_marker = benchmark_execution_lock_target(kit.listen_port);
    let _execution_lock = acquire_benchmark_file_lock(&execution_marker, "benchmark execution")?;

    if bundle_path.exists() && !overwrite {
        return Err(format!(
            "bundle '{}' already exists; pass --overwrite to replace it",
            bundle_path.display()
        ));
    }
    if let Some(parent) = bundle_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let _slot_lock = acquire_benchmark_run_slot(&bundle_path)?;

    ensure_candidate_executable(kit_dir, &kit, candidate, &executable_path, &plan.model_path)?;
    let executable_raw = fs::read_to_string(&executable_path)
        .map_err(|e| format!("read {}: {e}", executable_path.display()))?;
    let executable: ExecutablePlanIR = serde_yaml::from_str(&executable_raw)
        .map_err(|e| format!("parse {}: {e}", executable_path.display()))?;
    if executable.source_plan_id != candidate.plan_id {
        return Err(format!(
            "executable source plan '{}' does not match candidate plan '{}'",
            executable.source_plan_id, candidate.plan_id
        ));
    }

    let model_identity_path = kit_dir.join(&kit.model_identity);
    let model_identity_raw = fs::read_to_string(&model_identity_path)
        .map_err(|e| format!("read {}: {e}", model_identity_path.display()))?;
    let model_identity: ModelArtifactIdentity = serde_yaml::from_str(&model_identity_raw)
        .map_err(|e| format!("parse {}: {e}", model_identity_path.display()))?;
    let request = prepare_local_benchmark_request(
        executable,
        model_identity,
        kit.concurrency,
        BenchmarkConfig {
            prompt: "Explain MeshFit in one sentence.".to_string(),
            max_tokens: 64,
            warmup_requests: 1,
            measured_requests: kit.measured_requests_per_run,
            request_timeout_ms: 120_000,
            startup_timeout_ms: 300_000,
        },
    )?;
    let bundle = run_local_benchmark(request)?;
    bundle.validate()?;

    let bundle_yaml = serde_yaml::to_string(&bundle).map_err(|e| e.to_string())?;
    let temp_path = bundle_path.with_extension("yaml.tmp");
    fs::write(&temp_path, bundle_yaml)
        .map_err(|e| format!("write {}: {e}", temp_path.display()))?;
    commit_benchmark_bundle(&temp_path, &bundle_path, overwrite)?;

    Ok(bundle_path)
}

fn load_benchmark_execution_kit(kit_dir: &Path) -> Result<BenchmarkExecutionKit, String> {
    let kit_path = kit_dir.join("kit.yaml");
    let raw =
        fs::read_to_string(&kit_path).map_err(|e| format!("read {}: {e}", kit_path.display()))?;
    serde_yaml::from_str(&raw).map_err(|e| format!("parse {}: {e}", kit_path.display()))
}

fn executable_matches_contract(
    executable_path: &Path,
    model_path: &str,
    listen_port: u16,
) -> Result<bool, String> {
    if !executable_path.is_file() {
        return Ok(false);
    }
    let raw = fs::read_to_string(executable_path)
        .map_err(|e| format!("read {}: {e}", executable_path.display()))?;
    let executable: ExecutablePlanIR = serde_yaml::from_str(&raw)
        .map_err(|e| format!("parse {}: {e}", executable_path.display()))?;
    Ok(executable.model_source == model_path && executable.service.port == listen_port)
}

fn inspect_model_path_for_host(
    kit_dir: &Path,
    kit: &BenchmarkExecutionKit,
    override_path: Option<&str>,
) -> Result<BenchmarkModelPathCheck, String> {
    let overridden = override_path.is_some();
    let effective_path = override_path.unwrap_or(&kit.model_path).trim().to_string();
    if effective_path.is_empty() {
        return Err("benchmark model path must not be empty".to_string());
    }

    let path = Path::new(&effective_path);
    if overridden {
        if !path.is_file() {
            return Ok(BenchmarkModelPathCheck {
                path: effective_path.clone(),
                status: "local_override_missing".to_string(),
                overridden: true,
                verified: false,
                issue: Some(format!(
                    "overridden local model artifact '{}' does not exist on this host",
                    effective_path
                )),
                warnings: Vec::new(),
            });
        }

        let effective_path = path
            .canonicalize()
            .map_err(|e| format!("canonicalize model override '{}': {e}", path.display()))?
            .to_string_lossy()
            .into_owned();
        let path = Path::new(&effective_path);

        let identity_path = kit_dir.join(&kit.model_identity);
        let identity_raw = fs::read_to_string(&identity_path)
            .map_err(|e| format!("read {}: {e}", identity_path.display()))?;
        let expected: ModelArtifactIdentity = serde_yaml::from_str(&identity_raw)
            .map_err(|e| format!("parse {}: {e}", identity_path.display()))?;
        let Some(expected_sha) = expected.artifact_sha256.as_deref() else {
            return Ok(BenchmarkModelPathCheck {
                path: effective_path,
                status: "local_override_unverifiable".to_string(),
                overridden: true,
                verified: false,
                issue: Some(
                    "model path override requires artifact_sha256 in the materialized model identity"
                        .to_string(),
                ),
                warnings: Vec::new(),
            });
        };

        let observed = inspect_model_artifact(
            path,
            expected.model_id.clone(),
            expected.format.clone(),
            expected.quantization.clone(),
            expected.revision.clone(),
        )?;
        if observed.artifact_sha256.as_deref() != Some(expected_sha) {
            return Ok(BenchmarkModelPathCheck {
                path: effective_path,
                status: "local_override_hash_mismatch".to_string(),
                overridden: true,
                verified: false,
                issue: Some(
                    "overridden model artifact SHA-256 does not match the materialized model identity"
                        .to_string(),
                ),
                warnings: Vec::new(),
            });
        }

        return Ok(BenchmarkModelPathCheck {
            path: effective_path,
            status: "local_override_verified".to_string(),
            overridden: true,
            verified: true,
            issue: None,
            warnings: Vec::new(),
        });
    }

    let model_is_explicit_local =
        path.is_absolute() || effective_path.starts_with("./") || effective_path.starts_with("../");
    if path.is_file() {
        return Ok(BenchmarkModelPathCheck {
            path: effective_path,
            status: "local_present".to_string(),
            overridden: false,
            verified: false,
            issue: None,
            warnings: Vec::new(),
        });
    }
    if model_is_explicit_local {
        return Ok(BenchmarkModelPathCheck {
            path: effective_path.clone(),
            status: "local_missing".to_string(),
            overridden: false,
            verified: false,
            issue: Some(format!(
                "local model artifact '{}' does not exist on this host",
                effective_path
            )),
            warnings: Vec::new(),
        });
    }

    Ok(BenchmarkModelPathCheck {
        path: effective_path.clone(),
        status: "runtime_resolved_unverified".to_string(),
        overridden: false,
        verified: false,
        issue: None,
        warnings: vec![format!(
            "model path '{}' is not a local file; runtime resolution has not been verified",
            effective_path
        )],
    })
}

fn commit_executable_artifact(temp_path: &Path, executable_path: &Path) -> Result<(), String> {
    if !executable_path.exists() {
        return fs::rename(temp_path, executable_path).map_err(|e| {
            format!(
                "move {} to {}: {e}",
                temp_path.display(),
                executable_path.display()
            )
        });
    }

    let backup_path = PathBuf::from(format!("{}.meshfit-backup", executable_path.display()));
    if backup_path.exists() {
        return Err(format!(
            "executable backup '{}' already exists; refusing replacement until it is resolved",
            backup_path.display()
        ));
    }

    fs::rename(executable_path, &backup_path).map_err(|e| {
        format!(
            "move existing executable {} to backup {}: {e}",
            executable_path.display(),
            backup_path.display()
        )
    })?;

    match fs::rename(temp_path, executable_path) {
        Ok(()) => {
            fs::remove_file(&backup_path).map_err(|e| {
                format!(
                    "replacement committed to '{}' but backup '{}' could not be removed: {e}",
                    executable_path.display(),
                    backup_path.display()
                )
            })?;
            Ok(())
        }
        Err(commit_error) => match fs::rename(&backup_path, executable_path) {
            Ok(()) => Err(format!(
                "replacement of executable '{}' failed: {commit_error}; previous artifact was restored",
                executable_path.display()
            )),
            Err(restore_error) => Err(format!(
                "replacement of executable '{}' failed: {commit_error}; restoring backup '{}' also failed: {restore_error}",
                executable_path.display(),
                backup_path.display()
            )),
        },
    }
}

fn ensure_candidate_executable(
    kit_dir: &Path,
    kit: &BenchmarkExecutionKit,
    candidate: &BenchmarkExecutionCandidate,
    executable_path: &Path,
    model_path: &str,
) -> Result<(), String> {
    if executable_matches_contract(executable_path, model_path, kit.listen_port)? {
        return Ok(());
    }

    if let Some(parent) = executable_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let _compile_lock = acquire_benchmark_compile_lock(executable_path)?;
    if executable_matches_contract(executable_path, model_path, kit.listen_port)? {
        return Ok(());
    }

    let snapshot_path = kit_dir.join(&kit.snapshot);
    let target_path = kit_dir.join(&kit.target);
    let snapshot_raw = fs::read_to_string(&snapshot_path)
        .map_err(|e| format!("read {}: {e}", snapshot_path.display()))?;
    let target_raw = fs::read_to_string(&target_path)
        .map_err(|e| format!("read {}: {e}", target_path.display()))?;
    let snapshot: InfrastructureSnapshot = serde_yaml::from_str(&snapshot_raw)
        .map_err(|e| format!("parse {}: {e}", snapshot_path.display()))?;
    let target: PlacementTargetIR = serde_yaml::from_str(&target_raw)
        .map_err(|e| format!("parse {}: {e}", target_path.display()))?;

    let report = solve(
        &target
            .clone()
            .into_scenario(snapshot.infrastructure.clone()),
    );
    let plan = report
        .feasible
        .iter()
        .find(|plan| plan.id == candidate.plan_id)
        .cloned()
        .ok_or_else(|| {
            format!(
                "candidate plan '{}' is not feasible in the materialized snapshot",
                candidate.plan_id
            )
        })?;

    let request = CompileRequest {
        plan,
        model_path: model_path.to_string(),
        model_id: target.model.id.clone(),
        context_tokens: target.workload.context_tokens,
        listen_port: kit.listen_port,
        gpu_layers: None,
        extra_args: vec![],
    };
    let executable = compile_plan(&request).map_err(|e| e.to_string())?;
    if executable.source_plan_id != candidate.plan_id {
        return Err(format!(
            "compiled executable source plan '{}' does not match candidate plan '{}'",
            executable.source_plan_id, candidate.plan_id
        ));
    }

    let yaml = serde_yaml::to_string(&executable).map_err(|e| e.to_string())?;
    let temp_path = PathBuf::from(format!("{}.tmp", executable_path.display()));
    fs::write(&temp_path, yaml).map_err(|e| format!("write {}: {e}", temp_path.display()))?;
    commit_executable_artifact(&temp_path, executable_path)
}

fn benchmark_preflight_issues(
    kit_ready: bool,
    candidate: &BenchmarkExecutionCandidate,
    observed_host: &str,
    runtime_found: bool,
    model_issue: Option<String>,
    existing_bundle_count: usize,
    allow_existing: bool,
) -> Vec<String> {
    let mut issues = Vec::new();

    if !kit_ready {
        issues.push("execution kit is not marked ready".to_string());
    }
    if !candidate.compile_ready {
        issues.push(format!(
            "candidate is not compiler-ready{}",
            candidate
                .compile_error
                .as_deref()
                .map(|error| format!(": {error}"))
                .unwrap_or_default()
        ));
    }
    if observed_host != candidate.benchmark_host {
        issues.push(format!(
            "wrong benchmark host: candidate requires '{}' but current/declarative host is '{}'",
            candidate.benchmark_host, observed_host
        ));
    }
    if !runtime_found {
        issues.push(format!(
            "required runtime '{}' was not discovered on PATH",
            candidate.runtime
        ));
    }
    if let Some(issue) = model_issue {
        issues.push(issue);
    }
    if existing_bundle_count > 0 && !allow_existing {
        issues.push(format!(
            "{existing_bundle_count} result bundle(s) already exist; refusing accidental evidence overwrite"
        ));
    }

    issues
}

fn inspect_benchmark_preflight(
    kit_dir: &Path,
    candidate_name: &str,
    declared_host: Option<&str>,
    model_path_override: Option<&str>,
    allow_existing: bool,
) -> Result<BenchmarkPreflight, String> {
    let kit_path = kit_dir.join("kit.yaml");
    let raw =
        fs::read_to_string(&kit_path).map_err(|e| format!("read {}: {e}", kit_path.display()))?;
    let kit: BenchmarkExecutionKit =
        serde_yaml::from_str(&raw).map_err(|e| format!("parse {}: {e}", kit_path.display()))?;
    let candidate = kit
        .candidates
        .iter()
        .find(|candidate| candidate.name == candidate_name)
        .ok_or_else(|| format!("candidate '{candidate_name}' is not present in kit.yaml"))?;

    let local = discover_local();
    let observed_host = declared_host
        .map(str::to_string)
        .unwrap_or_else(|| local.node.id.clone());
    let host_match = observed_host == candidate.benchmark_host;
    let runtime_discovery = discover_runtimes();
    let runtime_found = runtime_discovery
        .runtimes
        .iter()
        .any(|runtime| runtime.runtime == candidate.runtime);

    let model_check = inspect_model_path_for_host(kit_dir, &kit, model_path_override)?;

    let comparison = kit
        .comparison_manifest
        .candidates
        .iter()
        .find(|item| item.name == candidate.name)
        .ok_or_else(|| {
            format!(
                "candidate '{}' is missing from comparison manifest",
                candidate.name
            )
        })?;
    let existing_bundles = comparison
        .bundles
        .iter()
        .filter(|bundle| kit_dir.join(bundle).exists())
        .cloned()
        .collect::<Vec<_>>();

    let listen_port_available = benchmark_listen_port_available(kit.listen_port);
    let mut issues = benchmark_preflight_issues(
        kit.ready,
        candidate,
        &observed_host,
        runtime_found,
        model_check.issue.clone(),
        existing_bundles.len(),
        allow_existing,
    );
    if !listen_port_available {
        issues.push(format!(
            "benchmark listen port {} is not currently available on this host",
            kit.listen_port
        ));
    }
    let mut warnings = model_check.warnings.clone();
    warnings.extend(local.warnings);
    warnings.extend(runtime_discovery.warnings);

    Ok(BenchmarkPreflight {
        benchmark_id: kit.benchmark_id,
        candidate: candidate.name.clone(),
        ready: issues.is_empty(),
        expected_host: candidate.benchmark_host.clone(),
        observed_host,
        host_match,
        runtime: candidate.runtime.clone(),
        runtime_found,
        model_path: model_check.path,
        model_path_status: model_check.status,
        model_path_overridden: model_check.overridden,
        model_artifact_verified: model_check.verified,
        listen_port: kit.listen_port,
        listen_port_available,
        existing_bundles,
        issues,
        warnings,
    })
}

fn inspect_benchmark_worklist(
    kit_dir: &Path,
    host_filter: Option<&str>,
    pending_only: bool,
) -> Result<BenchmarkWorklist, String> {
    let kit = load_benchmark_execution_kit(kit_dir)?;
    let mut hosts = std::collections::BTreeMap::<String, BenchmarkHostWork>::new();

    for candidate in &kit.candidates {
        if host_filter.is_some_and(|host| host != candidate.benchmark_host) {
            continue;
        }

        let comparison = kit
            .comparison_manifest
            .candidates
            .iter()
            .find(|item| item.name == candidate.name)
            .ok_or_else(|| {
                format!(
                    "candidate '{}' is missing from comparison manifest",
                    candidate.name
                )
            })?;

        for (index, bundle_rel) in comparison.bundles.iter().enumerate() {
            let run_number = index + 1;
            let bundle_path = kit_dir.join(bundle_rel);
            let lock_path = PathBuf::from(format!("{}.lock", bundle_path.display()));
            let (state, error) = inspect_work_slot(&bundle_path, &candidate.plan_id, &lock_path);

            let host = hosts
                .entry(candidate.benchmark_host.clone())
                .or_insert_with(|| BenchmarkHostWork {
                    host: candidate.benchmark_host.clone(),
                    total_slots: 0,
                    valid_slots: 0,
                    pending_slots: 0,
                    locked_slots: 0,
                    invalid_slots: 0,
                    slots: Vec::new(),
                });
            host.total_slots += 1;
            match state {
                "valid" => host.valid_slots += 1,
                "pending" => host.pending_slots += 1,
                "locked" => host.locked_slots += 1,
                "invalid" => host.invalid_slots += 1,
                _ => {}
            }

            if pending_only && state == "valid" {
                continue;
            }

            let command = format!(
                "meshfit benchmark-run-one . {} {} --host {}",
                shell_quote(&candidate.name),
                run_number,
                shell_quote(&candidate.benchmark_host)
            );
            host.slots.push(BenchmarkWorkSlot {
                candidate: candidate.name.clone(),
                runtime: candidate.runtime.clone(),
                plan_id: candidate.plan_id.clone(),
                run_number,
                bundle_path: bundle_rel.clone(),
                state: state.to_string(),
                command,
                error,
            });
        }
    }

    if let Some(host) = host_filter {
        if !hosts.contains_key(host) {
            return Err(format!(
                "benchmark host '{host}' has no assigned run slots in this kit"
            ));
        }
    }

    let hosts = hosts.into_values().collect::<Vec<_>>();
    let total_slots = hosts.iter().map(|host| host.total_slots).sum();
    let valid_slots = hosts.iter().map(|host| host.valid_slots).sum();
    let pending_slots = hosts.iter().map(|host| host.pending_slots).sum();
    let locked_slots = hosts.iter().map(|host| host.locked_slots).sum();
    let invalid_slots = hosts.iter().map(|host| host.invalid_slots).sum();

    Ok(BenchmarkWorklist {
        benchmark_id: kit.benchmark_id,
        kit_ready: kit.ready,
        host_filter: host_filter.map(str::to_string),
        total_slots,
        valid_slots,
        pending_slots,
        locked_slots,
        invalid_slots,
        hosts,
    })
}

fn inspect_work_slot(
    bundle_path: &Path,
    expected_plan_id: &str,
    lock_path: &Path,
) -> (&'static str, Option<String>) {
    if bundle_path.is_file() {
        let raw = match fs::read_to_string(bundle_path) {
            Ok(raw) => raw,
            Err(error) => return ("invalid", Some(format!("read failed: {error}"))),
        };
        let bundle: BenchmarkBundle = match serde_yaml::from_str(&raw) {
            Ok(bundle) => bundle,
            Err(error) => return ("invalid", Some(format!("parse failed: {error}"))),
        };
        if let Err(error) = bundle.validate() {
            return ("invalid", Some(error));
        }
        if bundle.request.executable.source_plan_id != expected_plan_id {
            return (
                "invalid",
                Some(format!(
                    "source plan '{}' does not match candidate plan '{}'",
                    bundle.request.executable.source_plan_id, expected_plan_id
                )),
            );
        }
        return ("valid", None);
    }

    if lock_path.exists() {
        return (
            "locked",
            Some(
                "run slot lock exists; the run may be active or the lock may be stale".to_string(),
            ),
        );
    }

    ("pending", None)
}

fn inspect_benchmark_kit(kit_dir: &Path) -> Result<BenchmarkKitStatus, String> {
    let kit_path = kit_dir.join("kit.yaml");
    let raw =
        fs::read_to_string(&kit_path).map_err(|e| format!("read {}: {e}", kit_path.display()))?;
    let kit: BenchmarkExecutionKit =
        serde_yaml::from_str(&raw).map_err(|e| format!("parse {}: {e}", kit_path.display()))?;

    let mut expected_bundles = 0_usize;
    let mut valid_bundles = 0_usize;
    let mut issues = kit.warnings.clone();
    let mut candidates = Vec::new();

    if !kit.ready {
        issues.push("execution kit is not marked ready".to_string());
    }

    for candidate in &kit.candidates {
        if !candidate.compile_ready {
            issues.push(format!(
                "candidate '{}' is not compiler-ready{}",
                candidate.name,
                candidate
                    .compile_error
                    .as_deref()
                    .map(|error| format!(": {error}"))
                    .unwrap_or_default()
            ));
        }

        let comparison = kit
            .comparison_manifest
            .candidates
            .iter()
            .find(|item| item.name == candidate.name);

        let Some(comparison) = comparison else {
            issues.push(format!(
                "candidate '{}' is missing from comparison manifest",
                candidate.name
            ));
            candidates.push(BenchmarkCandidateStatus {
                name: candidate.name.clone(),
                benchmark_host: candidate.benchmark_host.clone(),
                plan_id: candidate.plan_id.clone(),
                expected_runs: 0,
                valid_runs: 0,
                missing_bundles: Vec::new(),
                invalid_bundles: Vec::new(),
            });
            continue;
        };

        expected_bundles += comparison.bundles.len();
        let mut valid_runs = 0_usize;
        let mut missing_bundles = Vec::new();
        let mut invalid_bundles = Vec::new();

        for bundle_rel in &comparison.bundles {
            let bundle_path = kit_dir.join(bundle_rel);
            if !bundle_path.is_file() {
                missing_bundles.push(bundle_rel.clone());
                continue;
            }

            let bundle_raw = match fs::read_to_string(&bundle_path) {
                Ok(raw) => raw,
                Err(error) => {
                    invalid_bundles.push(BenchmarkInvalidBundle {
                        path: bundle_rel.clone(),
                        error: format!("read failed: {error}"),
                    });
                    continue;
                }
            };

            let bundle: BenchmarkBundle = match serde_yaml::from_str(&bundle_raw) {
                Ok(bundle) => bundle,
                Err(error) => {
                    invalid_bundles.push(BenchmarkInvalidBundle {
                        path: bundle_rel.clone(),
                        error: format!("parse failed: {error}"),
                    });
                    continue;
                }
            };

            if let Err(error) = bundle.validate() {
                invalid_bundles.push(BenchmarkInvalidBundle {
                    path: bundle_rel.clone(),
                    error,
                });
                continue;
            }

            if bundle.request.executable.source_plan_id != candidate.plan_id {
                invalid_bundles.push(BenchmarkInvalidBundle {
                    path: bundle_rel.clone(),
                    error: format!(
                        "source plan '{}' does not match candidate plan '{}'",
                        bundle.request.executable.source_plan_id, candidate.plan_id
                    ),
                });
                continue;
            }

            valid_runs += 1;
            valid_bundles += 1;
        }

        candidates.push(BenchmarkCandidateStatus {
            name: candidate.name.clone(),
            benchmark_host: candidate.benchmark_host.clone(),
            plan_id: candidate.plan_id.clone(),
            expected_runs: comparison.bundles.len(),
            valid_runs,
            missing_bundles,
            invalid_bundles,
        });
    }

    let complete = kit.ready
        && issues.is_empty()
        && valid_bundles == expected_bundles
        && candidates.iter().all(|candidate| {
            candidate.missing_bundles.is_empty() && candidate.invalid_bundles.is_empty()
        });

    Ok(BenchmarkKitStatus {
        benchmark_id: kit.benchmark_id,
        kit_ready: kit.ready,
        complete,
        expected_bundles,
        valid_bundles,
        candidates,
        issues,
    })
}

fn materialize_benchmark_kit(
    kit: &BenchmarkExecutionKit,
    output_dir: &Path,
    snapshot_raw: &str,
    target_raw: &str,
    model_identity_raw: &str,
) -> Result<PathBuf, String> {
    let inputs_dir = output_dir.join("inputs");
    fs::create_dir_all(&inputs_dir).map_err(|e| format!("create {}: {e}", inputs_dir.display()))?;

    fs::write(inputs_dir.join("snapshot.yaml"), snapshot_raw)
        .map_err(|e| format!("write snapshot input: {e}"))?;
    fs::write(inputs_dir.join("target.yaml"), target_raw)
        .map_err(|e| format!("write target input: {e}"))?;
    fs::write(inputs_dir.join("model-identity.yaml"), model_identity_raw)
        .map_err(|e| format!("write model identity input: {e}"))?;

    let mut localized = kit.clone();
    localized.snapshot = "inputs/snapshot.yaml".to_string();
    localized.target = "inputs/target.yaml".to_string();
    localized.model_identity = "inputs/model-identity.yaml".to_string();
    localized.model_path = portable_model_path(&kit.model_path)?;

    for candidate in &mut localized.candidates {
        fs::create_dir_all(output_dir.join(&candidate.result_dir))
            .map_err(|e| format!("create result dir '{}': {e}", candidate.result_dir))?;

        let executable_parent = Path::new(&candidate.executable_path)
            .parent()
            .ok_or_else(|| format!("invalid executable path '{}'", candidate.executable_path))?;
        fs::create_dir_all(output_dir.join(executable_parent))
            .map_err(|e| format!("create artifact dir '{}': {e}", executable_parent.display()))?;

        if candidate.compile_ready {
            candidate.compile_command = Some(format!(
                "meshfit compile-snapshot {} {} {} {} --listen-port {} > {}",
                shell_quote(&localized.snapshot),
                shell_quote(&localized.target),
                shell_quote(&candidate.plan_id),
                shell_quote(&localized.model_path),
                localized.listen_port,
                shell_quote(&candidate.executable_path),
            ));
            candidate.run_commands = (1..=localized.runs_per_candidate)
                .map(|run| {
                    let bundle = format!("{}/run-{run:02}.yaml", candidate.result_dir);
                    format!(
                        "meshfit benchmark-auto {} {} --concurrency {} --measured-requests {} > {}",
                        shell_quote(&candidate.executable_path),
                        shell_quote(&localized.model_identity),
                        localized.concurrency,
                        localized.measured_requests_per_run,
                        shell_quote(&bundle),
                    )
                })
                .collect();
        }
    }

    let kit_yaml = serde_yaml::to_string(&localized).map_err(|e| e.to_string())?;
    fs::write(output_dir.join("kit.yaml"), kit_yaml).map_err(|e| format!("write kit.yaml: {e}"))?;

    let comparison_yaml =
        serde_yaml::to_string(&localized.comparison_manifest).map_err(|e| e.to_string())?;
    fs::write(output_dir.join("comparison.yaml"), comparison_yaml)
        .map_err(|e| format!("write comparison.yaml: {e}"))?;

    let runbook = render_benchmark_runbook(&localized);
    fs::write(output_dir.join("RUNBOOK.md"), runbook)
        .map_err(|e| format!("write RUNBOOK.md: {e}"))?;

    Ok(output_dir.to_path_buf())
}

fn portable_model_path(model_path: &str) -> Result<String, String> {
    let path = Path::new(model_path);
    if path.exists() {
        path.canonicalize()
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|e| format!("canonicalize model path '{model_path}': {e}"))
    } else {
        Ok(model_path.to_string())
    }
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "/._:-".contains(ch))
    {
        return value.to_string();
    }

    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn render_benchmark_runbook(kit: &BenchmarkExecutionKit) -> String {
    let mut out = String::new();
    out.push_str("# MeshFit Benchmark 001 Runbook\n\n");
    out.push_str(&format!(
        "**Ready:** {}  \n**Concurrency:** {}  \n**Measured requests/run:** {}  \n**Runs/candidate:** {}  \n**Listen port:** {}\n\n",
        kit.ready,
        kit.concurrency,
        kit.measured_requests_per_run,
        kit.runs_per_candidate,
        kit.listen_port
    ));
    out.push_str(
        "Run commands from this directory. Execute each candidate on its listed benchmark_host.\n\n",
    );

    if !kit.warnings.is_empty() {
        out.push_str("## Warnings\n\n");
        for warning in &kit.warnings {
            out.push_str(&format!("- {warning}\n"));
        }
        out.push('\n');
    }

    for candidate in &kit.candidates {
        out.push_str(&format!(
            "## {}\n\n- Plan: {}\n- Benchmark host: {}\n- Runtime: {}\n- Compile ready: {}\n\n",
            candidate.name,
            candidate.plan_id,
            candidate.benchmark_host,
            candidate.runtime,
            candidate.compile_ready
        ));
        out.push_str(&format!(
            "    meshfit benchmark-preflight . {} --host {} --require-ready\n\n",
            shell_quote(&candidate.name),
            shell_quote(&candidate.benchmark_host)
        ));
        if let Some(error) = &candidate.compile_error {
            out.push_str(&format!("Compiler preflight error: {error}\n\n"));
            continue;
        }
        for run in 1..=kit.runs_per_candidate {
            out.push_str(&format!(
                "    meshfit benchmark-run-one . {} {} --host {}\n\n",
                shell_quote(&candidate.name),
                run,
                shell_quote(&candidate.benchmark_host)
            ));
        }
    }

    out.push_str("## Verify completion\n\n");
    out.push_str("    meshfit benchmark-status . --require-complete\n\n");
    out.push_str("## Compare\n\n");
    out.push_str(
        "    meshfit compare-benchmarks comparison.yaml --markdown --require-publishable\n",
    );
    out
}

fn default_measured_requests(concurrency: u32) -> u32 {
    let minimum_samples_per_run = 10_u32;
    let waves = minimum_samples_per_run.div_ceil(concurrency);
    waves.max(1).saturating_mul(concurrency)
}
fn select_benchmark_plans(
    report: &PlacementReport,
    meshfit_plan_id: &str,
) -> Result<Vec<(&'static str, &'static str, PlanIR)>, String> {
    let meshfit = report
        .feasible
        .iter()
        .find(|plan| plan.id == meshfit_plan_id)
        .cloned()
        .ok_or_else(|| format!("meshfit plan-id '{meshfit_plan_id}' is not feasible"))?;

    let single_best = report
        .feasible
        .iter()
        .filter(|plan| plan.nodes.len() == 1)
        .max_by(|left, right| baseline_plan_order(left, right))
        .cloned()
        .ok_or_else(|| "no feasible single-node baseline plan".to_string())?;

    let max_compute = report
        .feasible
        .iter()
        .max_by(|left, right| baseline_plan_order(left, right))
        .cloned()
        .ok_or_else(|| "no feasible max-compute baseline plan".to_string())?;

    let selected = vec![
        (
            "single-best-node",
            "highest relative compute among feasible single-node plans",
            single_best,
        ),
        (
            "max-aggregate-compute",
            "highest relative compute among all structurally feasible plans",
            max_compute,
        ),
        (
            "meshfit",
            "explicit MeshFit-selected plan supplied by plan ID",
            meshfit,
        ),
    ];

    Ok(selected)
}

fn benchmark_plan_warnings(selected: &[(&str, &str, PlanIR)]) -> Vec<String> {
    let unique_ids = selected
        .iter()
        .map(|(_, _, plan)| plan.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    if unique_ids.len() == selected.len() {
        Vec::new()
    } else {
        vec![
            "Benchmark 001 candidate plans overlap. A publishable comparison requires three distinct plan IDs; use a cluster/model workload with discriminating alternatives."
                .to_string(),
        ]
    }
}

fn baseline_plan_order(left: &PlanIR, right: &PlanIR) -> std::cmp::Ordering {
    left.relative_compute
        .total_cmp(&right.relative_compute)
        .then_with(|| right.hourly_cost_usd.total_cmp(&left.hourly_cost_usd))
        .then_with(|| left.memory_headroom_gb.total_cmp(&right.memory_headroom_gb))
        .then_with(|| right.id.cmp(&left.id))
}

fn benchmark_plan_candidate(name: &str, strategy: &str, plan: PlanIR) -> BenchmarkPlanCandidate {
    BenchmarkPlanCandidate {
        name: name.to_string(),
        strategy: strategy.to_string(),
        plan_id: plan.id,
        placement: plan.placement,
        runtime: plan.runtime,
        nodes: plan.nodes,
        relative_compute: plan.relative_compute,
        hourly_cost_usd: plan.hourly_cost_usd,
    }
}

fn option_value<'a>(args: &'a [String], option: &str) -> Result<Option<&'a str>, String> {
    for (index, arg) in args.iter().enumerate() {
        if arg == option {
            return args
                .get(index + 1)
                .map(|value| Some(value.as_str()))
                .ok_or_else(|| format!("missing value for {option}"));
        }
    }

    Ok(None)
}

fn print_report(report: &PlacementReport) {
    println!("MeshFit v0.1 structural placement\n");
    println!("model       {}", report.model);
    println!("feasible    {}", report.feasible.len());
    println!("pareto      {}", report.pareto.len());
    println!("rejected    {}\n", report.rejected.len());

    println!("PARETO");
    println!("------");
    for (idx, plan) in report.pareto.iter().enumerate() {
        println!(
            "{:02}  id={}  {:?}  nodes={}  devices={}  runtime={}  compute={:.1}  cost=${:.2}/h  headroom={:.1}GB",
            idx + 1,
            plan.id,
            plan.placement,
            plan.nodes.join("+"),
            plan.accelerators
                .iter()
                .map(|a| format!("{}/{}", a.node, a.accelerator))
                .collect::<Vec<_>>()
                .join(","),
            plan.runtime,
            plan.relative_compute,
            plan.hourly_cost_usd,
            plan.memory_headroom_gb,
        );
    }

    if !report.excluded_nodes.is_empty() {
        println!("\nEXCLUDED");
        println!("--------");
        for node in &report.excluded_nodes {
            println!(
                "{}\n  {}\n  better role: {}",
                node.node, node.reason, node.suggested_role
            );
        }
    }

    if !report.rejected.is_empty() {
        println!("\nREJECTIONS");
        println!("----------");
        for rejection in report.rejected.iter().take(12) {
            println!(
                "{}  [{}]\n  {}",
                rejection.candidate, rejection.code, rejection.reason
            );
        }
        if report.rejected.len() > 12 {
            println!("... {} more", report.rejected.len() - 12);
        }
    }

    println!(
        "\nNote: v0.1 reports structural feasibility only. It does not claim real TTFT or tok/s."
    );
}

fn print_prediction(prediction: &Prediction) {
    println!("MeshFit v0.2 evidence-backed prediction\n");
    println!("metric       {:?}", prediction.metric);
    println!("status       {:?}", prediction.status);
    println!("fingerprint  {}", prediction.execution_fingerprint);
    println!("samples      {}", prediction.sample_count);
    println!("confidence   {:.2}", prediction.confidence);
    if let (Some(mean), Some(min), Some(max)) = (prediction.mean, prediction.min, prediction.max) {
        println!("mean         {:.3}", mean);
        println!("range        {:.3} .. {:.3}", min, max);
    }
    println!("explanation  {}", prediction.explanation);
    if !prediction.evidence.is_empty() {
        println!("\nEVIDENCE");
        println!("--------");
        for item in &prediction.evidence {
            match &item.source_url {
                Some(url) => println!(
                    "{}  {}  {}  {}",
                    item.benchmark_id, item.execution_fingerprint, item.source, url
                ),
                None => println!(
                    "{}  {}  {}",
                    item.benchmark_id, item.execution_fingerprint, item.source
                ),
            }
        }
    }
}

fn print_help() {
    println!(
        "MeshFit — placement intelligence for heterogeneous inference\n\nUsage:\n  meshfit discover\n  meshfit runtimes\n  meshfit inspect-model <path> <model-id> <format> <quantization> [revision]\n  meshfit probe <peer> [--bandwidth]\n  meshfit snapshot-manifest <manifest.yaml>\n  meshfit snapshot <local-discovery.yaml> <peer-discovery.yaml> [probe.yaml]\n  meshfit plan-snapshot <snapshot.yaml> <target.yaml>\n  meshfit compile <request.yaml>\n  meshfit compile-snapshot <snapshot.yaml> <target.yaml> <plan-id> <model-path> [gpu-layers]\n  meshfit benchmark-auto <executable.yaml> <model-identity.yaml> [--concurrency N] [--measured-requests N] [--prompt TEXT]\n  meshfit benchmark-local <request.yaml>\n  meshfit evidence-from-benchmark <bundle.yaml>\n  meshfit benchmark-candidates <snapshot.yaml> <target.yaml> <meshfit-plan-id> [--require-distinct]\n  meshfit benchmark-kit <snapshot.yaml> <target.yaml> <meshfit-plan-id> <model-path> <model-identity.yaml> [--listen-port N] [--require-ready] [--write-dir DIR]\n  meshfit benchmark-run-one <kit-dir> <candidate> <run-number> [--host NODE] [--model-path PATH] [--dry-run] [--overwrite]\n  meshfit benchmark-run-candidate <kit-dir> <candidate> [--host NODE] [--model-path PATH] [--dry-run] [--resume|--overwrite]\n  meshfit benchmark-run-host <kit-dir> (--host NODE | --current-host) [--model-path PATH] [--dry-run] [--resume|--overwrite]\n  meshfit benchmark-worklist <kit-dir> [--host NODE | --current-host] [--pending-only]\n  meshfit benchmark-status <kit-dir> [--require-complete]\n  meshfit benchmark-finalize <kit-dir> [--markdown] [--require-publishable]\n  meshfit benchmark-preflight <kit-dir> <candidate> [--host NODE] [--model-path PATH] [--allow-existing] [--require-ready]\n  meshfit compare-benchmarks <comparison.yaml> [--markdown] [--require-publishable]\n  meshfit plan <scenario.yaml>\n  meshfit predict <evidence.yaml> <query.yaml>\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_auto_defaults_to_at_least_ten_samples() {
        assert_eq!(default_measured_requests(1), 10);
        assert_eq!(default_measured_requests(2), 10);
        assert_eq!(default_measured_requests(4), 12);
        assert_eq!(default_measured_requests(20), 20);
    }

    #[test]
    fn benchmark_auto_default_is_divisible_by_concurrency() {
        for concurrency in 1..=32 {
            let measured = default_measured_requests(concurrency);
            assert!(measured >= 10 || measured == concurrency);
            assert_eq!(measured % concurrency, 0);
        }
    }

    fn status_test_kit() -> BenchmarkExecutionKit {
        BenchmarkExecutionKit {
            benchmark_id: "benchmark-001-test".into(),
            ready: true,
            snapshot: "inputs/snapshot.yaml".into(),
            target: "inputs/target.yaml".into(),
            model_path: "demo".into(),
            model_identity: "inputs/model-identity.yaml".into(),
            concurrency: 1,
            measured_requests_per_run: 10,
            runs_per_candidate: 1,
            listen_port: BENCHMARK_LISTEN_PORT,
            candidates: vec![BenchmarkExecutionCandidate {
                name: "meshfit".into(),
                plan_id: "plan-test".into(),
                nodes: vec!["node-a".into()],
                benchmark_host: "node-a".into(),
                runtime: "vllm".into(),
                result_dir: "results/meshfit".into(),
                executable_path: "artifacts/meshfit/executable.yaml".into(),
                compile_ready: true,
                compile_error: None,
                compile_command: None,
                run_commands: vec![],
            }],
            comparison_manifest: BenchmarkExecutionComparison {
                benchmark_id: "benchmark-001-test".into(),
                meshfit_candidate: "meshfit".into(),
                objective: "p95_ttft_ms".into(),
                candidates: vec![BenchmarkExecutionComparisonCandidate {
                    name: "meshfit".into(),
                    strategy: "test".into(),
                    hourly_cost_usd: 0.0,
                    bundles: vec!["results/meshfit/run-01.yaml".into()],
                }],
            },
            warnings: vec![],
        }
    }

    fn status_test_dir(label: &str) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("meshfit-{label}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn preflight_hard_gates_host_runtime_and_existing_evidence() {
        let candidate = &status_test_kit().candidates[0];
        let issues = benchmark_preflight_issues(true, candidate, "node-b", false, None, 1, false);

        assert!(issues
            .iter()
            .any(|issue| issue.contains("wrong benchmark host")));
        assert!(issues
            .iter()
            .any(|issue| issue.contains("required runtime 'vllm'")));
        assert!(issues
            .iter()
            .any(|issue| issue.contains("refusing accidental evidence overwrite")));
    }

    #[test]
    fn preflight_can_resume_existing_evidence_only_when_explicitly_allowed() {
        let candidate = &status_test_kit().candidates[0];
        let issues = benchmark_preflight_issues(true, candidate, "node-a", true, None, 1, true);

        assert!(issues.is_empty());
    }

    #[test]
    fn evidence_commit_refuses_implicit_overwrite() {
        let dir = status_test_dir("evidence-no-overwrite");
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("run-01.yaml");
        let temp = dir.join("run-01.yaml.tmp");
        fs::write(&target, "old").unwrap();
        fs::write(&temp, "new").unwrap();

        let error = commit_benchmark_bundle(&temp, &target, false).unwrap_err();
        assert!(error.contains("refusing replacement"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn evidence_commit_replaces_only_with_explicit_overwrite() {
        let dir = status_test_dir("evidence-overwrite");
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("run-01.yaml");
        let temp = dir.join("run-01.yaml.tmp");
        fs::write(&target, "old").unwrap();
        fs::write(&temp, "new").unwrap();

        commit_benchmark_bundle(&temp, &target, true).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert!(!dir.join("run-01.yaml.meshfit-backup").exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn resume_rejects_corrupt_existing_bundle() {
        let dir = status_test_dir("resume-corrupt");
        fs::create_dir_all(&dir).unwrap();
        let bundle = dir.join("run-01.yaml");
        fs::write(&bundle, "not-valid-yaml: [").unwrap();

        let error = validate_existing_candidate_bundle(&bundle, "plan-test").unwrap_err();
        assert!(error.contains("parse existing bundle"));

        let _ = fs::remove_dir_all(dir);
    }

    fn write_model_override_fixture(
        dir: &Path,
        model_bytes: &[u8],
        override_bytes: &[u8],
    ) -> (BenchmarkExecutionKit, PathBuf) {
        let inputs = dir.join("inputs");
        fs::create_dir_all(&inputs).unwrap();
        let source = dir.join("source-model.bin");
        let override_path = dir.join("host-model.bin");
        fs::write(&source, model_bytes).unwrap();
        fs::write(&override_path, override_bytes).unwrap();

        let identity = inspect_model_artifact(
            &source,
            "benchmark-model",
            "bin",
            "test",
            Some("fixture".into()),
        )
        .unwrap();
        fs::write(
            inputs.join("model-identity.yaml"),
            serde_yaml::to_string(&identity).unwrap(),
        )
        .unwrap();

        let mut kit = status_test_kit();
        kit.model_identity = "inputs/model-identity.yaml".into();
        (kit, override_path)
    }

    #[test]
    fn model_path_override_is_verified_by_artifact_hash() {
        let dir = status_test_dir("model-override-match");
        fs::create_dir_all(&dir).unwrap();
        let (kit, override_path) = write_model_override_fixture(&dir, b"same-model", b"same-model");

        let check =
            inspect_model_path_for_host(&dir, &kit, Some(override_path.to_str().unwrap())).unwrap();

        assert_eq!(check.status, "local_override_verified");
        assert!(Path::new(&check.path).is_absolute());
        assert!(check.overridden);
        assert!(check.verified);
        assert!(check.issue.is_none());
        assert!(check.warnings.is_empty());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn model_path_override_rejects_hash_mismatch() {
        let dir = status_test_dir("model-override-mismatch");
        fs::create_dir_all(&dir).unwrap();
        let (kit, override_path) =
            write_model_override_fixture(&dir, b"expected-model", b"different-model");

        let check =
            inspect_model_path_for_host(&dir, &kit, Some(override_path.to_str().unwrap())).unwrap();

        assert_eq!(check.status, "local_override_hash_mismatch");
        assert!(check.overridden);
        assert!(!check.verified);
        assert!(check.issue.unwrap().contains("SHA-256"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn executable_replacement_refuses_stale_backup() {
        let dir = status_test_dir("executable-backup-guard");
        fs::create_dir_all(&dir).unwrap();
        let executable = dir.join("executable.yaml");
        let temp = dir.join("executable.yaml.tmp");
        let backup = PathBuf::from(format!("{}.meshfit-backup", executable.display()));
        fs::write(&executable, "old").unwrap();
        fs::write(&temp, "new").unwrap();
        fs::write(&backup, "stale-backup").unwrap();

        let error = commit_executable_artifact(&temp, &executable).unwrap_err();
        assert!(error.contains("backup"));
        assert_eq!(fs::read_to_string(&executable).unwrap(), "old");
        assert_eq!(fs::read_to_string(&temp).unwrap(), "new");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn benchmark_preflight_rejects_unknown_candidate() {
        let dir = status_test_dir("preflight-missing-candidate");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("kit.yaml"),
            serde_yaml::to_string(&status_test_kit()).unwrap(),
        )
        .unwrap();

        let error =
            inspect_benchmark_preflight(&dir, "not-a-candidate", Some("node-a"), None, false)
                .unwrap_err();
        assert!(error.contains("not present in kit.yaml"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn benchmark_listen_port_defaults_and_rejects_zero() {
        assert_eq!(parse_benchmark_listen_port(&[]).unwrap(), 18080);

        let args = vec!["--listen-port".to_string(), "19002".to_string()];
        assert_eq!(parse_benchmark_listen_port(&args).unwrap(), 19002);

        let zero = vec!["--listen-port".to_string(), "0".to_string()];
        assert!(parse_benchmark_listen_port(&zero)
            .unwrap_err()
            .contains("between 1 and 65535"));
    }

    #[test]
    fn legacy_benchmark_kit_without_listen_port_uses_default() {
        let yaml = serde_yaml::to_string(&status_test_kit()).unwrap();
        let legacy = yaml
            .lines()
            .filter(|line| !line.trim_start().starts_with("listen_port:"))
            .collect::<Vec<_>>()
            .join("\n");
        let parsed: BenchmarkExecutionKit = serde_yaml::from_str(&legacy).unwrap();
        assert_eq!(parsed.listen_port, BENCHMARK_LISTEN_PORT);
    }

    #[test]
    fn benchmark_listen_port_availability_tracks_bound_socket() {
        let listener = std::net::TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(!benchmark_listen_port_available(port));

        drop(listener);
        assert!(benchmark_listen_port_available(port));
    }

    #[test]
    fn benchmark_execution_lock_is_host_local_and_port_scoped() {
        let target = benchmark_execution_lock_target(19001);
        assert!(target.starts_with(env::temp_dir()));
        assert_eq!(
            target.file_name().and_then(|name| name.to_str()),
            Some("meshfit-benchmark-port-19001")
        );
    }

    #[test]
    fn benchmark_run_slot_lock_is_exclusive_and_released() {
        let dir = status_test_dir("run-lock");
        fs::create_dir_all(&dir).unwrap();
        let bundle_path = dir.join("run-01.yaml");

        let first = acquire_benchmark_run_slot(&bundle_path).unwrap();
        let second = acquire_benchmark_run_slot(&bundle_path).unwrap_err();
        assert!(second.contains("already locked"));

        drop(first);
        let third = acquire_benchmark_run_slot(&bundle_path).unwrap();
        drop(third);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn benchmark_run_host_rejects_unassigned_host() {
        let dir = status_test_dir("run-host-unassigned");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("kit.yaml"),
            serde_yaml::to_string(&status_test_kit()).unwrap(),
        )
        .unwrap();

        let error = plan_benchmark_host_runs(&dir, "node-z", None, false, false).unwrap_err();
        assert!(error.contains("has no assigned candidates"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn benchmark_run_host_collects_preflight_failures_before_execution() {
        let dir = status_test_dir("run-host-preflight");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("kit.yaml"),
            serde_yaml::to_string(&status_test_kit()).unwrap(),
        )
        .unwrap();

        let plan = plan_benchmark_host_runs(&dir, "node-a", None, false, false).unwrap();
        assert!(!plan.ready);
        assert_eq!(plan.candidates.len(), 1);
        assert!(!plan.issues.is_empty());
        assert_eq!(plan.candidates[0].pending_runs, vec![1]);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn benchmark_worklist_rejects_unassigned_host() {
        let dir = status_test_dir("worklist-host");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("kit.yaml"),
            serde_yaml::to_string(&status_test_kit()).unwrap(),
        )
        .unwrap();

        let error = inspect_benchmark_worklist(&dir, Some("node-z"), true).unwrap_err();
        assert!(error.contains("has no assigned run slots"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn benchmark_work_slot_distinguishes_pending_locked_and_invalid() {
        let dir = status_test_dir("work-slot");
        fs::create_dir_all(&dir).unwrap();
        let bundle_path = dir.join("run-01.yaml");
        let lock_path = PathBuf::from(format!("{}.lock", bundle_path.display()));

        assert_eq!(
            inspect_work_slot(&bundle_path, "plan-test", &lock_path),
            ("pending", None)
        );

        fs::write(&lock_path, "").unwrap();
        let locked = inspect_work_slot(&bundle_path, "plan-test", &lock_path);
        assert_eq!(locked.0, "locked");
        assert!(locked.1.unwrap().contains("may be stale"));

        fs::remove_file(&lock_path).unwrap();
        fs::write(&bundle_path, "not-valid-yaml: [").unwrap();
        let invalid = inspect_work_slot(&bundle_path, "plan-test", &lock_path);
        assert_eq!(invalid.0, "invalid");
        assert!(invalid.1.unwrap().contains("parse failed"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn benchmark_finalize_refuses_incomplete_kit() {
        let dir = status_test_dir("finalize-incomplete");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("kit.yaml"),
            serde_yaml::to_string(&status_test_kit()).unwrap(),
        )
        .unwrap();

        let error = finalize_benchmark_kit(&dir).unwrap_err();
        assert!(error.contains("Benchmark 001 is incomplete"));
        assert!(error.contains("0/1 valid bundles"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn benchmark_status_reports_missing_bundle() {
        let dir = status_test_dir("missing");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("kit.yaml"),
            serde_yaml::to_string(&status_test_kit()).unwrap(),
        )
        .unwrap();

        let status = inspect_benchmark_kit(&dir).unwrap();
        assert!(!status.complete);
        assert_eq!(status.expected_bundles, 1);
        assert_eq!(status.valid_bundles, 0);
        assert_eq!(
            status.candidates[0].missing_bundles,
            vec!["results/meshfit/run-01.yaml"]
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn benchmark_status_reports_corrupt_bundle() {
        let dir = status_test_dir("corrupt");
        fs::create_dir_all(dir.join("results/meshfit")).unwrap();
        fs::write(
            dir.join("kit.yaml"),
            serde_yaml::to_string(&status_test_kit()).unwrap(),
        )
        .unwrap();
        fs::write(dir.join("results/meshfit/run-01.yaml"), "not-valid-yaml: [").unwrap();

        let status = inspect_benchmark_kit(&dir).unwrap();
        assert!(!status.complete);
        assert_eq!(status.valid_bundles, 0);
        assert_eq!(status.candidates[0].invalid_bundles.len(), 1);
        assert!(status.candidates[0].invalid_bundles[0]
            .error
            .contains("parse failed"));

        let _ = fs::remove_dir_all(dir);
    }
}
