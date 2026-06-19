//! Heurística determinística para gerar descriptions de memórias ingeridas.
//!
//! GAP-E2E-011 (FALTA-6): toda memória ingerida recebia description
//! hardcoded `"ingested from <path>"`, o que tornava a listagem inútil
//! e empobrecia o resultado de buscas. Esta heurística pure-Rust extrai
//! a primeira linha significativa do body, ignorando headers markdown.
//!
//! Regras:
//! - Primeira linha não-vazia com mais de 20 caracteres
//! - Ignora linhas que começam com `#` (markdown headers)
//! - Trunca em 100 caracteres via `chars().take(100)`
//! - Fallback: `"ingested document"` quando nenhuma linha válida
//!
//! Determinismo: zero alocação baseada em ordem de hash, zero LLM,
//! zero dependência de ordem de filesystem. Saída reproduzível byte a byte.

/// Extrai uma description heurística do body de um documento ingerido.
///
/// Retorna a primeira linha significativa (não-vazia, >20 chars, não-header
/// markdown) truncada em 100 caracteres. Fallback determinístico contextual:
/// quando nenhuma linha atende os critérios, usa o stem (nome sem extensão)
/// do path, ou `"ingested document"` se o stem for vazio ou inválido.
///
/// FALTA-6 (v1.0.89): edge case de body só com headers Markdown agora gera
/// description útil ao operador em vez do placeholder genérico.
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

/// Extrai o stem (nome sem extensão) de um path, sanitizado.
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
