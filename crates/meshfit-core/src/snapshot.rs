use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    discovery::LocalDiscovery,
    identity::HardwareIdentity,
    ir::{InfrastructureIR, LinkKind},
    probe::PeerProbeResult,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotProbeSpec {
    pub from_node: String,
    pub to_node: String,
    pub probe_file: String,
    pub kind: LinkKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotManifest {
    pub discovery_files: Vec<String>,
    #[serde(default)]
    pub probes: Vec<SnapshotProbeSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerMeasurementEvidence {
    pub from_node: String,
    pub to_node: String,
    pub kind: LinkKind,
    #[serde(default)]
    pub latency_ms: Option<f64>,
    #[serde(default)]
    pub jitter_ms: Option<f64>,
    #[serde(default)]
    pub bandwidth_gbps: Option<f64>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub captured_at_unix_ms: Option<u128>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InfrastructureSnapshot {
    pub infrastructure: InfrastructureIR,
    #[serde(default)]
    pub hardware_identities: BTreeMap<String, HardwareIdentity>,
    #[serde(default)]
    pub peer_measurements: Vec<PeerMeasurementEvidence>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    DuplicateNode(String),
    MissingLocalNode(String),
    MissingPeerNode(String),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotError::DuplicateNode(node) => write!(f, "duplicate node id: {node}"),
            SnapshotError::MissingLocalNode(node) => write!(f, "local node not found: {node}"),
            SnapshotError::MissingPeerNode(node) => write!(f, "peer node not found: {node}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

impl InfrastructureSnapshot {
    pub fn from_discoveries(discoveries: Vec<LocalDiscovery>) -> Result<Self, SnapshotError> {
        let mut seen = HashSet::new();
        let mut nodes = Vec::new();
        let mut links = Vec::new();
        let mut hardware_identities = BTreeMap::new();
        let mut warnings = Vec::new();

        for discovery in discoveries {
            let node_id = discovery.node.id.clone();
            if !seen.insert(node_id.clone()) {
                return Err(SnapshotError::DuplicateNode(node_id));
            }

            hardware_identities.insert(node_id.clone(), discovery.hardware_identity);
            nodes.push(discovery.node);
            links.extend(discovery.local_fabric);
            warnings.extend(
                discovery
                    .warnings
                    .into_iter()
                    .map(|warning| format!("{node_id}: {warning}")),
            );
        }

        Ok(Self {
            infrastructure: InfrastructureIR { nodes, links },
            hardware_identities,
            peer_measurements: Vec::new(),
            warnings,
        })
    }

    pub fn add_peer_probe(
        &mut self,
        from_node: &str,
        to_node: &str,
        probe: &PeerProbeResult,
        kind: LinkKind,
    ) -> Result<(), SnapshotError> {
        if self.infrastructure.node(from_node).is_none() {
            return Err(SnapshotError::MissingLocalNode(from_node.to_string()));
        }
        if self.infrastructure.node(to_node).is_none() {
            return Err(SnapshotError::MissingPeerNode(to_node.to_string()));
        }

        self.infrastructure
            .links
            .push(probe.to_node_edge(from_node, to_node, kind));
        self.peer_measurements.push(PeerMeasurementEvidence {
            from_node: from_node.to_string(),
            to_node: to_node.to_string(),
            kind,
            latency_ms: probe.latency_ms,
            jitter_ms: probe.jitter_ms,
            bandwidth_gbps: probe.bandwidth_gbps,
            source: probe.source.clone(),
            captured_at_unix_ms: probe.captured_at_unix_ms,
        });

        self.warnings.extend(
            probe
                .warnings
                .iter()
                .map(|warning| format!("{from_node}->{to_node}: {warning}")),
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        discovery::LocalDiscovery,
        identity::HardwareIdentity,
        ir::{FabricEndpointIR, HardwareNodeIR},
    };

    fn discovery(node_id: &str) -> LocalDiscovery {
        LocalDiscovery {
            hardware_identity: HardwareIdentity {
                architecture: "x86_64".into(),
                operating_system: "linux".into(),
                cpu_model: None,
                ram_mib: Some(65_536),
                devices: vec![],
            },
            node: HardwareNodeIR {
                id: node_id.into(),
                site: "local".into(),
                ram_gb: 64.0,
                accelerators: vec![],
                hourly_cost_usd: 0.0,
            },
            local_fabric: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn merges_two_discoveries_and_peer_measurement() {
        let mut snapshot = InfrastructureSnapshot::from_discoveries(vec![
            discovery("node-a"),
            discovery("node-b"),
        ])
        .unwrap();

        snapshot
            .add_peer_probe(
                "node-a",
                "node-b",
                &PeerProbeResult {
                    peer: "10.0.0.2".into(),
                    latency_ms: Some(0.42),
                    jitter_ms: Some(0.03),
                    bandwidth_gbps: Some(21.8),
                    source: Some("meshfit-peer-probe".into()),
                    captured_at_unix_ms: Some(1_700_000_000_000),
                    warnings: vec![],
                },
                LinkKind::Ethernet,
            )
            .unwrap();

        assert_eq!(snapshot.infrastructure.nodes.len(), 2);
        assert_eq!(snapshot.infrastructure.links.len(), 1);

        let link = &snapshot.infrastructure.links[0];
        assert_eq!(
            link.from,
            FabricEndpointIR::Node {
                node: "node-a".into()
            }
        );
        assert_eq!(link.bandwidth_gbps, Some(21.8));
        assert_eq!(snapshot.peer_measurements.len(), 1);
        assert_eq!(
            snapshot.peer_measurements[0].source.as_deref(),
            Some("meshfit-peer-probe")
        );
        assert_eq!(
            snapshot.peer_measurements[0].captured_at_unix_ms,
            Some(1_700_000_000_000)
        );
    }

    #[test]
    fn rejects_duplicate_node_ids() {
        let result = InfrastructureSnapshot::from_discoveries(vec![
            discovery("node-a"),
            discovery("node-a"),
        ]);

        assert_eq!(
            result.unwrap_err(),
            SnapshotError::DuplicateNode("node-a".into())
        );
    }
}
