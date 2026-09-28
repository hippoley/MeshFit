use std::{env, fs, process};

use meshfit_core::{
    discover_local, discover_runtimes, inspect_model_artifact, probe_peer, solve, EvidenceStore,
    InfrastructureSnapshot, LinkKind, LocalDiscovery, PeerProbeResult, PlacementReport, Prediction,
    PredictionQuery, ScenarioIR,
};

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
            let model_id = args
                .get(3)
                .ok_or_else(|| "missing model-id".to_string())?;
            let format = args
                .get(4)
                .ok_or_else(|| "missing format".to_string())?;
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
        "snapshot" => {
            let local_path = args
                .get(2)
                .ok_or_else(|| "usage: meshfit snapshot <local-discovery.yaml> <peer-discovery.yaml> [probe.yaml]".to_string())?;
            let peer_path = args
                .get(3)
                .ok_or_else(|| "usage: meshfit snapshot <local-discovery.yaml> <peer-discovery.yaml> [probe.yaml]".to_string())?;

            let local_raw = fs::read_to_string(local_path)
                .map_err(|e| format!("read {local_path}: {e}"))?;
            let peer_raw = fs::read_to_string(peer_path)
                .map_err(|e| format!("read {peer_path}: {e}"))?;
            let local: LocalDiscovery = serde_yaml::from_str(&local_raw)
                .map_err(|e| format!("parse {local_path}: {e}"))?;
            let peer: LocalDiscovery = serde_yaml::from_str(&peer_raw)
                .map_err(|e| format!("parse {peer_path}: {e}"))?;

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
            let evidence_path = args.get(2).ok_or_else(|| {
                "usage: meshfit predict <evidence.yaml> <query.yaml>".to_string()
            })?;
            let query_path = args.get(3).ok_or_else(|| {
                "usage: meshfit predict <evidence.yaml> <query.yaml>".to_string()
            })?;
            let evidence_raw = fs::read_to_string(evidence_path)
                .map_err(|e| format!("read {evidence_path}: {e}"))?;
            let query_raw =
                fs::read_to_string(query_path).map_err(|e| format!("read {query_path}: {e}"))?;
            let evidence: EvidenceStore = serde_yaml::from_str(&evidence_raw)
                .map_err(|e| format!("parse {evidence_path}: {e}"))?;
            let query: PredictionQuery = serde_yaml::from_str(&query_raw)
                .map_err(|e| format!("parse {query_path}: {e}"))?;
            let prediction = evidence.predict_exact(&query);
            print_prediction(&prediction);
        }
        other => return Err(format!("unknown command '{other}'")),
    }

    Ok(())
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
            "{:02}  {:?}  nodes={}  runtime={}  compute={:.1}  cost=${:.2}/h  headroom={:.1}GB",
            idx + 1,
            plan.placement,
            plan.nodes.join("+"),
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
        "MeshFit — placement intelligence for heterogeneous inference\n\nUsage:\n  meshfit discover\n  meshfit runtimes\n  meshfit inspect-model <path> <model-id> <format> <quantization> [revision]\n  meshfit probe <peer> [--bandwidth]\n  meshfit snapshot <local-discovery.yaml> <peer-discovery.yaml> [probe.yaml]\n  meshfit plan <scenario.yaml>\n  meshfit predict <evidence.yaml> <query.yaml>\n"
    );
}