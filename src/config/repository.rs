//! Per-repository configuration, read from `.smoothee.toml` at the repo root.
//!
//! The schema mirrors the spec. All fields are optional so a repository with no
//! `.smoothee.toml` still yields a usable default configuration.

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The synchronization strategy Smoothee should prefer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SyncStrategy {
    /// Recommend merge or rebase based on whether the branch is shared.
    #[default]
    Auto,
    /// Always rebase.
    Rebase,
    /// Always merge.
    Merge,
}

/// Project-defined verification commands. Smoothee never hardcodes toolchain
/// commands; it runs exactly what the repository configures.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationConfig {
    pub format: Option<String>,
    pub lint: Option<String>,
    pub types: Option<String>,
    pub test: Option<String>,
}

/// AI-related settings for a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiConfig {
    pub enabled: bool,
    pub send_surrounding_lines: u32,
    pub confirm_before_send: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        // Safe defaults: AI on, but always confirm before any data leaves.
        Self {
            enabled: true,
            send_surrounding_lines: 40,
            confirm_before_send: true,
        }
    }
}

/// Privacy controls: globs that must never be sent to an AI provider.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacyConfig {
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// The full `.smoothee.toml` document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoConfig {
    pub base_branch: Option<String>,
    #[serde(default)]
    pub sync_strategy: SyncStrategy,
    #[serde(default)]
    pub verification: VerificationConfig,
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub privacy: PrivacyConfig,
}

/// Errors while loading repository configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
}

impl RepoConfig {
    /// The conventional config file name at the repository root.
    pub const FILE_NAME: &'static str = ".smoothee.toml";

    /// Load configuration from `<workdir>/.smoothee.toml`.
    ///
    /// A missing file is not an error — it yields [`RepoConfig::default`], so
    /// callers can treat "no config" and "empty config" identically.
    pub fn load(workdir: &Path) -> Result<Self, ConfigError> {
        let path = workdir.join(Self::FILE_NAME);
        if !path.exists() {
            return Ok(Self::default());
        }

        let text = std::fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;

        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.display().to_string(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_defaults() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = RepoConfig::load(dir.path()).unwrap();
        assert_eq!(cfg, RepoConfig::default());
        assert_eq!(cfg.sync_strategy, SyncStrategy::Auto);
        assert!(cfg.ai.enabled);
        assert!(cfg.ai.confirm_before_send);
    }

    #[test]
    fn parses_full_document() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(RepoConfig::FILE_NAME),
            r#"
base_branch = "main"
sync_strategy = "rebase"

[verification]
lint = "npm run lint"
test = "npm test"

[ai]
enabled = false
send_surrounding_lines = 20
confirm_before_send = true

[privacy]
exclude = [".env*", "secrets/**"]
"#,
        )
        .unwrap();

        let cfg = RepoConfig::load(dir.path()).unwrap();
        assert_eq!(cfg.base_branch.as_deref(), Some("main"));
        assert_eq!(cfg.sync_strategy, SyncStrategy::Rebase);
        assert_eq!(cfg.verification.lint.as_deref(), Some("npm run lint"));
        assert_eq!(cfg.verification.test.as_deref(), Some("npm test"));
        assert!(!cfg.ai.enabled);
        assert_eq!(cfg.ai.send_surrounding_lines, 20);
        assert_eq!(cfg.privacy.exclude, vec![".env*", "secrets/**"]);
    }

    #[test]
    fn rejects_invalid_toml() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(RepoConfig::FILE_NAME),
            "sync_strategy = \"nonsense\"\n",
        )
        .unwrap();
        assert!(matches!(
            RepoConfig::load(dir.path()),
            Err(ConfigError::Parse { .. })
        ));
    }
}
