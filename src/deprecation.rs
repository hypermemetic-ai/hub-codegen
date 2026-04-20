//! Deprecation annotation support (IR-7).
//!
//! When the IR contains deprecated surfaces (methods, types, params, or
//! plugins) carrying `DeprecationInfo`, hub-codegen emits target-language
//! annotations above the generated surface. This module exposes the shared
//! helpers used by both the TypeScript and Rust backends.
//!
//! # Version detection
//!
//! An IR is considered "post-IR" (deprecation-aware) if any of:
//!
//! - Any `MethodDef.md_deprecation` is `Some`.
//! - Any `TypeDef.td_deprecation` is `Some`.
//! - Any `ParamDef.pd_deprecation` is `Some`.
//! - Any `FieldDef.fd_deprecation` is `Some`.
//! - The `ir_plugin_deprecations` map is non-empty.
//! - `ir.ir_version` parses to `>= 0.5`.
//!
//! Pre-IR IRs (no deprecation fields, version < 0.5) skip annotation
//! emission entirely and produce byte-identical output to pre-ticket.
//!
//! # Body format (pinned across backends)
//!
//! All annotations share the body format `since <X>, removed in <Y>: <message>`.
//! Only the comment leader differs per language (TypeScript / JSDoc / Rust).

use crate::ir::{DeprecationInfo, TypeKind, IR};

/// Runtime toggles for deprecation emission. Shared across TS + Rust backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeprecationOptions {
    /// When `false`, codegen behaves as if the IR were pre-IR — no
    /// annotations, no warnings. Used to implement `--no-deprecation-annotations`.
    pub enabled: bool,
}

