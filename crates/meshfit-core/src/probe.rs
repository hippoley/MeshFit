use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ir::{FabricEdgeIR, FabricEndpointIR, LinkKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerProbeResult {
    pub peer: String,
    #[serde(default)]
    pub latency_ms: Option<f64>,
    #[serde(default)]
    pub jitter_ms: Option<f64>,
    #[serde(default)]
    pub bandwidth_gbps: Option<f64>,
    #[serde(default)]
    pub bandwidth_forward_gbps: Option<f64>,
    #[serde(default)]
    pub bandwidth_reverse_gbps: Option<f64>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub captured_at_unix_ms: Option<u128>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl PeerProbeResult {
    pub fn to_node_edge(
        &self,
        from_node: impl Into<String>,
        to_node: impl Into<String>,
        kind: LinkKind,
    ) -> FabricEdgeIR {
        FabricEdgeIR {
            from: FabricEndpointIR::Node {
                node: from_node.into(),
            },
            to: FabricEndpointIR::Node {
                node: to_node.into(),
            },
            kind,
            bandwidth_gbps: self.bandwidth_gbps,
            latency_ms: self.latency_ms,
            jitter_ms: self.jitter_ms.unwrap_or(0.0),
            egress_cost_usd_per_gb: 0.0,
        }
    }
}

pub fn probe_peer(peer: &str, measure_bandwidth: bool) -> PeerProbeResult {
    let captured_at_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis());
    let mut result = PeerProbeResult {
        peer: peer.to_string(),
        latency_ms: None,
        jitter_ms: None,
        bandwidth_gbps: None,
        bandwidth_forward_gbps: None,
        bandwidth_reverse_gbps: None,
        source: Some("meshfit-peer-probe".into()),
        captured_at_unix_ms,
        warnings: Vec::new(),
    };

    match run_ping(peer) {
        Ok(raw) => match parse_ping_summary(&raw) {
            Some((latency_ms, jitter_ms)) => {
                result.latency_ms = Some(latency_ms);
                result.jitter_ms = Some(jitter_ms);
            }
            None => result
                .warnings
                .push("ping completed but RTT summary could not be parsed".into()),
        },
        Err(reason) => result.warnings.push(reason),
    }

    if measure_bandwidth {
        match run_iperf3(peer, false) {
            Ok(raw) => match parse_iperf3_json(&raw) {
                Some(gbps) => result.bandwidth_forward_gbps = Some(gbps),
                None => result
                    .warnings
                    .push("forward iperf3 completed but throughput could not be parsed".into()),
            },
            Err(reason) => result.warnings.push(format!("forward {reason}")),
        }

        match run_iperf3(peer, true) {
            Ok(raw) => match parse_iperf3_json(&raw) {
                Some(gbps) => result.bandwidth_reverse_gbps = Some(gbps),
                None => result
                    .warnings
                    .push("reverse iperf3 completed but throughput could not be parsed".into()),
            },
            Err(reason) => result.warnings.push(format!("reverse {reason}")),
        }

        result.bandwidth_gbps = conservative_bidirectional_bandwidth(
            result.bandwidth_forward_gbps,
            result.bandwidth_reverse_gbps,
        );
        if result.bandwidth_gbps.is_none() {
            result.warnings.push(
                "bidirectional bandwidth evidence incomplete; conservative effective bandwidth omitted"
                    .into(),
            );
        }
    }

    result
}

fn run_ping(peer: &str) -> Result<String, String> {
    let output = Command::new("ping")
        .args(["-c", "4", "-W", "2", peer])
        .output()
        .map_err(|e| format!("ping unavailable: {e}"))?;

    if !output.status.success() {
        return Err(format!("ping failed with status {}", output.status));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("ping output is not UTF-8: {e}"))
}

fn run_iperf3(peer: &str, reverse: bool) -> Result<String, String> {
    let mut command = Command::new("iperf3");
    command.args(["-c", peer, "-J"]);
    if reverse {
        command.arg("-R");
    }
    let output = command
        .output()
        .map_err(|e| format!("iperf3 unavailable: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "iperf3 failed with status {}; ensure an iperf3 server is running on the peer",
            output.status
        ));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("iperf3 output is not UTF-8: {e}"))
}

fn conservative_bidirectional_bandwidth(
    forward_gbps: Option<f64>,
    reverse_gbps: Option<f64>,
) -> Option<f64> {
    Some(forward_gbps?.min(reverse_gbps?))
}

pub fn parse_ping_summary(raw: &str) -> Option<(f64, f64)> {
    let line = raw.lines().find(|line| {
        line.contains("min/avg/max")
            || line.contains("round-trip min/avg/max")
            || line.contains("rtt min/avg/max")
    })?;

    let (_, values) = line.split_once('=')?;
    let values = values.split_whitespace().next()?;
    let parts: Vec<_> = values.split('/').collect();
    if parts.len() < 4 {
        return None;
    }

    let avg = parts[1].parse::<f64>().ok()?;
    let jitter = parts[3].parse::<f64>().ok()?;
    Some((avg, jitter))
}

pub fn parse_iperf3_json(raw: &str) -> Option<f64> {
    let value: Value = serde_json::from_str(raw).ok()?;

    let bits_per_second = value
        .pointer("/end/sum_received/bits_per_second")
        .and_then(Value::as_f64)
        .or_else(|| {
            value
                .pointer("/end/sum_sent/bits_per_second")
                .and_then(Value::as_f64)
        })?;

    Some(bits_per_second / 1_000_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_ping_summary() {
        let raw = "64 bytes from 10.0.0.2: icmp_seq=1 ttl=64 time=0.320 ms\n\nrtt min/avg/max/mdev = 0.301/0.355/0.421/0.048 ms\n";
        let (latency, jitter) = parse_ping_summary(raw).unwrap();
        assert_eq!(latency, 0.355);
        assert_eq!(jitter, 0.048);
    }

    #[test]
    fn converts_probe_to_graph_ready_node_edge() {
        let probe = PeerProbeResult {
            peer: "10.0.0.2".into(),
            latency_ms: Some(0.42),
            jitter_ms: Some(0.03),
            bandwidth_gbps: Some(21.8),
            bandwidth_forward_gbps: Some(24.1),
            bandwidth_reverse_gbps: Some(21.8),
            source: Some("meshfit-peer-probe".into()),
            captured_at_unix_ms: Some(1_700_000_000_000),
            warnings: vec![],
        };
        let edge = probe.to_node_edge("node-a", "node-b", LinkKind::Ethernet);
        assert_eq!(
            edge.from,
            FabricEndpointIR::Node {
                node: "node-a".into()
            }
        );
        assert_eq!(edge.bandwidth_gbps, Some(21.8));
        assert_eq!(edge.latency_ms, Some(0.42));
    }

    #[test]
    fn uses_slower_direction_as_conservative_bandwidth() {
        assert_eq!(
            conservative_bidirectional_bandwidth(Some(24.1), Some(21.8)),
            Some(21.8)
        );
        assert_eq!(
            conservative_bidirectional_bandwidth(Some(24.1), None),
            None
        );
    }

    #[test]
    fn parses_iperf3_received_throughput() {
        let raw = r#"{
          "end": {
            "sum_received": {
              "bits_per_second": 21800000000.0
            }
          }
        }"#;

        assert_eq!(parse_iperf3_json(raw), Some(21.8));
    }
}
