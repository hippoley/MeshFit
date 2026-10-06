use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=MESHFIT_GIT_COMMIT");
    register_git_rerun_paths();

    if let Ok(commit) = env::var("MESHFIT_GIT_COMMIT") {
        let commit = commit.trim();
        if is_commit_like(commit) {
            println!("cargo:rustc-env=MESHFIT_GIT_COMMIT={commit}");
        }
        return;
    }

    let Ok(output) = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
    else {
        return;
    };

    if !output.status.success() {
        return;
    }

    let Ok(commit) = String::from_utf8(output.stdout) else {
        return;
    };
    let commit = commit.trim();

    if is_commit_like(commit) {
        println!("cargo:rustc-env=MESHFIT_GIT_COMMIT={commit}");
    }
}

fn register_git_rerun_paths() {
    let Some(git_dir) = git_output(&["rev-parse", "--git-dir"]).map(PathBuf::from) else {
        return;
    };

    let head = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head.display());
    println!(
        "cargo:rerun-if-changed={}",
        git_dir.join("packed-refs").display()
    );

    let Ok(head_content) = fs::read_to_string(&head) else {
        return;
    };

    if let Some(reference) = head_content.trim().strip_prefix("ref: ") {
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join(reference).display()
        );
    }
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn is_commit_like(value: &str) -> bool {
    (7..=64).contains(&value.len()) && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_git_like_commit_ids() {
        assert!(is_commit_like("67b7be6aa00cbca9a281f25cf3f35ff19ffce2f1"));
        assert!(is_commit_like("abcdef0"));
        assert!(!is_commit_like("not-a-commit"));
        assert!(!is_commit_like("abc"));
    }
}
