use crate::error::{PhaetonError, Result};
use crate::logging::get_logger;
use std::path::Path;

#[cfg(feature = "updater")]
use flate2::read::GzDecoder;
#[cfg(feature = "updater")]
use tar::Archive;

#[cfg(feature = "updater")]
pub fn is_gzip_file(path: &Path) -> Option<bool> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut magic = [0u8; 2];
    if file.read_exact(&mut magic).is_ok() {
        return Some(magic == [0x1F, 0x8B]);
    }
    None
}

#[cfg(feature = "updater")]
pub fn apply_package_archive(archive_path: &Path) -> Result<()> {
    let _logger = get_logger("updater");
    let install_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .ok_or_else(|| PhaetonError::update("Cannot determine install directory"))?;

    // Create staging directory alongside current install
    let staging_dir = install_dir.join(format!("update-staging-{}", std::process::id()));
    if staging_dir.exists() {
        let _ = std::fs::remove_dir_all(&staging_dir);
    }
    std::fs::create_dir_all(&staging_dir)?;

    // Extract tar.gz
    let file = std::fs::File::open(archive_path)?;
    let dec = GzDecoder::new(file);
    let mut ar = Archive::new(dec);
    ar.unpack(&staging_dir)
        .map_err(|e| PhaetonError::update(format!("Failed to extract package: {}", e)))?;

    // Install webui directory if present
    let src_webui = staging_dir.join("webui");
    if src_webui.is_dir() {
        let dest_webui = install_dir.join("webui");
        replace_directory_atomic(&src_webui, &dest_webui)?;
    }

    // Install sample config if present (do not touch active config)
    let src_sample = staging_dir.join("phaeton_config.sample.yaml");
    if src_sample.is_file() {
        let dest_sample = install_dir.join("phaeton_config.sample.yaml");
        replace_file_atomic(&src_sample, &dest_sample, 0o644)?;
    }

    // Replace current executable last
    let src_bin = staging_dir.join("phaeton");
    if src_bin.is_file() {
        // Ensure executable bit
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&src_bin)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&src_bin, perms)?;
        }
        super::GitUpdater::replace_current_executable(&src_bin)?;
    } else {
        return Err(PhaetonError::update(
            "Package missing 'phaeton' binary after extraction",
        ));
    }

    // Best-effort cleanup of staging
    let _ = std::fs::remove_dir_all(&staging_dir);
    Ok(())
}

#[cfg(feature = "updater")]
fn replace_directory_atomic(src_dir: &Path, dest_dir: &Path) -> Result<()> {
    let logger = get_logger("updater");
    let backup_dir = dest_dir.with_extension("old");

    // Remove any previous backup
    let _ = std::fs::remove_dir_all(&backup_dir);

    if dest_dir.exists()
        && let Err(e) = std::fs::rename(dest_dir, &backup_dir)
    {
        logger.warn(&format!(
            "Failed to backup existing directory {}: {}",
            dest_dir.display(),
            e
        ));
        // Best-effort clean before copy fallback
        let _ = std::fs::remove_dir_all(&backup_dir);
    }

    match std::fs::rename(src_dir, dest_dir) {
        Ok(_) => {
            let _ = std::fs::remove_dir_all(&backup_dir);
            Ok(())
        }
        Err(rename_err) => {
            // Fallback to recursive copy
            logger.warn(&format!(
                "Directory rename failed: {}. Falling back to copy.",
                rename_err
            ));
            if !dest_dir.exists() {
                std::fs::create_dir_all(dest_dir)?;
            }
            copy_dir_recursive(src_dir, dest_dir)?;
            // Cleanup
            let _ = std::fs::remove_dir_all(src_dir);
            let _ = std::fs::remove_dir_all(&backup_dir);
            Ok(())
        }
    }
}

#[cfg(feature = "updater")]
fn copy_dir_recursive(from: &Path, to: &Path) -> Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = to.join(entry.file_name());
        if src_path.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(feature = "updater")]
fn replace_file_atomic(src: &Path, dest: &Path, mode: u32) -> Result<()> {
    let backup = dest.with_extension("old");
    let _ = std::fs::remove_file(&backup);
    if dest.exists() {
        let _ = std::fs::rename(dest, &backup);
    }
    match std::fs::rename(src, dest) {
        Ok(_) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(dest)?.permissions();
                perms.set_mode(mode);
                std::fs::set_permissions(dest, perms)?;
            }
            let _ = std::fs::remove_file(&backup);
            Ok(())
        }
        Err(_) => {
            // Fallback to copy
            std::fs::copy(src, dest)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(dest)?.permissions();
                perms.set_mode(mode);
                std::fs::set_permissions(dest, perms)?;
            }
            let _ = std::fs::remove_file(src);
            let _ = std::fs::remove_file(&backup);
            Ok(())
        }
    }
}
