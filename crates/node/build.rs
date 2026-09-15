//! Embed the short git sha so `web3_clientVersion`, the startup banner and the
//! verified-node table identify the build (`Mersennet/0.7.0-9a3a5bd`) instead
//! of reporting `-dev` for every binary. Precedence: `MERSENNET_GIT_SHA` from
//! the environment (release packaging), then `git rev-parse`, then `dev`.
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn main() {
    let sha = std::env::var("MERSENNET_GIT_SHA")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| git(&["rev-parse", "--short=7", "HEAD"]))
        .unwrap_or_else(|| "dev".to_string());
    println!("cargo:rustc-env=MERSENNET_GIT_SHA={sha}");
    println!("cargo:rerun-if-env-changed=MERSENNET_GIT_SHA");
    // Rebuild when HEAD moves so the sha never goes stale in incremental builds.
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/refs/heads");
    }
}
