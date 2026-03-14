#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

#[cfg(all(
    not(target_arch = "wasm32"),
    any(target_os = "macos", target_os = "ios")
))]
const IOS_MACOS_APP_DIR: &str = "app.adarcher.rustysound";
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
const WINDOWS_APP_DIR: &str = "RustySound";
#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
const LINUX_APP_DIR: &str = "rustysound";
#[cfg(not(target_arch = "wasm32"))]
const FALLBACK_APP_DIR: &str = ".rustysound";
#[cfg(not(target_arch = "wasm32"))]
const LEGACY_CACHE_DIR: &str = "rustysound";

#[cfg(not(target_arch = "wasm32"))]
pub fn app_data_dir() -> Option<PathBuf> {
    let dir = preferred_data_dir()?;
    migrate_dir_if_missing(&dir, &legacy_data_dir_candidates());
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn app_cache_dir() -> Option<PathBuf> {
    let dir = preferred_cache_dir()?;
    migrate_dir_if_missing(&dir, &legacy_cache_dir_candidates());
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

#[cfg(not(target_arch = "wasm32"))]
fn preferred_data_dir() -> Option<PathBuf> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        return home_dir().map(|home| {
            home.join("Library")
                .join("Application Support")
                .join(IOS_MACOS_APP_DIR)
        });
    }

    #[cfg(target_os = "windows")]
    {
        return std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|dir| dir.join(WINDOWS_APP_DIR));
    }

    #[cfg(target_os = "linux")]
    {
        return home_dir().map(|home| home.join(".local").join("share").join(LINUX_APP_DIR));
    }

    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "windows",
        target_os = "linux"
    )))]
    {
        return home_dir()
            .map(|home| home.join(FALLBACK_APP_DIR))
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|dir| dir.join(FALLBACK_APP_DIR))
            });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn preferred_cache_dir() -> Option<PathBuf> {
    #[cfg(target_os = "ios")]
    {
        return app_data_dir().map(|dir| dir.join("cache"));
    }

    #[cfg(not(target_os = "ios"))]
    {
        return dirs::cache_dir()
            .map(|dir| dir.join(LEGACY_CACHE_DIR))
            .or_else(|| app_data_dir().map(|dir| dir.join("cache")));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn legacy_data_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(home) = home_dir() {
        candidates.push(home.join(FALLBACK_APP_DIR));
    }

    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(FALLBACK_APP_DIR));
    }

    dedupe_paths(candidates)
}

#[cfg(not(target_arch = "wasm32"))]
fn legacy_cache_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(cache_dir) = dirs::cache_dir() {
        candidates.push(cache_dir.join(LEGACY_CACHE_DIR));
    }

    if let Some(data_dir) = app_data_dir() {
        candidates.push(data_dir.join("cache"));
    }

    dedupe_paths(candidates)
}

#[cfg(not(target_arch = "wasm32"))]
fn migrate_dir_if_missing(dest: &Path, candidates: &[PathBuf]) {
    if dest.exists() {
        return;
    }

    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }

    for candidate in candidates {
        if candidate == dest || !candidate.exists() {
            continue;
        }

        if fs::rename(candidate, dest).is_ok() {
            return;
        }

        if copy_dir_recursive(candidate, dest).is_ok() {
            let _ = fs::remove_dir_all(candidate);
            return;
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn copy_dir_recursive(source: &Path, dest: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            copy_dir_recursive(&source_path, &dest_path)?;
        } else if metadata.is_file() {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &dest_path)?;
        }
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut output = Vec::new();
    for path in paths {
        if output.iter().any(|existing| existing == &path) {
            continue;
        }
        output.push(path);
    }
    output
}

#[cfg(not(target_arch = "wasm32"))]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
