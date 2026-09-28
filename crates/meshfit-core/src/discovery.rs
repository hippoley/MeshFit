use std::{env, fs, process::Command};

use serde::{Deserialize, Serialize};

use crate::{
    identity::{DeviceIdentity, HardwareIdentity},
    ir::{AcceleratorBackend, AcceleratorIR, HardwareNodeIR},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalDiscovery {
    pub hardware_identity: HardwareIdentity,
    pub node: HardwareNodeIR,
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

    let devices = match query_nvidia_smi() {
        Ok(output) => parse_nvidia_smi_csv(&output),
        Err(reason) => {
            warnings.push(reason);
            Vec::new()
        }
    };

    let accelerators = devices
        .iter()
        .enumerate()
        .map(|(idx, device)| AcceleratorIR {
            id: format!("gpu{idx}"),
            backend: device.backend,
            memory_gb: device.memory_mib as f64 / 1024.0,
            free_memory_gb: Some(device.memory_mib as f64 / 1024.0),
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

pub fn parse_nvidia_smi_csv(raw: &str) -> Vec<DeviceIdentity> {
    raw.lines()
        .filter_map(|line| {
            let parts: Vec<_> = line.split(',').map(str::trim).collect();
            if parts.len() < 5 {
                return None;
            }

            let memory_mib = parts[2].parse::<u64>().ok()?;

            Some(DeviceIdentity {
                vendor: "nvidia".into(),
                model: parts[1].to_string(),
                backend: AcceleratorBackend::Cuda,
                memory_mib,
                driver_version: Some(parts[4].to_string()),
            })
        })
        .collect()
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
    let kib = line
        .split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()?;
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
    fn parses_nvidia_csv() {
        let raw = "0, NVIDIA H100 80GB HBM3, 81559, 80123, 580.65.06\n1, NVIDIA H100 80GB HBM3, 81559, 79999, 580.65.06\n";
        let devices = parse_nvidia_smi_csv(raw);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].model, "NVIDIA H100 80GB HBM3");
        assert_eq!(devices[0].memory_mib, 81559);
        assert_eq!(devices[0].backend, AcceleratorBackend::Cuda);
    }

    #[test]
    fn parses_linux_meminfo() {
        let raw = "MemTotal:       131900000 kB\nMemFree:          100000 kB\n";
        assert_eq!(parse_linux_meminfo_mib(raw), Some(128808));
    }
}
