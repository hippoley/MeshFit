use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=MESHFIT_GIT_COMMIT");

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

fn is_commit_like(value: &str) -> bool {
    (7..=64).contains(&value.len()) && value.chars().all(|ch| ch.is_ascii_hexdigit())
}
