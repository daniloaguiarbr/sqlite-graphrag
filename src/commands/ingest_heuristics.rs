//! Deterministic heuristic for generating descriptions of ingested memories.
//!
//! GAP-E2E-011 (FALTA-6): every ingested memory received the hardcoded
//! description `"ingested from <path>"`, which made the listing useless
//! and degraded search results. This pure-Rust heuristic extracts
//! the first meaningful line of the body, ignoring markdown headers.
//!
//! Rules:
//! - First non-empty line longer than 20 characters
//! - Ignores lines starting with `#` (markdown headers)
//! - Truncates at 100 characters via `chars().take(100)`
//! - Fallback: `"ingested document"` when no line is valid
//!
//! Determinism: zero hash-order-based allocation, zero LLM,
//! zero dependency on filesystem order. Byte-for-byte reproducible output.

/// Extracts a heuristic description from the body of an ingested document.
///
/// Returns the first meaningful line (non-empty, >20 chars, not a markdown
/// header) truncated at 100 characters. Contextual deterministic fallback:
/// when no line meets the criteria, uses the path stem (name without extension),
/// or `"ingested document"` if the stem is empty or invalid.
///
/// FALTA-6 (v1.0.89): the edge case of a body with only Markdown headers now
/// generates a description useful to the operator instead of the generic placeholder.
pub fn extract_heuristic_description(body: &str, path_hint: Option<&str>) -> String {
    let from_body = body
        .lines()
        .map(str::trim)
        .find(|line| line.len() > 20 && !line.starts_with('#'))
        .map(|line| line.chars().take(100).collect::<String>());
    if let Some(desc) = from_body {
        return desc;
    }
    // Fallback contextual: usar stem do path quando heurística do body falhar.
    if let Some(stem) = path_hint.and_then(derive_stem) {
        return stem;
    }
    "ingested document".to_string()
}

/// Extracts the stem (name without extension) from a path, sanitized.
fn derive_stem(path: &str) -> Option<String> {
    let basename = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .trim();
    if basename.is_empty() || basename.len() < 2 {
        return None;
    }
    Some(basename.chars().take(100).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_first_meaningful_line() {
        let body = "\
# Title

This is the actual first sentence of the document that has more than twenty characters.
Second line should be ignored.
";
        let desc = extract_heuristic_description(body, Some("/tmp/spec.md"));
        assert!(
            desc.starts_with("This is the actual"),
            "desc deve começar com a primeira linha útil, got: {desc}"
        );
    }

    #[test]
    fn falls_back_to_stem_when_only_headers() {
        // FALTA-6: documento markdown com apenas headers (sem texto > 20 chars).
        let body = "\
# Header 1
## Header 2
### Header 3
";
        let desc = extract_heuristic_description(body, Some("/tmp/headers-only.md"));
        assert_eq!(desc, "headers-only");
    }

    #[test]
    fn falls_back_to_ingested_document_when_no_path() {
        let body = "# Only Header";
        let desc = extract_heuristic_description(body, None);
        assert_eq!(desc, "ingested document");
    }

    #[test]
    fn truncates_at_100_chars() {
        let long = "a".repeat(200);
        let desc = extract_heuristic_description(&long, None);
        assert!(
            desc.chars().count() <= 100,
            "desc deve ter no máximo 100 chars, got: {}",
            desc.chars().count()
        );
    }

    #[test]
    fn back_compat_single_arg_returns_body_only() {
        // Confirma o caminho simplificado (sem path_hint) ainda funciona.
        let body = "\
# H

First sentence that has more than twenty characters of useful text.
";
        let desc = extract_heuristic_description(body, None);
        assert!(desc.starts_with("First sentence"));
    }
}
