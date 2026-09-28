use std::{env, fs, process::Command};

use serde::{Deserialize, Serialize};

use crate::{
    identity::{DeviceIdentity, HardwareIdentity},
    ir::{AcceleratorBackend, AcceleratorIR, FabricEdgeIR, HardwareNodeIR, LinkKind},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveredAccelerator {
    pub identity: DeviceIdentity,
    pub free_memory_mib: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalDiscovery {
    pub hardware_identity: HardwareIdentity,
    pub node: HardwareNodeIR,
    #[serde(default)]
    pub local_fabric: Vec<FabricEdgeIR>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

pub fn discover_local() -> LocalDiscovery {
    let architecture = env::consts::ARCH.to_string();
    let operating_system = env::consts::OS.to_string();
    let hostname = env::var("HOSTNAME")
        .or_else(|_| env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "localhost".into());

    let ram_mib = discover_ram_mib();
    let cpu_model = discover_cpu_model();
    let mut warnings = Vec::new();

    let discovered = match query_nvidia_smi() {
        Ok(output) => parse_nvidia_smi_csv(&output),
        Err(reason) => {
            warnings.push(reason);
            Vec::new()
        }
    };

    let local_fabric = match query_nvidia_topology() {
        Ok(output) => parse_nvidia_topo_matrix(&output),
        Err(reason) => {
            warnings.push(reason);
            Vec::new()
        }
    };

    let devices = discovered
        .iter()
        .map(|gpu| gpu.identity.clone())
        .collect();

    let accelerators = discovered
        .iter()
        .enumerate()
        .map(|(idx, gpu)| AcceleratorIR {
            id: format!("gpu{idx}"),
            backend: gpu.identity.backend,
            memory_gb: gpu.identity.memory_mib as f64 / 1024.0,
            free_memory_gb: gpu.free_memory_mib.map(|mib| mib as f64 / 1024.0),
            relative_compute: 0.0,
        })
        .collect();

    LocalDiscovery {
        hardware_identity: HardwareIdentity {
            architecture,
            operating_system,
            cpu_model,
            ram_mib,
            devices,
        },
        node: HardwareNodeIR {
            id: hostname,
            site: "local".into(),
            ram_gb: ram_mib.unwrap_or(0) as f64 / 1024.0,
            accelerators,
            hourly_cost_usd: 0.0,
        },
        local_fabric,
        warnings,
    }
}

fn query_nvidia_smi() -> Result<String, String> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,memory.total,memory.free,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .map_err(|e| format!("nvidia-smi unavailable: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "nvidia-smi failed with status {}",
            output.status
        ));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("nvidia-smi output is not UTF-8: {e}"))
}

fn query_nvidia_topology() -> Result<String, String> {
    let output = Command::new("nvidia-smi")
        .args(["topo", "-m"])
        .output()
        .map_err(|e| format!("nvidia topology unavailable: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "nvidia topology query failed with status {}",
            output.status
        ));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| format!("nvidia topology output is not UTF-8: {e}"))
}

pub fn parse_nvidia_smi_csv(raw: &str) -> Vec<DiscoveredAccelerator> {
    raw.lines()
        .filter_map(|line| {
            let parts: Vec<_> = line.split(',').map(str::trim).collect();
            if parts.len() < 5 {
                return None;
            }

            let memory_mib = parts[2].parse::<u64>().ok()?;
            let free_memory_mib = parts[3].parse::<u64>().ok();

            Some(DiscoveredAccelerator {
                identity: DeviceIdentity {
                    vendor: "nvidia".into(),
                    model: parts[1].to_string(),
                    backend: AcceleratorBackend::Cuda,
                    memory_mib,
                    driver_version: Some(parts[4].to_string()),
                },
                free_memory_mib,
            })
        })
        .collect()
}

