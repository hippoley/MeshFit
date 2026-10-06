use std::{env, fs, io::ErrorKind, process::Command};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    identity::{DeviceIdentity, HardwareIdentity, LinkIdentity, TopologyIdentity},
    ir::{
        AcceleratorBackend, AcceleratorIR, FabricEdgeIR, FabricEndpointIR, HardwareNodeIR, LinkKind,
    },
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
    let hostname = env::var("HOSTNAME").ok();
    let computername = env::var("COMPUTERNAME").ok();
    let node_override = env::var("MESHFIT_NODE_ID").ok();
    let node_id = resolve_node_id(
        node_override.as_deref(),
        hostname.as_deref(),
        computername.as_deref(),
    );

    let ram_mib = discover_ram_mib();
    let cpu_model = discover_cpu_model();
    let mut warnings = Vec::new();

    let mut discovered = match query_nvidia_smi() {
        Ok(output) => parse_nvidia_smi_csv(&output),
        Err(reason) => {
            warnings.push(reason);
            Vec::new()
        }
    };

    match query_amd_smi_static() {
        Ok(Some(static_output)) => {
            let monitor_output = match query_amd_smi_monitor() {
                Ok(output) => output,
                Err(reason) => {
                    warnings.push(reason);
                    None
                }
            };
            match parse_amd_smi_json(&static_output, monitor_output.as_deref()) {
                Ok(mut devices) => discovered.append(&mut devices),
                Err(reason) => warnings.push(reason),
            }
        }
        Ok(None) => {}
        Err(reason) => warnings.push(reason),
    }

    match query_intel_xpu_discovery() {
        Ok(Some(output)) => match parse_intel_xpu_smi_json(&output) {
            Ok(mut devices) => discovered.append(&mut devices),
            Err(reason) => warnings.push(reason),
        },
        Ok(None) => {}
        Err(reason) => warnings.push(reason),
    }

    match discover_apple_silicon_accelerator(&operating_system, &architecture) {
        Ok(Some(device)) => discovered.push(device),
        Ok(None) => {}
        Err(reason) => warnings.push(reason),
    }

    let local_fabric = match query_nvidia_topology() {
        Ok(output) => parse_nvidia_topo_matrix(&output, &node_id),
        Err(reason) => {
            warnings.push(reason);
            Vec::new()
        }
    };

    let devices = discovered.iter().map(|gpu| gpu.identity.clone()).collect();

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
            id: node_id,
            site: "local".into(),
            ram_gb: ram_mib.unwrap_or(0) as f64 / 1024.0,
            accelerators,
            hourly_cost_usd: 0.0,
        },
        local_fabric,
        warnings,
    }
}

pub fn resolve_node_id(
    override_id: Option<&str>,
    hostname: Option<&str>,
    computername: Option<&str>,
) -> String {
    override_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| hostname.map(str::trim).filter(|value| !value.is_empty()))
        .or_else(|| {
            computername
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("localhost")
        .to_string()
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
        return Err(format!("nvidia-smi failed with status {}", output.status));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("nvidia-smi output is not UTF-8: {e}"))
}

fn query_optional_command(binary: &str, args: &[&str]) -> Result<Option<String>, String> {
    let output = match Command::new(binary).args(args).output() {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{binary} unavailable: {error}")),
    };

    if !output.status.success() {
        return Err(format!("{binary} failed with status {}", output.status));
    }

    String::from_utf8(output.stdout)
        .map(Some)
        .map_err(|error| format!("{binary} output is not UTF-8: {error}"))
}

fn query_amd_smi_static() -> Result<Option<String>, String> {
    query_optional_command(
        "amd-smi",
        &["static", "--asic", "--driver", "--vram", "--json"],
    )
}

fn query_amd_smi_monitor() -> Result<Option<String>, String> {
    query_optional_command("amd-smi", &["monitor", "--vram-usage", "--json"])
}

fn query_intel_xpu_discovery() -> Result<Option<String>, String> {
    query_optional_command("xpu-smi", &["discovery", "-j"])
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

fn json_records<'a>(value: &'a Value, list_key: &str) -> Vec<&'a Value> {
    if let Some(items) = value.as_array() {
        return items.iter().collect();
    }
    value
        .get(list_key)
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn json_gpu_index(value: &Value) -> Option<u64> {
    value
        .get("gpu")
        .or_else(|| value.get("device_id"))
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str()?.parse::<u64>().ok())
        })
}

