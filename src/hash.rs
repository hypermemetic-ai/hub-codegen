use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Compute SHA-256 hash of content, returning first 16 hex characters
/// This matches Plexus hash format (16-char hex string)
pub fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    // Convert to hex and take first 16 characters (64 bits) like Plexus
    format!("{:x}", result)[..16].to_string()
}

/// Compute hash of IR fragment
/// Takes serialized IR content and returns deterministic hash
pub fn compute_ir_hash(ir_content: &str) -> String {
    compute_hash(ir_content)
}

/// Compute hash of a single generated file
pub fn compute_file_hash(file_content: &str) -> String {
    compute_hash(file_content)
}

/// Compute composite hash of all files in a plugin
/// Files are sorted by name for deterministic hashing
pub fn compute_plugin_hash(files: &HashMap<String, String>) -> String {
    let mut content = String::new();

    // Sort file names for deterministic ordering
    let mut file_names: Vec<_> = files.keys().collect();
    file_names.sort();

    // Hash each file with its path as prefix
    for name in file_names {
        let file_hash = compute_file_hash(&files[name]);
        content.push_str(&format!("{}:{}\n", name, file_hash));
    }

    compute_hash(&content)
}

/// Compute hash map of individual file hashes
/// Returns map of filename -> hash for each file
pub fn compute_file_hashes(files: &HashMap<String, String>) -> HashMap<String, String> {
    files
        .iter()
        .map(|(name, content)| (name.clone(), compute_file_hash(content)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash_deterministic() {
        let content = "hello world";
        let hash1 = compute_hash(content);
        let hash2 = compute_hash(content);
        assert_eq!(hash1, hash2, "Hash should be deterministic");
    }

    #[test]
    fn test_compute_hash_length() {
        let hash = compute_hash("test");
        assert_eq!(hash.len(), 16, "Hash should be 16 characters");
    }

    #[test]
    fn test_compute_hash_different_inputs() {
        let hash1 = compute_hash("foo");
        let hash2 = compute_hash("bar");
        assert_ne!(hash1, hash2, "Different inputs should produce different hashes");
    }

    #[test]
    fn test_compute_plugin_hash_sorted() {
        let mut files1 = HashMap::new();
        files1.insert("a.ts".to_string(), "content a".to_string());
        files1.insert("b.ts".to_string(), "content b".to_string());

        let mut files2 = HashMap::new();
        files2.insert("b.ts".to_string(), "content b".to_string());
        files2.insert("a.ts".to_string(), "content a".to_string());

        let hash1 = compute_plugin_hash(&files1);
        let hash2 = compute_plugin_hash(&files2);

        assert_eq!(
            hash1, hash2,
            "Plugin hash should be same regardless of insertion order"
        );
    }

    #[test]
    fn test_compute_file_hashes() {
        let mut files = HashMap::new();
        files.insert("types.ts".to_string(), "type Foo = string;".to_string());
        files.insert("methods.ts".to_string(), "export function bar() {}".to_string());

        let hashes = compute_file_hashes(&files);

        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains_key("types.ts"));
        assert!(hashes.contains_key("methods.ts"));
        assert_eq!(hashes["types.ts"].len(), 16);
        assert_eq!(hashes["methods.ts"].len(), 16);
    }
}
