use std::{env, fs, process};

use meshfit_core::{solve, ScenarioIR};

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
        "plan" => {
            let path = args
                .get(2)
                .ok_or_else(|| "usage: meshfit plan <scenario.yaml>".to_string())?;
            let raw = fs::read_to_string(path)
                .map_err(|e| format!("read {path}: {e}"))?;
            let scenario: ScenarioIR =
                serde_yaml::from_str(&raw).map_err(|e| format!("parse {path}: {e}"))?;

            let report = solve(&scenario);
            print_report(&report);
        }
        other => return Err(format!("unknown command '{other}'")),
    }

    Ok(())
}

fn print_report(report: &meshfit_core::PlacementReport) {
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

fn print_help() {
    println!(
        "MeshFit — placement intelligence for heterogeneous inference\n\nUsage:\n  meshfit plan <scenario.yaml>\n"
    );
}