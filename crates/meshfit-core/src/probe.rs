use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    pub warnings: Vec<String>,
}

pub fn probe_peer(peer: &str, measure_bandwidth: bool) -> PeerProbeResult {
    let mut result = PeerProbeResult {
        peer: peer.to_string(),
        latency_ms: None,
        jitter_ms: None,
        bandwidth_gbps: None,
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
        match run_iperf3(peer) {
            Ok(raw) => match parse_iperf3_json(&raw) {
                Some(gbps) => result.bandwidth_gbps = Some(gbps),
                None => result
                    .warnings
                    .push("iperf3 completed but throughput could not be parsed".into()),
            },
            Err(reason) => result.warnings.push(reason),
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

fn run_iperf3(peer: &str) -> Result<String, String> {
    let output = Command::new("iperf3")
        .args(["-c", peer, "-J"])
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

pub fn parse_ping_summary(raw: &str) -> Option<(f64, f64)> {
    let line = raw.lines().find(|line| {
        line.contains("min/avg/max")
            || line.contains("round-trip min/avg/max")
            || line.contains("rtt min/avg/max")
    })?;

    let (_, values) = line.split_once('=')?;
    let values = values.trim().split_whitespace().next()?;
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
