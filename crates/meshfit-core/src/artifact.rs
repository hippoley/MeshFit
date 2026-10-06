use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use sha2::{Digest, Sha256};

use crate::identity::ModelArtifactIdentity;

pub fn inspect_model_artifact(
    path: impl AsRef<Path>,
    model_id: impl Into<String>,
    format: impl Into<String>,
    quantization: impl Into<String>,
    revision: Option<String>,
) -> Result<ModelArtifactIdentity, String> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let digest = hasher.finalize();
    let mut sha = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut sha, "{byte:02x}").expect("write to String cannot fail");
    }

    Ok(ModelArtifactIdentity {
        model_id: model_id.into(),
        format: format.into(),
        quantization: quantization.into(),
        artifact_sha256: Some(sha),
        revision,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn hashes_real_file_bytes() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("meshfit-artifact-{suffix}.bin"));
        fs::write(&path, b"meshfit").unwrap();

        let identity =
            inspect_model_artifact(&path, "demo", "gguf", "q4_k_m", Some("test".into())).unwrap();

        assert_eq!(
            identity.artifact_sha256.as_deref(),
            Some("1700d6ad0d90692a1ee5680de2e002e9c32d7923afa67834f4a04a85d604a034")
        );

        let _ = fs::remove_file(path);
    }
}
