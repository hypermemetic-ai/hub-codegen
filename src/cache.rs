use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Toolchain version information for cache invalidation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainVersions {
    #[serde(rename = "synapse-cc")]
    pub synapse_cc: String,
    pub synapse: String,
    #[serde(rename = "hub-codegen")]
    pub hub_codegen: String,
}

/// Cache entry for a single plugin's generated code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePluginCache {
    /// Hash of the IR that generated this code
    #[serde(rename = "irHash")]
    pub ir_hash: String,

    /// Per-file hashes for granular change detection
    /// Map of relative file path -> hash
    #[serde(rename = "fileHashes")]
    pub file_hashes: HashMap<String, String>,

    /// ISO 8601 timestamp when this was cached
    #[serde(rename = "cachedAt")]
    pub cached_at: String,
}

/// Code cache manifest (written to hub-codegen/{target}/{backend}/manifest.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeCacheManifest {
    /// Manifest format version
    pub version: String,

    /// Target language (typescript, python, rust)
    pub target: String,

    /// Toolchain versions for invalidation
    pub toolchain: ToolchainVersions,

    /// ISO 8601 timestamp when manifest was last updated
    #[serde(rename = "updatedAt")]
    pub updated_at: String,

    /// Cache entries per plugin
    pub plugins: HashMap<String, CodePluginCache>,
}

impl CodeCacheManifest {
    /// Create a new cache manifest
    pub fn new(target: String, toolchain: ToolchainVersions) -> Self {
        Self {
            version: "2.0".to_string(),
            target,
            toolchain,
            updated_at: current_timestamp(),
            plugins: HashMap::new(),
        }
    }

    /// Add or update a plugin cache entry
    pub fn add_plugin(
        &mut self,
        plugin_name: String,
        ir_hash: String,
        file_hashes: HashMap<String, String>,
    ) {
        self.plugins.insert(
            plugin_name,
            CodePluginCache {
                ir_hash,
                file_hashes,
                cached_at: current_timestamp(),
            },
        );
        self.updated_at = current_timestamp();
    }
}

/// Get current ISO 8601 timestamp
fn current_timestamp() -> String {
    use std::time::SystemTime;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Time went backwards");

    // Format as ISO 8601: YYYY-MM-DDTHH:MM:SSZ
    let secs = now.as_secs();
    let datetime = time_to_iso8601(secs);
    datetime
}

/// Convert Unix timestamp to ISO 8601 format
fn time_to_iso8601(secs: u64) -> String {
    const SECS_PER_DAY: u64 = 86400;
    const SECS_PER_HOUR: u64 = 3600;
    const SECS_PER_MIN: u64 = 60;

    // Days since Unix epoch
    let days = secs / SECS_PER_DAY;
    let remaining = secs % SECS_PER_DAY;

    // Calculate date (simple approximation - good enough for cache timestamps)
    let year = 1970 + (days / 365);
    let day_of_year = days % 365;
    let month = 1 + (day_of_year / 30);
    let day = 1 + (day_of_year % 30);

    // Calculate time
    let hours = remaining / SECS_PER_HOUR;
    let remaining = remaining % SECS_PER_HOUR;
    let minutes = remaining / SECS_PER_MIN;
    let seconds = remaining % SECS_PER_MIN;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Get cache directory path: ~/.cache/plexus-codegen/hub-codegen/{target}/{backend}
pub fn get_cache_dir(target: &str, backend: &str) -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| anyhow::anyhow!("Cannot determine home directory"))?;

    let cache_dir = PathBuf::from(home)
        .join(".cache")
        .join("plexus-codegen")
        .join("hub-codegen")
        .join(target)
        .join(backend);

    Ok(cache_dir)
}

/// Read cache manifest from disk
pub fn read_cache_manifest(target: &str, backend: &str) -> Result<CodeCacheManifest> {
    let cache_dir = get_cache_dir(target, backend)?;
    let manifest_path = cache_dir.join("manifest.json");

    if !manifest_path.exists() {
        anyhow::bail!("Cache manifest not found at {}", manifest_path.display());
    }

    let content = fs::read_to_string(&manifest_path)?;
    let manifest: CodeCacheManifest = serde_json::from_str(&content)?;

    Ok(manifest)
}

/// Write cache manifest to disk
pub fn write_cache_manifest(
    target: &str,
    backend: &str,
    manifest: &CodeCacheManifest,
) -> Result<()> {
    let cache_dir = get_cache_dir(target, backend)?;
    fs::create_dir_all(&cache_dir)?;

    let manifest_path = cache_dir.join("manifest.json");
    let content = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, content)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_manifest() {
        let toolchain = ToolchainVersions {
            synapse_cc: "0.1.0.0".to_string(),
            synapse: "0.2.0.0".to_string(),
            hub_codegen: "0.1.0".to_string(),
        };

        let manifest = CodeCacheManifest::new("typescript".to_string(), toolchain);

        assert_eq!(manifest.version, "2.0");
        assert_eq!(manifest.target, "typescript");
        assert_eq!(manifest.plugins.len(), 0);
    }

    #[test]
    fn test_add_plugin() {
        let toolchain = ToolchainVersions {
            synapse_cc: "0.1.0.0".to_string(),
            synapse: "0.2.0.0".to_string(),
            hub_codegen: "0.1.0".to_string(),
        };

        let mut manifest = CodeCacheManifest::new("typescript".to_string(), toolchain);

        let mut file_hashes = HashMap::new();
        file_hashes.insert("types.ts".to_string(), "abc123".to_string());
        file_hashes.insert("methods.ts".to_string(), "def456".to_string());

        manifest.add_plugin("cone".to_string(), "ir_hash_123".to_string(), file_hashes);

        assert_eq!(manifest.plugins.len(), 1);
        assert!(manifest.plugins.contains_key("cone"));

        let plugin = &manifest.plugins["cone"];
        assert_eq!(plugin.ir_hash, "ir_hash_123");
        assert_eq!(plugin.file_hashes.len(), 2);
        assert_eq!(plugin.file_hashes["types.ts"], "abc123");
    }

    #[test]
    fn test_serialization() {
        let toolchain = ToolchainVersions {
            synapse_cc: "0.1.0.0".to_string(),
            synapse: "0.2.0.0".to_string(),
            hub_codegen: "0.1.0".to_string(),
        };

        let manifest = CodeCacheManifest::new("typescript".to_string(), toolchain);

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let deserialized: CodeCacheManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, "2.0");
        assert_eq!(deserialized.target, "typescript");
    }
}
