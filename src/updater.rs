//! Self-update functionality for Phaeton
//!
//! This module provides Git-based self-update capabilities to keep
//! the application up-to-date with the latest releases.

use crate::error::{PhaetonError, Result};
use crate::logging::get_logger;
#[cfg(feature = "updater")]
use ammonia::Builder as AmmoniaBuilder;
#[cfg(feature = "updater")]
use pulldown_cmark::{Options, Parser, html};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub tag: String,
    pub name: Option<String>,
    pub draft: bool,
    pub prerelease: bool,
    pub published_at: Option<String>,
    pub body: Option<String>,
    /// Sanitized HTML rendered from `body` (Markdown)
    pub body_html: Option<String>,
}

/// Update status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub last_check: Option<u64>,
    pub error: Option<String>,
}

/// Git updater for self-updates
pub struct GitUpdater {
    #[allow(dead_code)]
    repo_url: String,
    #[allow(dead_code)]
    current_branch: String,
    #[allow(dead_code)]
    logger: crate::logging::StructuredLogger,
}

impl GitUpdater {
    /// Create new Git updater
    pub fn new(repo_url: String, current_branch: String) -> Self {
        let logger = get_logger("updater");
        Self {
            repo_url,
            current_branch,
            logger,
        }
    }

    /// Check for available updates (stable only)
    pub async fn check_for_updates(&mut self) -> Result<UpdateStatus> {
        self.check_for_updates_with_prereleases(false).await
    }

