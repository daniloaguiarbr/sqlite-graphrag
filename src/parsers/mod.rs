//! Input format parsers (timestamp, range validators).

use chrono::DateTime;

/// Accepts a Unix epoch (integer >= 0) or RFC 3339 timestamp and returns the Unix epoch.
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
                "value must be a Unix epoch (integer >= 0) or RFC 3339 (e.g. 2026-04-19T12:00:00Z): {e}"
            )
        })
}

/// Validates `-k`/`--k` for `recall` and `hybrid-search` to the inclusive range `1..=4096`.
///
/// The upper bound matches the `sqlite-vec` knn limit; values above it would surface a leaky
/// engine error such as `k value in knn query too large, provided 10000 and the limit is 4096`.
/// Validating at parse time turns the failure into a clean Clap error before any database work.
pub fn parse_k_range(s: &str) -> Result<usize, String> {
    let value: usize = s
        .parse()
        .map_err(|_| format!("'{s}' is not a valid non-negative integer"))?;
    if !(1..=4096).contains(&value) {
        return Err(format!(
            "k must be between 1 and 4096 (inclusive); got {value}"
        ));
    }
    Ok(value)
}

/// Flexible boolean parser for Clap env var integration.
///
/// Accepts common truthy/falsy conventions used in shell environments:
/// truthy: `1`, `true`, `yes`, `on` (case-insensitive)
/// falsy: `0`, `false`, `no`, `off`, empty string (case-insensitive)
pub fn parse_bool_flexible(s: &str) -> Result<bool, String> {
    match s.to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" | "" => Ok(false),
        _ => Err(format!(
            "invalid boolean value '{s}': expected true/false/1/0/yes/no/on/off"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_unix_epoch() {
        assert_eq!(parse_expected_updated_at("1700000000").unwrap(), 1700000000);
    }

    #[test]
    fn accepts_zero() {
        assert_eq!(parse_expected_updated_at("0").unwrap(), 0);
    }

    #[test]
    fn accepts_rfc_3339_utc() {
        let result = parse_expected_updated_at("2020-01-01T00:00:00Z");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1577836800);
    }

    #[test]
    fn accepts_rfc_3339_with_offset() {
        let result = parse_expected_updated_at("2026-04-19T12:00:00+00:00");
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_invalid_string() {
        assert!(parse_expected_updated_at("bananas").is_err());
    }

    #[test]
    fn rejects_negative() {
        let err = parse_expected_updated_at("-1");
        assert!(err.is_err());
    }

    #[test]
    fn error_message_mentions_format() {
        let msg = parse_expected_updated_at("invalid").unwrap_err();
        assert!(msg.contains("RFC 3339") || msg.contains("Unix epoch"));
    }

    #[test]
    fn k_accepts_valid_range_endpoints() {
        assert_eq!(parse_k_range("1").unwrap(), 1);
        assert_eq!(parse_k_range("4096").unwrap(), 4096);
        assert_eq!(parse_k_range("10").unwrap(), 10);
    }

    #[test]
    fn k_rejects_zero() {
        let msg = parse_k_range("0").unwrap_err();
        assert!(msg.contains("between 1 and 4096"));
    }

    #[test]
    fn k_rejects_above_limit() {
        let msg = parse_k_range("10000").unwrap_err();
        assert!(msg.contains("between 1 and 4096"));
    }

    #[test]
    fn k_rejects_non_integer() {
        let msg = parse_k_range("abc").unwrap_err();
        assert!(msg.contains("not a valid"));
    }

    #[test]
    fn k_rejects_negative() {
        // usize parser fails on negatives before range check
        assert!(parse_k_range("-5").is_err());
    }

    #[test]
    fn bool_flexible_truthy() {
        for v in &["1", "true", "True", "TRUE", "yes", "Yes", "on", "ON"] {
            assert!(parse_bool_flexible(v).unwrap(), "should be true: {v}");
        }
    }

    #[test]
    fn bool_flexible_falsy() {
        for v in &["0", "false", "False", "FALSE", "no", "No", "off", "OFF", ""] {
            assert!(!parse_bool_flexible(v).unwrap(), "should be false: {v}");
        }
    }

    #[test]
    fn bool_flexible_rejects_invalid() {
        assert!(parse_bool_flexible("banana").is_err());
        assert!(parse_bool_flexible("2").is_err());
        assert!(parse_bool_flexible("nope").is_err());
    }
}

/// The 12 well-known relation types from v1.0.0.
///
/// Non-canonical relations are accepted but emit a `tracing::warn!`.
pub const CANONICAL_RELATIONS: &[&str] = &[
    "applies_to",
    "uses",
    "depends_on",
    "causes",
    "fixes",
    "contradicts",
    "supports",
    "follows",
    "related",
    "mentions",
    "replaces",
    "tracked_in",
];

/// Returns `true` when the relation is one of the 12 canonical types.
pub fn is_canonical_relation(s: &str) -> bool {
    CANONICAL_RELATIONS.contains(&s)
}

/// Normalizes a relation string: lowercase + hyphens to underscores.
pub fn normalize_relation(s: &str) -> String {
    s.to_lowercase().replace('-', "_")
}

/// Validates that a normalized relation matches `^[a-z][a-z0-9_]*$`.
pub fn validate_relation_format(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("relation must not be empty".to_string());
    }
    if !s.as_bytes()[0].is_ascii_lowercase() {
        return Err(format!(
            "relation must start with a lowercase letter, got '{s}'"
        ));
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(format!(
            "relation must contain only lowercase letters, digits and underscores, got '{s}'"
        ));
    }
    Ok(())
}

/// Emits a `tracing::warn!` when the relation is not in [`CANONICAL_RELATIONS`].
pub fn warn_if_non_canonical(relation: &str) {
    if !is_canonical_relation(relation) {
        tracing::warn!(
            relation,
            "non-canonical relation accepted; consider using a well-known value"
        );
    }
}

/// Clap `value_parser` for `--relation`: normalizes and validates format.
///
/// Accepts any kebab-case or snake_case string. Non-canonical values are
/// accepted at parse time; the warning is emitted at command execution.
pub fn parse_relation(s: &str) -> Result<String, String> {
    let normalized = normalize_relation(s);
    validate_relation_format(&normalized)?;
    Ok(normalized)
}

#[cfg(test)]
mod relation_tests {
    use super::*;

    #[test]
    fn canonical_relations_all_valid() {
        for r in CANONICAL_RELATIONS {
            assert!(
                validate_relation_format(r).is_ok(),
                "canonical relation '{r}' should be valid"
            );
        }
    }

    #[test]
    fn normalize_converts_hyphens_and_uppercase() {
        assert_eq!(normalize_relation("Depends-On"), "depends_on");
        assert_eq!(normalize_relation("TESTED-BY"), "tested_by");
        assert_eq!(normalize_relation("uses"), "uses");
    }

    #[test]
    fn validate_rejects_empty() {
        assert!(validate_relation_format("").is_err());
    }

    #[test]
    fn validate_rejects_digit_start() {
        assert!(validate_relation_format("123abc").is_err());
    }

    #[test]
    fn validate_rejects_spaces() {
        assert!(validate_relation_format("has spaces").is_err());
    }

    #[test]
    fn validate_accepts_custom_relations() {
        assert!(validate_relation_format("implements").is_ok());
        assert!(validate_relation_format("tested_by").is_ok());
        assert!(validate_relation_format("part_of").is_ok());
        assert!(validate_relation_format("blocks").is_ok());
    }

    #[test]
    fn parse_relation_normalizes_and_validates() {
        assert_eq!(parse_relation("Tested-By").unwrap(), "tested_by");
        assert_eq!(parse_relation("uses").unwrap(), "uses");
        assert!(parse_relation("").is_err());
    }

    #[test]
    fn is_canonical_detects_known() {
        assert!(is_canonical_relation("uses"));
        assert!(is_canonical_relation("applies_to"));
        assert!(!is_canonical_relation("implements"));
        assert!(!is_canonical_relation("blocks"));
    }
}
