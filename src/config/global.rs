//! Global configuration paths.
//!
//! Phase 1 only needs to know *where* global config and state live; the richer
//! global settings document arrives with later phases. Centralising path
//! resolution here keeps platform-specific directory logic out of the rest of
//! the codebase.
//!
//! Resolution is established now for later phases' global config and history
//! commands; it is not yet called from a command, hence the allow below.
#![allow(dead_code)]

use std::path::PathBuf;

use directories::ProjectDirs;

/// Resolved per-user directories for Smoothee.
#[derive(Debug, Clone)]
pub struct GlobalPaths {
    config_dir: PathBuf,
    data_dir: PathBuf,
}

impl GlobalPaths {
    /// Resolve the platform-appropriate config and data directories.
    ///
    /// Returns `None` if no home directory can be determined (rare, but
    /// possible in minimal sandboxes); callers should degrade gracefully.
    pub fn resolve() -> Option<Self> {
        let dirs = ProjectDirs::from("dev", "smoothee", "smoothee")?;
        Some(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
        })
    }

    /// Directory for the global config file.
    pub fn config_dir(&self) -> &std::path::Path {
        &self.config_dir
    }

    /// Directory for global state (operation history index, etc.).
    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }
}