impl Default for DeprecationOptions {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Record of a deprecated surface that the generator emitted code for.
/// The runner prints one stderr line per record in the format:
///
/// ```text
/// WARNING: generated code consumes deprecated <kind>:<name> at <file> — <message>
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeprecationWarning {
    /// Kind of surface: `"method"`, `"type"`, `"param"`, `"field"`, `"plugin"`,
    /// or one of the schema-field kinds (`"children"`, `"is_hub"`,
    /// `"child_capabilities"`).
    pub kind: String,
    /// Fully qualified surface name (e.g. `"echo.old_ping"`).
    pub name: String,
    /// Relative path in the generated output where the annotation lives.
    pub file: String,
    /// Human-readable migration guidance copied from the schema.
    pub message: String,
    /// Version at which deprecation began.
    pub since: String,
    /// Planned removal version.
    pub removed_in: String,
}

impl DeprecationWarning {
    /// Format as a single stderr line per the ticket's "WARNING: ..." contract.
    pub fn format_stderr(&self) -> String {
        format!(
            "WARNING: generated code consumes deprecated {} '{}' at {} — since {}, removed in {}: {}",
            self.kind, self.name, self.file, self.since, self.removed_in, self.message
        )
    }
}

/// Decide whether the IR carries any deprecation information — i.e. whether
/// annotations + warnings should be emitted.
///
/// Returns `true` when:
/// - any method, type, param, or field has a `Some(_)` deprecation, OR
/// - `ir_plugin_deprecations` is non-empty, OR
/// - `ir_version` parses to `>= 0.5`.
pub fn is_post_ir(ir: &IR) -> bool {
    if !ir.ir_plugin_deprecations.is_empty() {
        return true;
    }
    for method in ir.ir_methods.values() {
        if method.md_deprecation.is_some() {
            return true;
        }
        for param in &method.md_params {
            if param.pd_deprecation.is_some() {
                return true;
            }
        }
    }
    for typedef in ir.ir_types.values() {
        if typedef.td_deprecation.is_some() {
            return true;
        }
        match &typedef.td_kind {
            TypeKind::KindStruct { ks_fields } => {
                for f in ks_fields {
                    if f.fd_deprecation.is_some() {
                        return true;
                    }
                }
            }
            TypeKind::KindEnum { ke_variants, .. } => {
                for v in ke_variants {
                    for f in &v.vd_fields {
                        if f.fd_deprecation.is_some() {
                            return true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // Explicit version pin: treat ir_version >= 0.5 as post-IR.
    parse_minor(&ir.ir_version).map_or(false, |(major, minor)| major > 0 || minor >= 5)
}

/// Parse an `ir_version` string like `"2.0"` or `"0.5"` into `(major, minor)`.
/// Returns `None` when the string is not a simple `MAJOR.MINOR[.PATCH]`.
fn parse_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    Some((major, minor))
}

/// Format the pinned body: `since <X>, removed in <Y>: <message>`.
pub fn format_body(info: &DeprecationInfo) -> String {
    format!(
        "since {}, removed in {}: {}",
        info.since, info.removed_in, info.message
    )
}

/// Format a TypeScript annotation — one JSDoc line and one `// DEPRECATED`
/// line. Returned as a `Vec<String>` so callers may insert either leader
/// independently or join them.
pub fn format_ts(info: &DeprecationInfo) -> Vec<String> {
    let body = format_body(info);
    vec![
        format!("/** @deprecated {} */", body),
        format!("// DEPRECATED {}", body),
    ]
}

/// Format a Rust `#[deprecated(...)]` attribute.
pub fn format_rust(info: &DeprecationInfo) -> String {
    // Rust's `#[deprecated]` takes `since` and `note`. Include the removed_in
    // version in the note since Rust has no first-class slot for it.
    let note = format!("{} (removed in {})", info.message, info.removed_in);
    let escaped_note = note.replace('\\', "\\\\").replace('"', "\\\"");
    let escaped_since = info.since.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "#[deprecated(since = \"{}\", note = \"{}\")]",
        escaped_since, escaped_note
    )
}

/// Format a one-line comment for a TypeScript reference site (not the
/// JSDoc — used where a `/** */` block is not syntactically appropriate).
pub fn format_ts_inline_comment(info: &DeprecationInfo) -> String {
    format!("// DEPRECATED {}", format_body(info))
}

/// Format a one-line comment for a Rust reference site.
pub fn format_rust_inline_comment(info: &DeprecationInfo) -> String {
    format!("// DEPRECATED {}", format_body(info))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(since: &str, removed_in: &str, message: &str) -> DeprecationInfo {
        DeprecationInfo {
            since: since.into(),
            removed_in: removed_in.into(),
            message: message.into(),
        }
    }

    #[test]
    fn test_format_body_pinned() {
        let got = format_body(&info("0.5", "0.6", "use foo2"));
        assert_eq!(got, "since 0.5, removed in 0.6: use foo2");
    }

    #[test]
    fn test_format_ts_lines() {
        let lines = format_ts(&info("0.5", "0.6", "use foo2"));
        assert_eq!(lines[0], "/** @deprecated since 0.5, removed in 0.6: use foo2 */");
        assert_eq!(lines[1], "// DEPRECATED since 0.5, removed in 0.6: use foo2");
    }

    #[test]
    fn test_format_rust_attribute() {
        let got = format_rust(&info("0.5", "0.6", "use foo2"));
        assert_eq!(
            got,
            "#[deprecated(since = \"0.5\", note = \"use foo2 (removed in 0.6)\")]"
        );
    }

    #[test]
    fn test_format_rust_attribute_escapes_quotes() {
        let got = format_rust(&info("0.5", "0.6", "use \"new\" API"));
        assert!(got.contains("use \\\"new\\\" API"));
    }

    #[test]
    fn test_warning_format_stderr() {
        let w = DeprecationWarning {
            kind: "method".into(),
            name: "echo.old_ping".into(),
            file: "echo/client.ts".into(),
            message: "use foo2".into(),
            since: "0.5".into(),
            removed_in: "0.6".into(),
        };
        let s = w.format_stderr();
        assert!(s.contains("WARNING"));
        assert!(s.contains("echo.old_ping"));
        assert!(s.contains("0.5"));
        assert!(s.contains("0.6"));
        assert!(s.contains("use foo2"));
    }

    #[test]
    fn test_parse_minor() {
        assert_eq!(parse_minor("0.5"), Some((0, 5)));
        assert_eq!(parse_minor("2.0"), Some((2, 0)));
        assert_eq!(parse_minor("0.4"), Some((0, 4)));
        assert_eq!(parse_minor("invalid"), None);
    }
}
