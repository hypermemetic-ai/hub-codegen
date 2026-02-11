use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cache::CodeCacheManifest;
use crate::hash::compute_file_hash;

/// Status of a file in three-way comparison (cache vs current vs new)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    /// File unchanged: cache == current == new
    Unchanged,
    /// Safe to update: cache == current, but new is different
    SafeToUpdate,
    /// User modified: cache != current (conflict!)
    UserModified,
    /// New file not in cache
    NewFile,
}

/// Strategy for handling merge conflicts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    /// Skip modified files (safe default)
    Skip,
    /// Force overwrite everything
    Force,
    /// Interactive prompts (not yet implemented)
    Interactive,
}

impl std::str::FromStr for MergeStrategy {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "skip" => Ok(MergeStrategy::Skip),
            "force" => Ok(MergeStrategy::Force),
            "interactive" => Ok(MergeStrategy::Interactive),
            _ => anyhow::bail!("Invalid merge strategy: {}. Valid options: skip, force, interactive", s),
        }
    }
}

impl std::fmt::Display for MergeStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MergeStrategy::Skip => write!(f, "skip"),
            MergeStrategy::Force => write!(f, "force"),
            MergeStrategy::Interactive => write!(f, "interactive"),
        }
    }
}

/// Result of a merge operation
#[derive(Debug)]
pub struct MergeResult {
    /// Files that were updated
    pub updated: Vec<PathBuf>,
    /// Files that were skipped due to user modifications
    pub skipped: Vec<PathBuf>,
    /// Files that were unchanged
    pub unchanged: Vec<PathBuf>,
    /// New files that were added
    pub new: Vec<PathBuf>,
}

/// Determine file status from three-way hash comparison
fn determine_file_status(
    cached_hash: Option<&str>,
    current_hash: Option<&str>,
    new_hash: &str,
) -> FileStatus {
    match (cached_hash, current_hash) {
        // File not in cache
        (None, None) => FileStatus::NewFile,
        (None, Some(current)) => {
            // Not in cache but exists on disk
            if current == new_hash {
                FileStatus::Unchanged
            } else {
                FileStatus::NewFile
            }
        }
        // File in cache but deleted from disk
        (Some(_cached), None) => FileStatus::SafeToUpdate, // Recreate it
        // File in cache and on disk
        (Some(cached), Some(current)) => {
            if cached == current {
                // User hasn't modified it
                if current == new_hash {
                    FileStatus::Unchanged
                } else {
                    FileStatus::SafeToUpdate
                }
            } else {
                // User has modified it - conflict!
                FileStatus::UserModified
            }
        }
    }
}

/// Perform three-way merge: staging vs output vs cache
pub fn merge_generated_code(
    staging_files: &HashMap<String, String>,
    output_dir: &Path,
    cache_manifest: Option<&CodeCacheManifest>,
    strategy: MergeStrategy,
) -> Result<MergeResult> {
    let mut updated = Vec::new();
    let mut skipped = Vec::new();
    let mut unchanged = Vec::new();
    let mut new = Vec::new();

    for (rel_path, new_content) in staging_files {
        let output_path = output_dir.join(rel_path);
        let new_hash = compute_file_hash(new_content);

        // Get current file hash if it exists
        let current_hash = if output_path.exists() {
            let current_content = fs::read_to_string(&output_path)?;
            Some(compute_file_hash(&current_content))
        } else {
            None
        };

        // Get cached hash from manifest
        let cached_hash = cache_manifest
            .and_then(|m| m.plugins.values().find_map(|p| p.file_hashes.get(rel_path)))
            .map(|s| s.as_str());

        // Determine file status
        let status = determine_file_status(cached_hash, current_hash.as_deref(), &new_hash);

        // Apply merge strategy
        match status {
            FileStatus::Unchanged => {
                unchanged.push(PathBuf::from(rel_path));
            }
            FileStatus::SafeToUpdate => {
                // Safe to update - write the file
                if let Some(parent) = output_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&output_path, new_content)?;
                updated.push(PathBuf::from(rel_path));
            }
            FileStatus::NewFile => {
                // New file - write it
                if let Some(parent) = output_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&output_path, new_content)?;
                new.push(PathBuf::from(rel_path));
            }
            FileStatus::UserModified => {
                // Conflict! User has modified the file
                match strategy {
                    MergeStrategy::Skip => {
                        // Skip this file
                        skipped.push(PathBuf::from(rel_path));
                    }
                    MergeStrategy::Force => {
                        // Overwrite anyway
                        if let Some(parent) = output_path.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        fs::write(&output_path, new_content)?;
                        updated.push(PathBuf::from(rel_path));
                    }
                    MergeStrategy::Interactive => {
                        anyhow::bail!("Interactive merge strategy not yet implemented");
                    }
                }
            }
        }
    }

    Ok(MergeResult {
        updated,
        skipped,
        unchanged,
        new,
    })
}

/// Print merge result summary with warnings for conflicts
pub fn print_merge_summary(result: &MergeResult) {
    let total = result.updated.len() + result.skipped.len() + result.unchanged.len() + result.new.len();

    println!("\nMerge Summary:");
    println!("  Updated:   {} files", result.updated.len());
    println!("  New:       {} files", result.new.len());
    println!("  Unchanged: {} files", result.unchanged.len());

    if !result.skipped.is_empty() {
        eprintln!("\n  WARNING: The following files have been modified and were NOT updated:");
        for file in &result.skipped {
            eprintln!("    {}", file.display());
        }
        eprintln!("\n  These files were skipped to preserve your changes.");
        eprintln!("  To overwrite them, use: --merge-strategy force");
    }

    println!("\nTotal: {} files", total);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_file_status_unchanged() {
        let hash = "abc123";
        let status = determine_file_status(Some(hash), Some(hash), hash);
        assert_eq!(status, FileStatus::Unchanged);
    }

    #[test]
    fn test_determine_file_status_safe_to_update() {
        let cached = "abc123";
        let current = "abc123";
        let new = "def456";
        let status = determine_file_status(Some(cached), Some(current), new);
        assert_eq!(status, FileStatus::SafeToUpdate);
    }

    #[test]
    fn test_determine_file_status_user_modified() {
        let cached = "abc123";
        let current = "modified";
        let new = "def456";
        let status = determine_file_status(Some(cached), Some(current), new);
        assert_eq!(status, FileStatus::UserModified);
    }

    #[test]
    fn test_determine_file_status_new_file() {
        let new = "abc123";
        let status = determine_file_status(None, None, new);
        assert_eq!(status, FileStatus::NewFile);
    }

    #[test]
    fn test_merge_strategy_from_str() {
        assert_eq!("skip".parse::<MergeStrategy>().unwrap(), MergeStrategy::Skip);
        assert_eq!("force".parse::<MergeStrategy>().unwrap(), MergeStrategy::Force);
        assert_eq!("interactive".parse::<MergeStrategy>().unwrap(), MergeStrategy::Interactive);
        assert!("invalid".parse::<MergeStrategy>().is_err());
    }
}
