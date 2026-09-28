use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::identity::RuntimeIdentity;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeDiscovery {
    pub runtimes: Vec<RuntimeIdentity>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

pub fn discover_runtimes() -> RuntimeDiscovery {
    let mut runtimes = Vec::new();
    let mut warnings = Vec::new();

    for (runtime, binaries) in [
        ("vllm", &["vllm"][..]),
        ("llama.cpp", &["llama-cli", "llama-server"][..]),
    ] {
        match discover_one(runtime, binaries) {
            Some(identity) => runtimes.push(identity),
            None => warnings.push(format!("{runtime}: no supported runtime binary found on PATH")),
        }
    }

    RuntimeDiscovery { runtimes, warnings }
}

fn discover_one(runtime: &str, binaries: &[&str]) -> Option<RuntimeIdentity> {
    for binary in binaries {
        let output = match Command::new(binary).arg("--version").output() {
            Ok(output) => output,
            Err(_) => continue,
        };
        if !output.status.success() {
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let version = if !stdout.is_empty() { stdout } else { stderr };
        if version.is_empty() {
            continue;
        }

        return Some(RuntimeIdentity {
            runtime: runtime.into(),
            version,
            build_commit: None,
            flags: vec![],
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_runtime_is_non_fatal() {
        let result = discover_one("missing", &["meshfit-runtime-that-does-not-exist"]);
        assert!(result.is_none());
    }
}