pub fn parse_nvidia_topo_matrix(raw: &str) -> Vec<FabricEdgeIR> {
    let mut gpu_columns = Vec::new();
    let mut rows = Vec::new();

    for line in raw.lines() {
        let tokens: Vec<_> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        if gpu_columns.is_empty() && tokens.iter().any(|token| token.starts_with("GPU")) {
            gpu_columns = tokens
                .iter()
                .filter(|token| token.starts_with("GPU"))
                .map(|token| token.to_string())
                .collect();
            continue;
        }

        if tokens[0].starts_with("GPU") {
            rows.push(tokens);
        }
    }

    let mut edges = Vec::new();

    for row in rows {
        let Some(row_index) = parse_gpu_index(row[0]) else {
            continue;
        };

        for (column_index, relation) in row.iter().skip(1).take(gpu_columns.len()).enumerate() {
            if column_index <= row_index || *relation == "X" {
                continue;
            }

            let Some(kind) = topology_relation_kind(relation) else {
                continue;
            };

            edges.push(FabricEdgeIR {
                from: format!("gpu{row_index}"),
                to: format!("gpu{column_index}"),
                kind,
                bandwidth_gbps: None,
                latency_ms: None,
                jitter_ms: 0.0,
                egress_cost_usd_per_gb: 0.0,
            });
        }
    }

    edges
}

fn parse_gpu_index(token: &str) -> Option<usize> {
    token.strip_prefix("GPU")?.parse().ok()
}

fn topology_relation_kind(relation: &str) -> Option<LinkKind> {
    if relation.starts_with("NV") {
        return Some(LinkKind::Nvlink);
    }

    match relation {
        "PIX" | "PXB" | "PHB" | "NODE" | "SYS" => Some(LinkKind::Pcie),
        _ => None,
    }
}

fn discover_ram_mib() -> Option<u64> {
    if env::consts::OS == "linux" {
        let raw = fs::read_to_string("/proc/meminfo").ok()?;
        return parse_linux_meminfo_mib(&raw);
    }

    None
}

pub fn parse_linux_meminfo_mib(raw: &str) -> Option<u64> {
    let line = raw.lines().find(|line| line.starts_with("MemTotal:"))?;
    let kib = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    Some(kib / 1024)
}

fn discover_cpu_model() -> Option<String> {
    if env::consts::OS == "linux" {
        let raw = fs::read_to_string("/proc/cpuinfo").ok()?;
        for line in raw.lines() {
            if let Some((key, value)) = line.split_once(':') {
                if key.trim() == "model name" {
                    return Some(value.trim().to_string());
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nvidia_csv_without_polluting_identity_with_free_memory() {
        let raw = "0, NVIDIA H100 80GB HBM3, 81559, 80123, 580.65.06\n1, NVIDIA H100 80GB HBM3, 81559, 79999, 580.65.06\n";
        let devices = parse_nvidia_smi_csv(raw);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].identity.model, "NVIDIA H100 80GB HBM3");
        assert_eq!(devices[0].identity.memory_mib, 81559);
        assert_eq!(devices[0].free_memory_mib, Some(80123));
        assert_eq!(devices[0].identity.backend, AcceleratorBackend::Cuda);
    }

    #[test]
    fn parses_nvlink_and_pcie_topology_without_fake_metrics() {
        let raw = "\tGPU0\tGPU1\tGPU2\tCPU Affinity\nGPU0\tX\tNV4\tPHB\t0-31\nGPU1\tNV4\tX\tPXB\t0-31\nGPU2\tPHB\tPXB\tX\t32-63\n";

        let edges = parse_nvidia_topo_matrix(raw);
        assert_eq!(edges.len(), 3);

        let nv = edges
            .iter()
            .find(|edge| edge.from == "gpu0" && edge.to == "gpu1")
            .unwrap();
        assert_eq!(nv.kind, LinkKind::Nvlink);
        assert_eq!(nv.bandwidth_gbps, None);
        assert_eq!(nv.latency_ms, None);

        let pcie = edges
            .iter()
            .find(|edge| edge.from == "gpu0" && edge.to == "gpu2")
            .unwrap();
        assert_eq!(pcie.kind, LinkKind::Pcie);
    }

    #[test]
    fn parses_linux_meminfo() {
        let raw = "MemTotal:       131900000 kB\nMemFree:          100000 kB\n";
        assert_eq!(parse_linux_meminfo_mib(raw), Some(128808));
    }
}