fn find_json_key<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(found) = map.get(*key) {
                    return Some(found);
                }
            }
            map.values().find_map(|child| find_json_key(child, keys))
        }
        Value::Array(items) => items.iter().find_map(|child| find_json_key(child, keys)),
        _ => None,
    }
}

fn json_string(value: &Value, keys: &[&str]) -> Option<String> {
    let value = find_json_key(value, keys)?;
    match value {
        Value::String(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn json_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.trim().parse::<f64>().ok(),
        Value::Object(map) => map.get("value").and_then(json_number),
        _ => None,
    }
}

fn json_unit(value: &Value) -> Option<&str> {
    value.as_object()?.get("unit")?.as_str().map(str::trim)
}

fn value_to_mib(value: &Value, default_unit: &str) -> Option<u64> {
    let number = json_number(value)?;
    if !number.is_finite() || number < 0.0 {
        return None;
    }

    let unit = json_unit(value)
        .unwrap_or(default_unit)
        .to_ascii_lowercase();
    let bytes = match unit.as_str() {
        "b" | "byte" | "bytes" => number,
        "kb" => number * 1_000.0,
        "kib" => number * 1024.0,
        "mb" => number * 1_000_000.0,
        "mib" => number * 1024.0 * 1024.0,
        "gb" => number * 1_000_000_000.0,
        "gib" => number * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };

    Some((bytes / (1024.0 * 1024.0)).round() as u64)
}

fn json_memory_mib(value: &Value, keys: &[&str], default_unit: &str) -> Option<u64> {
    let value = find_json_key(value, keys)?;
    value_to_mib(value, default_unit)
}

pub fn parse_amd_smi_json(
    static_raw: &str,
    monitor_raw: Option<&str>,
) -> Result<Vec<DiscoveredAccelerator>, String> {
    let static_json: Value =
        serde_json::from_str(static_raw).map_err(|e| format!("parse amd-smi static JSON: {e}"))?;
    let monitor_json = monitor_raw
        .map(|raw| {
            serde_json::from_str::<Value>(raw)
                .map_err(|e| format!("parse amd-smi monitor JSON: {e}"))
        })
        .transpose()?;

    let monitor_records = monitor_json
        .as_ref()
        .map(|value| json_records(value, "gpu_data"))
        .unwrap_or_default();

    let mut devices = Vec::new();
    for (position, record) in json_records(&static_json, "gpu_data")
        .into_iter()
        .enumerate()
    {
        let gpu_index = json_gpu_index(record).unwrap_or(position as u64);
        let monitor = monitor_records
            .iter()
            .copied()
            .find(|candidate| json_gpu_index(candidate) == Some(gpu_index));

        let model = json_string(record, &["market_name", "device_name", "asic_name"])
            .unwrap_or_else(|| format!("AMD GPU {gpu_index}"));
        let driver_version = json_string(record, &["driver_version"]);
        let memory_mib = json_memory_mib(
            record,
            &["vram_size", "vram_total", "vram_total_mb"],
            "MB",
        )
        .or_else(|| {
            monitor.and_then(|value| json_memory_mib(value, &["vram_total", "vram_size"], "MB"))
        });

        let Some(memory_mib) = memory_mib.filter(|memory| *memory > 0) else {
            continue;
        };

        let used_memory_mib =
            monitor.and_then(|value| json_memory_mib(value, &["vram_used", "vram_used_mb"], "MB"));
        let free_memory_mib = used_memory_mib.map(|used| memory_mib.saturating_sub(used));

        devices.push(DiscoveredAccelerator {
            identity: DeviceIdentity {
                vendor: "amd".into(),
                model,
                backend: AcceleratorBackend::Rocm,
                memory_mib,
                driver_version,
            },
            free_memory_mib,
        });
    }

    Ok(devices)
}

pub fn parse_intel_xpu_smi_json(raw: &str) -> Result<Vec<DiscoveredAccelerator>, String> {
    let root: Value =
        serde_json::from_str(raw).map_err(|e| format!("parse xpu-smi discovery JSON: {e}"))?;
    let mut devices = Vec::new();

    for record in json_records(&root, "device_list") {
        if let Some(device_type) = record.get("device_type").and_then(Value::as_str) {
            if !device_type.eq_ignore_ascii_case("gpu") {
                continue;
            }
        }

        let Some(memory_mib) = record
            .get("memory_physical_size_byte")
            .and_then(|value| value_to_mib(value, "B"))
            .filter(|memory| *memory > 0)
        else {
            continue;
        };

        let free_memory_mib = record
            .get("memory_free_size_byte")
            .and_then(|value| value_to_mib(value, "B"));
        let device_id = json_gpu_index(record).unwrap_or(devices.len() as u64);
        let model = record
            .get("device_name")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Intel GPU {device_id}"));
        let driver_version = record
            .get("driver_version")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);

        devices.push(DiscoveredAccelerator {
            identity: DeviceIdentity {
                vendor: "intel".into(),
                model,
                backend: AcceleratorBackend::Xpu,
                memory_mib,
                driver_version,
            },
            free_memory_mib,
        });
    }

    Ok(devices)
}

