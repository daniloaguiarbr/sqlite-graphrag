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
}
