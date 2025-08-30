use std::process::Command;

fn main() {
    // Base version from Cargo
    let base = env!("CARGO_PKG_VERSION");

    // Determine if this is a nightly build via env flag
    let is_nightly = std::env::var("PHAETON_NIGHTLY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    // Try to get short git sha if available
    let mut sha: Option<String> = None;
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        && output.status.success()
    {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            sha = Some(s);
        }
    }
    // Allow override via env (useful in CI without git)
    if sha.is_none()
        && let Ok(s) = std::env::var("GIT_SHA")
        && !s.is_empty()
    {
        sha = Some(s);
    }

    let version = if is_nightly {
        match sha {
            Some(s) => format!("{}-nightly+{}", base, s),
            None => format!("{}-nightly", base),
        }
    } else {
        base.to_string()
    };

    println!("cargo:rustc-env=APP_VERSION={}", version);

    // Rebuild when git HEAD changes or when PHAETON_NIGHTLY changes
    println!("cargo:rerun-if-env-changed=PHAETON_NIGHTLY");
    println!("cargo:rerun-if-env-changed=GIT_SHA");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
}