pub fn topology_identity_from_discovery(discovery: &LocalDiscovery) -> TopologyIdentity {
    let mut links = discovery
        .local_fabric
        .iter()
        .map(|edge| LinkIdentity {
            from: fabric_endpoint_key(&edge.from),
            to: fabric_endpoint_key(&edge.to),
            kind: edge.kind,
            bandwidth_mbps: edge
                .bandwidth_gbps
                .map(|gbps| (gbps * 1000.0).round() as u64),
            latency_micros: edge.latency_ms.map(|ms| (ms * 1000.0).round() as u64),
        })
        .collect::<Vec<_>>();

    links.sort_by(|a, b| {
        (a.from.as_str(), a.to.as_str(), format!("{:?}", a.kind)).cmp(&(
            b.from.as_str(),
            b.to.as_str(),
            format!("{:?}", b.kind),
        ))
    });

    TopologyIdentity { links }
}

fn fabric_endpoint_key(endpoint: &FabricEndpointIR) -> String {
    match endpoint {
        FabricEndpointIR::Node { node } => format!("node:{node}"),
        FabricEndpointIR::Accelerator { node, accelerator } => {
            format!("accelerator:{node}/{accelerator}")
        }
    }
}

pub fn parse_nvidia_topo_matrix(raw: &str, node_id: &str) -> Vec<FabricEdgeIR> {
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
                from: FabricEndpointIR::Accelerator {
                    node: node_id.to_string(),
                    accelerator: format!("gpu{row_index}"),
                },
                to: FabricEndpointIR::Accelerator {
                    node: node_id.to_string(),
                    accelerator: format!("gpu{column_index}"),
                },
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

fn query_sysctl_value(key: &str) -> Result<String, String> {
    let output = Command::new("sysctl")
        .args(["-n", key])
        .output()
        .map_err(|error| format!("sysctl {key} unavailable: {error}"))?;

    if !output.status.success() {
        return Err(format!("sysctl {key} failed with status {}", output.status));
    }

    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_string())
        .map_err(|error| format!("sysctl {key} output is not UTF-8: {error}"))
}

fn query_vm_stat() -> Result<String, String> {
    let output = Command::new("vm_stat")
        .output()
        .map_err(|error| format!("vm_stat unavailable: {error}"))?;

    if !output.status.success() {
        return Err(format!("vm_stat failed with status {}", output.status));
    }

    String::from_utf8(output.stdout)
        .map_err(|error| format!("vm_stat output is not UTF-8: {error}"))
}

pub fn parse_byte_count_mib(raw: &str) -> Option<u64> {
    let bytes = raw.trim().parse::<u64>().ok()?;
    Some(bytes / (1024 * 1024))
}

pub fn parse_vm_stat_available_mib(raw: &str) -> Option<u64> {
    let page_size = raw
        .lines()
        .next()?
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;

    let mut pages = 0_u64;
    for label in ["Pages free", "Pages inactive", "Pages speculative"] {
        let line = raw
            .lines()
            .find(|line| line.trim_start().starts_with(label))?;
        let count = line
            .split(':')
            .nth(1)?
            .trim()
            .trim_end_matches('.')
            .parse::<u64>()
            .ok()?;
        pages = pages.saturating_add(count);
    }

    Some(pages.saturating_mul(page_size) / (1024 * 1024))
}

