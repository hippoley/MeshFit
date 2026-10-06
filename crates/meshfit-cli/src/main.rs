use std::{env, fs, path::Path, process};

use serde::{Deserialize, Serialize};

use meshfit_core::{
    compare_benchmarks, compile_plan, discover_local, discover_runtimes, inspect_model_artifact,
    prepare_local_benchmark_request, probe_peer, run_local_benchmark, solve, BenchmarkBundle,
    BenchmarkCandidate, BenchmarkComparisonRequest, BenchmarkConfig, BenchmarkRequestIR,
    ComparisonObjective, CompileRequest, EvidenceStore, ExecutablePlanIR, InfrastructureSnapshot,
    LinkKind, LocalDiscovery, ModelArtifactIdentity, PeerProbeResult, PlacementKind, PlacementReport, PlanIR,
    PlacementTargetIR, Prediction, PredictionQuery, ScenarioIR, SnapshotManifest,
};

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
                "usage: meshfit benchmark-auto <executable.yaml> <model-identity.yaml> [--concurrency N] [--prompt TEXT]"
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

            let measured_requests = concurrency
                .checked_mul(3)
                .ok_or_else(|| "concurrency is too large".to_string())?;

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
                "usage: meshfit benchmark-candidates <snapshot.yaml> <target.yaml> <meshfit-plan-id>"
                    .to_string()
            })?;
            let target_path = args.get(3).ok_or_else(|| "missing target.yaml".to_string())?;
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
            let meshfit = report
                .feasible
                .iter()
                .find(|plan| &plan.id == meshfit_plan_id)
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

            let mut warnings = Vec::new();
            let unique_ids = std::collections::HashSet::from([
                single_best.id.as_str(),
                max_compute.id.as_str(),
                meshfit.id.as_str(),
            ]);
            if unique_ids.len() != 3 {
                warnings.push(
                    "Benchmark 001 candidate plans overlap. A publishable comparison requires three distinct plan IDs; use a cluster/model workload with discriminating alternatives."
                        .to_string(),
                );
            }

            let set = BenchmarkPlanSet {
                candidates: vec![
                    benchmark_plan_candidate(
                        "single-best-node",
                        "highest relative compute among feasible single-node plans",
                        single_best,
                    ),
                    benchmark_plan_candidate(
                        "max-aggregate-compute",
                        "highest relative compute among all structurally feasible plans",
                        max_compute,
                    ),
                    benchmark_plan_candidate(
                        "meshfit",
                        "explicit MeshFit-selected plan supplied by plan ID",
                        meshfit,
                    ),
                ],
                warnings,
            };

            let yaml = serde_yaml::to_string(&set).map_err(|e| e.to_string())?;
            print!("{yaml}");
        }
        "compare-benchmarks" => {
            let manifest_path = args.get(2).ok_or_else(|| {
                "usage: meshfit compare-benchmarks <comparison.yaml> [--markdown] [--require-publishable]"
                    .to_string()
            })?;
            let raw = fs::read_to_string(manifest_path)
                .map_err(|e| format!("read {manifest_path}: {e}"))?;
            let manifest: BenchmarkComparisonManifest =
                serde_yaml::from_str(&raw).map_err(|e| format!("parse {manifest_path}: {e}"))?;
            let base_dir = Path::new(manifest_path)
                .parent()
                .unwrap_or_else(|| Path::new("."));

            let mut candidates = Vec::new();
            for candidate in manifest.candidates {
                let mut bundles = Vec::new();
                for bundle_file in candidate.bundles {
                    let path = base_dir.join(&bundle_file);
                    let bundle_raw = fs::read_to_string(&path)
                        .map_err(|e| format!("read {}: {e}", path.display()))?;
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

            let report = compare_benchmarks(&BenchmarkComparisonRequest {
                benchmark_id: manifest.benchmark_id,
                meshfit_candidate: manifest.meshfit_candidate,
                objective: manifest.objective,
                candidates,
            })?;

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
                .ok_or_else(|| "usage: meshfit compile-snapshot <snapshot.yaml> <target.yaml> <plan-id> <model-path> [gpu-layers]".to_string())?;
            let target_path = args
                .get(3)
                .ok_or_else(|| "missing target.yaml".to_string())?;
            let plan_id = args.get(4).ok_or_else(|| "missing plan-id".to_string())?;
            let model_path = args
                .get(5)
                .ok_or_else(|| "missing model-path".to_string())?;
            let gpu_layers = args
                .get(6)
                .map(|value| {
                    value
                        .parse::<u32>()
                        .map_err(|e| format!("invalid gpu-layers '{value}': {e}"))
                })
                .transpose()?;

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
                listen_port: 18080,
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
        "MeshFit — placement intelligence for heterogeneous inference\n\nUsage:\n  meshfit discover\n  meshfit runtimes\n  meshfit inspect-model <path> <model-id> <format> <quantization> [revision]\n  meshfit probe <peer> [--bandwidth]\n  meshfit snapshot-manifest <manifest.yaml>\n  meshfit snapshot <local-discovery.yaml> <peer-discovery.yaml> [probe.yaml]\n  meshfit plan-snapshot <snapshot.yaml> <target.yaml>\n  meshfit compile <request.yaml>\n  meshfit compile-snapshot <snapshot.yaml> <target.yaml> <plan-id> <model-path> [gpu-layers]\n  meshfit benchmark-auto <executable.yaml> <model-identity.yaml> [--concurrency N] [--prompt TEXT]\n  meshfit benchmark-local <request.yaml>\n  meshfit evidence-from-benchmark <bundle.yaml>\n  meshfit benchmark-candidates <snapshot.yaml> <target.yaml> <meshfit-plan-id>\n  meshfit compare-benchmarks <comparison.yaml> [--markdown] [--require-publishable]\n  meshfit plan <scenario.yaml>\n  meshfit predict <evidence.yaml> <query.yaml>\n"
    );
}