    /// Check for available updates with optional prereleases
    pub async fn check_for_updates_with_prereleases(
        &mut self,
        include_prerelease: bool,
    ) -> Result<UpdateStatus> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let current_version = Self::current_version_string();
        match self.list_releases(include_prerelease).await {
            Ok(list) => {
                let latest = list
                    .into_iter()
                    .find(|r| !r.draft && (!r.prerelease || include_prerelease));
                let latest_version = latest.as_ref().map(|r| r.tag.clone());
                let update_available = latest_version
                    .as_ref()
                    .map(|v| Self::is_newer_semver(v, &current_version))
                    .unwrap_or(false);
                Ok(UpdateStatus {
                    current_version,
                    latest_version,
                    update_available,
                    last_check: Some(now),
                    error: None,
                })
            }
            Err(e) => Ok(UpdateStatus {
                current_version,
                latest_version: None,
                update_available: false,
                last_check: Some(now),
                error: Some(e.to_string()),
            }),
        }
    }

    /// Apply available updates (latest stable)
    pub async fn apply_updates(&mut self) -> Result<()> {
        self.apply_updates_with_prereleases(false).await
    }

    /// Apply available updates with optional prereleases
    pub async fn apply_updates_with_prereleases(&mut self, include_prerelease: bool) -> Result<()> {
        self.apply_release_with_prereleases(None, include_prerelease)
            .await
    }

    /// List GitHub releases (most recent first)
    pub async fn list_releases(&self, include_prerelease: bool) -> Result<Vec<ReleaseInfo>> {
        let (owner, repo) = Self::parse_repo(&self.repo_url)
            .ok_or_else(|| PhaetonError::update("Invalid repository URL"))?;
        let url = format!("https://api.github.com/repos/{}/{}/releases", owner, repo);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(format!("phaeton/{}", Self::current_version_string()))
            .build()?;
        let resp = client.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(PhaetonError::update(format!(
                "GitHub API error: {}",
                resp.status()
            )));
        }
        let json = resp.json::<serde_json::Value>().await?;
        let mut out: Vec<ReleaseInfo> = Vec::new();
        if let Some(arr) = json.as_array() {
            for r in arr {
                let prerelease = r
                    .get("prerelease")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let draft = r.get("draft").and_then(|v| v.as_bool()).unwrap_or(false);
                if !include_prerelease && (prerelease || draft) {
                    continue;
                }
                let tag = r
                    .get("tag_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = r
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let published_at = r
                    .get("published_at")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let body = r
                    .get("body")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let body_html = body.as_ref().and_then(|md| Self::render_markdown_safe(md));
                if !tag.is_empty() {
                    out.push(ReleaseInfo {
                        tag,
                        name,
                        draft,
                        prerelease,
                        published_at,
                        body,
                        body_html,
                    });
                }
            }
        }
        Ok(out)
    }

    /// Apply a given release tag, or the latest stable if None
    pub async fn apply_release(&mut self, tag: Option<String>) -> Result<()> {
        self.apply_release_with_prereleases(tag, false).await
    }

    /// Apply a given release tag, or the latest (optionally including prereleases) if None
    pub async fn apply_release_with_prereleases(
        &mut self,
        tag: Option<String>,
        include_prerelease: bool,
    ) -> Result<()> {
        let (owner, repo) = Self::parse_repo(&self.repo_url)
            .ok_or_else(|| PhaetonError::update("Invalid repository URL"))?;
        let target_tag = if let Some(t) = tag {
            t
        } else {
            let releases = self.list_releases(include_prerelease).await?;
            releases
                .into_iter()
                .find(|r| !r.draft && (!r.prerelease || include_prerelease))
                .map(|r| r.tag)
                .ok_or_else(|| PhaetonError::update("No suitable releases found"))?
        };

        // Fetch release by tag to get assets
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/tags/{}",
            owner, repo, target_tag
        );
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent(format!("phaeton/{}", Self::current_version_string()))
            .build()?;
        let resp = client.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(PhaetonError::update(format!(
                "GitHub API error: {}",
                resp.status()
            )));
        }
        let json = resp.json::<serde_json::Value>().await?;
        let assets = json
            .get("assets")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let asset = Self::select_asset_for_current(&assets)
            .ok_or_else(|| PhaetonError::update("No matching asset for this platform"))?;
        let url = asset
            .get("browser_download_url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PhaetonError::update("Asset missing download URL"))?;

        // Download to temp file
        let tmp_path = Self::download_to_temp(&client, url).await?;
        // Replace current executable
        Self::replace_current_executable(&tmp_path)?;
        // Attempt restart
        Self::restart_after_delay(Duration::from_secs(1));
        Ok(())
    }

    /// Get current status
    pub fn get_status(&self) -> UpdateStatus {
        UpdateStatus {
            current_version: Self::current_version_string(),
            latest_version: None,
            update_available: false,
            last_check: None,
            error: None,
        }
    }

    fn current_version_string() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn normalize_tag(tag: &str) -> &str {
        tag.strip_prefix('v').unwrap_or(tag)
    }

    fn is_newer_semver(tag_a: &str, current: &str) -> bool {
        // Compare semver strings, ignore leading 'v'
        let a = Self::normalize_tag(tag_a);
        let b = Self::normalize_tag(current);
        let pa: Vec<u32> = a.split('.').filter_map(|s| s.parse::<u32>().ok()).collect();
        let pb: Vec<u32> = b.split('.').filter_map(|s| s.parse::<u32>().ok()).collect();
        for i in 0..3 {
            let va = *pa.get(i).unwrap_or(&0);
            let vb = *pb.get(i).unwrap_or(&0);
            if va != vb {
                return va > vb;
            }
        }
        false
    }

    #[cfg(feature = "updater")]
    fn render_markdown_safe(md: &str) -> Option<String> {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_FOOTNOTES);
        let parser = Parser::new_ext(md, options);
        let mut html_buf = String::new();
        html::push_html(&mut html_buf, parser);
        let clean = AmmoniaBuilder::default()
            .add_generic_attributes(&["class", "id", "aria-hidden"]) // minimal
            .clean(&html_buf)
            .to_string();
        Some(clean)
    }

    #[cfg(not(feature = "updater"))]
    fn render_markdown_safe(_md: &str) -> Option<String> {
        None
    }

    fn parse_repo(repo_url: &str) -> Option<(String, String)> {
        // Expecting https://github.com/owner/repo
        let parts: Vec<&str> = repo_url.trim_end_matches('/').split('/').collect();
        if parts.len() < 2 {
            return None;
        }
        let repo = parts.last()?.to_string();
        let owner = parts.get(parts.len() - 2)?.to_string();
        Some((owner, repo))
    }

    fn select_asset_for_current(assets: &[serde_json::Value]) -> Option<serde_json::Value> {
        // Heuristics: prefer raw binary named 'phaeton' or containing OS/arch hints
        let arch = option_env!("CARGO_CFG_TARGET_ARCH").unwrap_or("");
        let os = option_env!("CARGO_CFG_TARGET_OS").unwrap_or("");
        let mut candidates = assets.iter().filter(|a| {
            a.get("name")
                .and_then(|v| v.as_str())
                .map(|n| {
                    let ln = n.to_ascii_lowercase();
                    ln.contains("phaeton")
                        && (ln.contains(arch)
                            || ln.contains(os)
                            || ln.ends_with(".bin")
                            || ln.ends_with(".zip")
                            || ln.ends_with(".tar.gz"))
                })
                .unwrap_or(false)
        });
        // Prefer simple binary first
        for a in candidates.clone() {
            let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name == "phaeton" || name == "phaeton.bin" {
                return Some(a.clone());
            }
        }
        candidates.next().cloned()
    }

    async fn download_to_temp(client: &reqwest::Client, url: &str) -> Result<PathBuf> {
        // Prefer staging next to the running executable to avoid cross-device rename issues
        // common on embedded systems (e.g. /tmp vs /data).
        let logger = get_logger("updater");
        let staging_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or(std::env::temp_dir());

        let mut resp = client.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(PhaetonError::update(format!(
                "Download failed: {}",
                resp.status()
            )));
        }

        let mut path = staging_dir.clone();
        let filename = format!("phaeton-download-{}", std::process::id());
        path.push(&filename);
        logger.debug(&format!(
            "Downloading update to staging file: {}",
            path.display()
        ));

        let mut file = std::fs::File::create(&path)?;
        while let Some(chunk) = resp.chunk().await? {
            use std::io::Write;
            file.write_all(&chunk)?;
        }
        // Ensure data hits the disk before replacement attempt
        let _ = file.sync_all();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms)?;
        }
        Ok(path)
    }

    fn replace_current_executable(tmp_path: &Path) -> Result<()> {
        let logger = get_logger("updater");
        let current = std::env::current_exe().map_err(|e| PhaetonError::update(e.to_string()))?;
        let backup = current.with_extension("old");
        logger.info(&format!(
            "Applying update: current={}, staging={}, backup={}",
            current.display(),
            tmp_path.display(),
            backup.display()
        ));

        // Best-effort backup of current executable
        let _ = std::fs::rename(&current, &backup);

        match std::fs::rename(tmp_path, &current) {
            Ok(_) => {
                logger.info("Update applied via atomic rename");
                Ok(())
            }
            Err(err) => {
                // Handle cross-device link (EXDEV) by falling back to copy + fsync
                logger.warn(&format!(
                    "Primary rename failed: {}. Falling back to copy.",
                    err
                ));
                let copy_res = (|| {
                    let mut from = std::fs::File::open(tmp_path)?;
                    let mut to = std::fs::File::create(&current)?;
                    std::io::copy(&mut from, &mut to)?;
                    let _ = to.sync_all();
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mut perms = to.metadata()?.permissions();
                        perms.set_mode(0o755);
                        std::fs::set_permissions(&current, perms)?;
                    }
                    Ok::<(), std::io::Error>(())
                })();

                if let Err(copy_err) = copy_res {
                    // Try to restore backup to minimize downtime
                    let _ = std::fs::rename(&backup, &current);
                    return Err(PhaetonError::update(format!(
                        "Failed to apply update: {}; copy fallback failed: {}",
                        err, copy_err
                    )));
                }

                logger.info("Update applied via copy fallback");
                // Best-effort cleanup of staging file
                let _ = std::fs::remove_file(tmp_path);
                Ok(())
            }
        }
    }

    fn restart_after_delay(delay: Duration) {
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            // Try to re-exec the current binary with same args
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                let logger = get_logger("updater");
                let exe = std::env::current_exe()
                    .unwrap_or_else(|_| std::path::PathBuf::from("/proc/self/exe"));
                let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
                let mut cmd = std::process::Command::new(exe);
                if args.len() > 1 {
                    cmd.args(&args[1..]);
                }
                let e = cmd.exec();
                logger.warn(&format!("exec() failed: {}", e));
            }
            // Fallback: spawn new then exit
            let _ = std::process::Command::new(
                std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("phaeton")),
            )
            .args(std::env::args().skip(1))
            .spawn();
            std::process::exit(0);
        });
    }
}
