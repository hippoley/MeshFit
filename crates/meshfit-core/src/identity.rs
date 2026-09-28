use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ir::{AcceleratorBackend, LinkKind, PlacementKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub vendor: String,
    pub model: String,
    pub backend: AcceleratorBackend,
    pub memory_mib: u64,
    #[serde(default)]
    pub driver_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareIdentity {
    pub architecture: String,
    pub operating_system: String,
    #[serde(default)]
    pub cpu_model: Option<String>,
    #[serde(default)]
    pub ram_mib: Option<u64>,
    #[serde(default)]
    pub devices: Vec<DeviceIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelArtifactIdentity {
    pub model_id: String,
    pub format: String,
    pub quantization: String,
    #[serde(default)]
    pub artifact_sha256: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeIdentity {
    pub runtime: String,
    pub version: String,
    #[serde(default)]
    pub build_commit: Option<String>,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkIdentity {
    pub from: String,
    pub to: String,
    pub kind: LinkKind,
    #[serde(default)]
    pub bandwidth_mbps: Option<u64>,
    #[serde(default)]
    pub latency_micros: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TopologyIdentity {
    #[serde(default)]
    pub links: Vec<LinkIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionIdentity {
    pub hardware: HardwareIdentity,
    pub model: ModelArtifactIdentity,
    pub runtime: RuntimeIdentity,
    pub topology: TopologyIdentity,
    pub placement: PlacementKind,
}

impl ExecutionIdentity {
    pub fn fingerprint(&self) -> String {
        let mut normalized = self.clone();

        normalized.hardware.devices.sort_by(|a, b| {
            (
                a.vendor.as_str(),
                a.model.as_str(),
                a.memory_mib,
                format!("{:?}", a.backend),
            )
                .cmp(&(
                    b.vendor.as_str(),
                    b.model.as_str(),
                    b.memory_mib,
                    format!("{:?}", b.backend),
                ))
        });

        normalized.topology.links.sort_by(|a, b| {
            (
                a.from.as_str(),
                a.to.as_str(),
                format!("{:?}", a.kind),
                a.bandwidth_mbps,
                a.latency_micros,
            )
                .cmp(&(
                    b.from.as_str(),
                    b.to.as_str(),
                    format!("{:?}", b.kind),
                    b.bandwidth_mbps,
                    b.latency_micros,
                ))
        });

        let bytes = serde_json::to_vec(&normalized)
            .expect("ExecutionIdentity serialization must not fail");
        let digest = Sha256::digest(bytes);
        hex_lower(&digest)
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("write to String cannot fail");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> ExecutionIdentity {
        ExecutionIdentity {
            hardware: HardwareIdentity {
                architecture: "x86_64".into(),
                operating_system: "linux".into(),
                cpu_model: Some("EPYC".into()),
                ram_mib: Some(262_144),
                devices: vec![
                    DeviceIdentity {
                        vendor: "nvidia".into(),
                        model: "H100".into(),
                        backend: AcceleratorBackend::Cuda,
                        memory_mib: 81_920,
                        driver_version: Some("580.65".into()),
                    },
                    DeviceIdentity {
                        vendor: "nvidia".into(),
                        model: "H100".into(),
                        backend: AcceleratorBackend::Cuda,
                        memory_mib: 81_920,
                        driver_version: Some("580.65".into()),
                    },
                ],
            },
            model: ModelArtifactIdentity {
                model_id: "qwen-72b-q4".into(),
                format: "gguf".into(),
                quantization: "q4_k_m".into(),
                artifact_sha256: Some("abc".into()),
                revision: Some("main".into()),
            },
            runtime: RuntimeIdentity {
                runtime: "vllm".into(),
                version: "0.10.2".into(),
                build_commit: None,
                flags: vec!["--tensor-parallel-size=2".into()],
            },
            topology: TopologyIdentity {
                links: vec![LinkIdentity {
                    from: "gpu0".into(),
                    to: "gpu1".into(),
                    kind: LinkKind::Nvlink,
                    bandwidth_mbps: Some(900_000),
                    latency_micros: Some(5),
                }],
            },
            placement: PlacementKind::TensorParallel,
        }
    }

    #[test]
    fn fingerprint_is_stable_against_device_order() {
        let first = identity();
        let mut second = first.clone();
        second.hardware.devices.reverse();
        assert_eq!(first.fingerprint(), second.fingerprint());
    }

    #[test]
    fn runtime_flag_value_pairing_changes_fingerprint() {
        let mut first = identity();
        first.runtime.flags = vec![
            "--max-model-len".into(),
            "4096".into(),
            "--tensor-parallel-size".into(),
            "2".into(),
        ];

        let mut second = identity();
        second.runtime.flags = vec![
            "--max-model-len".into(),
            "2".into(),
            "--tensor-parallel-size".into(),
            "4096".into(),
        ];

        assert_ne!(first.fingerprint(), second.fingerprint());
    }

    #[test]
    fn fingerprint_changes_when_runtime_changes() {
        let first = identity();
        let mut second = first.clone();
        second.runtime.version = "0.11.0".into();
        assert_ne!(first.fingerprint(), second.fingerprint());
    }
}
