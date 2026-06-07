//! Executor version parsing (v1.0.75 — G22)

use crate::errors::AppError;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutorVersion {
    pub raw: String,
    pub semver: Option<Version>,
    pub channel: Option<String>,
}

impl ExecutorVersion {
    pub fn unknown() -> Self {
        Self {
            raw: "unknown".to_string(),
            semver: None,
            channel: None,
        }
    }

    pub fn parse(s: &str) -> Result<Self, AppError> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(AppError::Validation("empty version string".to_string()));
        }
        let channel_start = trimmed.find('-');
        let (numeric_part, channel_part) = match channel_start {
            Some(idx) => (
                trimmed[..idx].to_string(),
                Some(trimmed[idx + 1..].to_string()),
            ),
            None => (trimmed.to_string(), None),
        };
        let semver = if numeric_part
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            Version::from_str(&numeric_part).ok()
        } else {
            None
        };
        Ok(Self {
            raw: trimmed.to_string(),
            semver,
            channel: channel_part,
        })
    }

    pub fn major(&self) -> u64 {
        self.semver.as_ref().map(|v| v.major).unwrap_or(0)
    }

    pub fn minor(&self) -> u64 {
        self.semver.as_ref().map(|v| v.minor).unwrap_or(0)
    }

    pub fn patch(&self) -> u64 {
        self.semver.as_ref().map(|v| v.patch).unwrap_or(0)
    }

    pub fn is_at_least(&self, major: u64, minor: u64, patch: u64) -> bool {
        match &self.semver {
            Some(v) => (v.major, v.minor, v.patch) >= (major, minor, patch),
            None => false,
        }
    }

    pub fn in_range(&self, min: (u64, u64, u64), max: (u64, u64, u64)) -> bool {
        self.is_at_least(min.0, min.1, min.2) && {
            let current = (self.major(), self.minor(), self.patch());
            current <= max
        }
    }

    pub fn compare(&self, other: &ExecutorVersion) -> Ordering {
        match (&self.semver, &other.semver) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => Ordering::Equal,
        }
    }
}

impl std::fmt::Display for ExecutorVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw)
    }
}

impl Default for ExecutorVersion {
    fn default() -> Self {
        Self::unknown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_semver() {
        let v = ExecutorVersion::parse("0.137.0").unwrap();
        assert_eq!(v.major(), 0);
        assert_eq!(v.minor(), 137);
        assert_eq!(v.patch(), 0);
        assert!(v.is_at_least(0, 130, 0));
        assert!(!v.is_at_least(1, 0, 0));
    }

    #[test]
    fn parse_with_channel() {
        let v = ExecutorVersion::parse("2.1.0-beta.1").unwrap();
        assert_eq!(v.major(), 2);
        assert_eq!(v.minor(), 1);
        assert_eq!(v.channel.as_deref(), Some("beta.1"));
    }

    #[test]
    fn parse_unknown() {
        let v = ExecutorVersion::parse("n/a").unwrap();
        assert_eq!(v.semver, None);
        assert!(!v.is_at_least(0, 0, 0));
    }

    #[test]
    fn compare_ordering() {
        let a = ExecutorVersion::parse("0.137.0").unwrap();
        let b = ExecutorVersion::parse("0.138.0").unwrap();
        assert_eq!(a.compare(&b), Ordering::Less);
    }

    #[test]
    fn empty_string_is_error() {
        assert!(ExecutorVersion::parse("").is_err());
    }
}
