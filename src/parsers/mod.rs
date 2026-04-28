//! Input format parsers (Markdown, YAML, plain text, timestamp).

use chrono::DateTime;

/// Aceita Unix epoch (inteiro >= 0) ou RFC 3339 e retorna Unix epoch.
pub fn parse_expected_updated_at(s: &str) -> Result<i64, String> {
    if let Ok(secs) = s.parse::<i64>() {
        if secs >= 0 {
            return Ok(secs);
        }
    }
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.timestamp())
        .map_err(|e| {
            format!(
                "valor deve ser Unix epoch (inteiro >= 0) ou RFC 3339 (ex: 2026-04-19T12:00:00Z): {e}"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aceita_unix_epoch() {
        assert_eq!(parse_expected_updated_at("1700000000").unwrap(), 1700000000);
    }

    #[test]
    fn aceita_zero() {
        assert_eq!(parse_expected_updated_at("0").unwrap(), 0);
    }

    #[test]
    fn aceita_rfc_3339_utc() {
        let resultado = parse_expected_updated_at("2020-01-01T00:00:00Z");
        assert!(resultado.is_ok());
        assert_eq!(resultado.unwrap(), 1577836800);
    }

    #[test]
    fn aceita_rfc_3339_com_offset() {
        let resultado = parse_expected_updated_at("2026-04-19T12:00:00+00:00");
        assert!(resultado.is_ok());
    }

    #[test]
    fn rejeita_string_invalida() {
        assert!(parse_expected_updated_at("bananas").is_err());
    }

    #[test]
    fn rejeita_negativo() {
        let erro = parse_expected_updated_at("-1");
        assert!(erro.is_err());
    }

    #[test]
    fn mensagem_de_erro_menciona_formato() {
        let msg = parse_expected_updated_at("invalido").unwrap_err();
        assert!(msg.contains("RFC 3339") || msg.contains("Unix epoch"));
    }
}