fn discover_apple_silicon_accelerator(
    operating_system: &str,
    architecture: &str,
) -> Result<Option<DiscoveredAccelerator>, String> {
    if operating_system != "macos" || architecture != "aarch64" {
        return Ok(None);
    }

    let memory_mib = parse_byte_count_mib(&query_sysctl_value("hw.memsize")?)
        .filter(|memory| *memory > 0)
        .ok_or_else(|| "Apple unified memory size is unavailable".to_string())?;

    let chip = query_sysctl_value("machdep.cpu.brand_string")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Apple Silicon".to_string());
    let vm_stat = query_vm_stat()?;
    let free_memory_mib = parse_vm_stat_available_mib(&vm_stat)
        .map(|available| available.min(memory_mib))
        .ok_or_else(|| {
            "Apple unified-memory availability could not be derived from vm_stat".to_string()
        })?;

    Ok(Some(DiscoveredAccelerator {
        identity: DeviceIdentity {
            vendor: "apple".into(),
            model: format!("{chip} GPU"),
            backend: AcceleratorBackend::Metal,
            memory_mib,
            driver_version: None,
        },
        free_memory_mib: Some(free_memory_mib),
    }))
}

fn discover_ram_mib() -> Option<u64> {
    if env::consts::OS == "linux" {
        let raw = fs::read_to_string("/proc/meminfo").ok()?;
        return parse_linux_meminfo_mib(&raw);
    }

    if env::consts::OS == "macos" {
        let raw = query_sysctl_value("hw.memsize").ok()?;
        return parse_byte_count_mib(&raw);
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

    if env::consts::OS == "macos" {
        return query_sysctl_value("machdep.cpu.brand_string").ok();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_prefers_explicit_meshfit_override() {
        assert_eq!(
            resolve_node_id(
                Some("  local-4090  "),
                Some("docker-abc"),
                Some("windows-host")
            ),
            "local-4090"
        );
    }

    #[test]
    fn node_id_falls_back_to_platform_hostname_then_localhost() {
        assert_eq!(
            resolve_node_id(None, Some("linux-host"), Some("windows-host")),
            "linux-host"
        );
        assert_eq!(
            resolve_node_id(None, None, Some("windows-host")),
            "windows-host"
        );
        assert_eq!(resolve_node_id(Some("   "), Some("host-a"), None), "host-a");
        assert_eq!(resolve_node_id(None, None, None), "localhost");
    }

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
    fn parses_amd_smi_json_with_decimal_memory_units() {
        let static_raw = r#"{
            "gpu_data": [{
                "gpu": 0,
                "asic": {"market_name": "AMD Instinct MI300X"},
                "driver": {"driver_version": "7.0.2"},
                "vram": {"vram_size": {"value": 196300, "unit": "MB"}}
            }]
        }"#;
        let monitor_raw = r#"{
            "gpu_data": [{
                "gpu": 0,
                "vram_usage": {
                    "vram_used": {"value": 300, "unit": "MB"},
                    "vram_total": {"value": 196300, "unit": "MB"}
                }
            }]
        }"#;

        let devices = parse_amd_smi_json(static_raw, Some(monitor_raw)).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].identity.vendor, "amd");
        assert_eq!(devices[0].identity.model, "AMD Instinct MI300X");
        assert_eq!(devices[0].identity.backend, AcceleratorBackend::Rocm);
        assert_eq!(devices[0].identity.driver_version.as_deref(), Some("7.0.2"));
        assert_eq!(devices[0].identity.memory_mib, 187206);
        assert_eq!(devices[0].free_memory_mib, Some(186920));
    }

    #[test]
    fn parses_intel_xpu_discovery_json_from_byte_memory_fields() {
        let raw = r#"{
            "device_list": [{
                "device_id": 0,
                "device_type": "GPU",
                "device_name": "Intel(R) Data Center GPU Max 1550",
                "driver_version": "1.2.3",
                "memory_physical_size_byte": 25769803776,
                "memory_free_size_byte": 21474836480
            }]
        }"#;

        let devices = parse_intel_xpu_smi_json(raw).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].identity.vendor, "intel");
        assert_eq!(
            devices[0].identity.model,
            "Intel(R) Data Center GPU Max 1550"
        );
        assert_eq!(devices[0].identity.backend, AcceleratorBackend::Xpu);
        assert_eq!(devices[0].identity.memory_mib, 24576);
        assert_eq!(devices[0].free_memory_mib, Some(20480));
    }

    #[test]
    fn vendor_json_parsers_skip_devices_without_usable_memory() {
        let amd = parse_amd_smi_json(
            r#"{"gpu_data":[{"gpu":0,"asic":{"market_name":"AMD GPU"}}]}"#,
            None,
        )
        .unwrap();
        let intel =
            parse_intel_xpu_smi_json(r#"{"device_list":[{"device_id":0,"device_type":"GPU"}]}"#)
                .unwrap();

        assert!(amd.is_empty());
        assert!(intel.is_empty());
    }

    #[test]
    fn parses_nvlink_and_pcie_topology_without_fake_metrics() {
        let raw = "\tGPU0\tGPU1\tGPU2\tCPU Affinity\nGPU0\tX\tNV4\tPHB\t0-31\nGPU1\tNV4\tX\tPXB\t0-31\nGPU2\tPHB\tPXB\tX\t32-63\n";

        let edges = parse_nvidia_topo_matrix(raw, "node-a");
        assert_eq!(edges.len(), 3);

        let nv = edges
            .iter()
            .find(|edge| {
                edge.from
                    == FabricEndpointIR::Accelerator {
                        node: "node-a".into(),
                        accelerator: "gpu0".into(),
                    }
                    && edge.to
                        == FabricEndpointIR::Accelerator {
                            node: "node-a".into(),
                            accelerator: "gpu1".into(),
                        }
            })
            .unwrap();
        assert_eq!(nv.kind, LinkKind::Nvlink);
        assert_eq!(nv.bandwidth_gbps, None);
        assert_eq!(nv.latency_ms, None);

        let pcie = edges
            .iter()
            .find(|edge| {
                edge.from
                    == FabricEndpointIR::Accelerator {
                        node: "node-a".into(),
                        accelerator: "gpu0".into(),
                    }
                    && edge.to
                        == FabricEndpointIR::Accelerator {
                            node: "node-a".into(),
                            accelerator: "gpu2".into(),
                        }
            })
            .unwrap();
        assert_eq!(pcie.kind, LinkKind::Pcie);
    }

    #[test]
    fn converts_discovered_fabric_to_topology_identity_without_inventing_metrics() {
        let discovery = LocalDiscovery {
            hardware_identity: HardwareIdentity {
                architecture: "x86_64".into(),
                operating_system: "linux".into(),
                cpu_model: None,
                ram_mib: None,
                devices: vec![],
            },
            node: HardwareNodeIR {
                id: "node-a".into(),
                site: "local".into(),
                ram_gb: 0.0,
                accelerators: vec![],
                hourly_cost_usd: 0.0,
            },
            local_fabric: vec![FabricEdgeIR {
                from: FabricEndpointIR::Accelerator {
                    node: "node-a".into(),
                    accelerator: "gpu0".into(),
                },
                to: FabricEndpointIR::Accelerator {
                    node: "node-a".into(),
                    accelerator: "gpu1".into(),
                },
                kind: LinkKind::Nvlink,
                bandwidth_gbps: None,
                latency_ms: None,
                jitter_ms: 0.0,
                egress_cost_usd_per_gb: 0.0,
            }],
            warnings: vec![],
        };

        let topology = topology_identity_from_discovery(&discovery);
        assert_eq!(topology.links.len(), 1);
        assert_eq!(topology.links[0].from, "accelerator:node-a/gpu0");
        assert_eq!(topology.links[0].bandwidth_mbps, None);
    }

    #[test]
    fn parses_macos_hw_memsize_bytes_to_mib() {
        assert_eq!(parse_byte_count_mib("34359738368\n"), Some(32768));
    }

    #[test]
    fn parses_macos_vm_stat_available_shared_memory() {
        let raw = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:                               1000.\nPages active:                             5000.\nPages inactive:                           2000.\nPages speculative:                         500.\nPages wired down:                         3000.\n";
        assert_eq!(parse_vm_stat_available_mib(raw), Some(54));
    }

    #[test]
    fn vm_stat_requires_all_conservative_available_page_classes() {
        let raw = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 1000.\nPages inactive: 2000.\n";
        assert_eq!(parse_vm_stat_available_mib(raw), None);
    }

    #[test]
    fn parses_linux_meminfo() {
        let raw = "MemTotal:       131900000 kB\nMemFree:          100000 kB\n";
        assert_eq!(parse_linux_meminfo_mib(raw), Some(128808));
    }
}
