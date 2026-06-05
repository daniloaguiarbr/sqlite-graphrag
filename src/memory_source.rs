//! Type-safe enumeration of the `memories.source` column domain.
//!
//! The CHECK constraint on the `memories` table accepts exactly five values:
//! `agent`, `user`, `system`, `import`, `sync`. Any other literal is rejected
//! at runtime by SQLite with `SQLITE_CONSTRAINT_CHECK`.
//!
//! This enum eliminates the silent footgun of `pub source: String` by forcing
//! every call-site to pick a typed variant that maps deterministically to one
//! of the five allowed CHECK values via [`MemorySource::as_str`].
//!
//! # Examples
//!
//! ```
//! use sqlite_graphrag::memory_source::MemorySource;
//!
//! let src = MemorySource::Agent;
//! assert_eq!(src.as_str(), "agent");
//!
//! let parsed = MemorySource::try_from("user").expect("user is valid");
//! assert_eq!(parsed, MemorySource::User);
//!
//! let err = MemorySource::try_from("enrich").unwrap_err();
//! assert!(format!("{err}").contains("invalid memory source"));
//! ```

use crate::errors::AppError;
use serde::{Deserialize, Serialize};

/// Enumerates the five values accepted by the `memories.source` CHECK constraint.
///
/// Adding a new variant requires:
///
/// 1. Updating the DDL CHECK constraint in `migrations/V001__init.sql`.
/// 2. Running a migration that backfills any pre-existing values
///    (`UPDATE memories SET source='agent' WHERE source NOT IN (...)`).
/// 3. Bumping [`crate::constants::CURRENT_SCHEMA_VERSION`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    /// Mutated by an LLM agent (remember, edit, rename, body-enrich).
    Agent,
    /// Mutated by a human operator.
    User,
    /// Mutated by an internal migration or system job.
    System,
    /// Inserted by bulk import (ingest, ingest --mode claude-code, ingest --mode codex).
    Import,
    /// Inserted by an external sync job.
    Sync,
}

impl MemorySource {
    /// Returns the canonical snake_case string stored in the SQLite column.
    ///
    /// The returned slice has `'static` lifetime because all five values are
    /// ASCII literals known at compile time.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::User => "user",
            Self::System => "system",
            Self::Import => "import",
            Self::Sync => "sync",
        }
    }

    /// Returns every variant as a static slice, useful for error messages and docs.
    pub const ALL: &'static [MemorySource] = &[
        Self::Agent,
        Self::User,
        Self::System,
        Self::Import,
        Self::Sync,
    ];
}

impl std::fmt::Display for MemorySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Parses a stored `memories.source` string back into a typed variant.
///
/// # Errors
///
/// Returns [`AppError::Validation`] when the input is not one of the five
/// canonical values. The error message lists every accepted value so the
/// caller can self-correct without consulting the schema.
impl TryFrom<&str> for MemorySource {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "agent" => Ok(Self::Agent),
            "user" => Ok(Self::User),
            "system" => Ok(Self::System),
            "import" => Ok(Self::Import),
            "sync" => Ok(Self::Sync),
            other => Err(AppError::Validation(format!(
                "invalid memory source: {other:?}; expected one of {}",
                Self::ALL
                    .iter()
                    .map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }
}

impl TryFrom<String> for MemorySource {
    type Error = AppError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

/// Validates a raw `memories.source` string against the CHECK constraint domain.
///
/// This is the runtime guard for callers that still take `&str` (legacy
/// call-sites, FTS rows already in the database, deserialised JSON). The
/// function returns the canonical slice on success and an [`AppError::Validation`]
/// on failure, with an actionable message listing every accepted value.
///
/// Use this at every boundary that touches the `source` column:
/// `memories::insert`, `memories::update`, and any new code path that
/// builds a `NewMemory` from operator-supplied input. It is the safety
/// net that prevented the original G29 bug from regressing in v1.0.69
/// when the typed [`MemorySource`] enum was still being rolled out.
pub fn validate_source(raw: &str) -> Result<&'static str, AppError> {
    match raw {
        "agent" => Ok("agent"),
        "user" => Ok("user"),
        "system" => Ok("system"),
        "import" => Ok("import"),
        "sync" => Ok("sync"),
        other => Err(AppError::Validation(format!(
            "invalid memory source: {other:?}; expected one of {}",
            MemorySource::ALL
                .iter()
                .map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_returns_canonical_lowercase() {
        assert_eq!(MemorySource::Agent.as_str(), "agent");
        assert_eq!(MemorySource::User.as_str(), "user");
        assert_eq!(MemorySource::System.as_str(), "system");
        assert_eq!(MemorySource::Import.as_str(), "import");
        assert_eq!(MemorySource::Sync.as_str(), "sync");
    }

    #[test]
    fn try_from_valid_strings_succeeds() {
        assert_eq!(
            MemorySource::try_from("agent").unwrap(),
            MemorySource::Agent
        );
        assert_eq!(MemorySource::try_from("user").unwrap(), MemorySource::User);
        assert_eq!(
            MemorySource::try_from("system").unwrap(),
            MemorySource::System
        );
        assert_eq!(
            MemorySource::try_from("import").unwrap(),
            MemorySource::Import
        );
        assert_eq!(MemorySource::try_from("sync").unwrap(), MemorySource::Sync);
    }

    #[test]
    fn try_from_invalid_string_returns_err() {
        // G29 reproducer: "enrich" is the historical bug.
        let err = MemorySource::try_from("enrich").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("invalid memory source"), "got: {msg}");
        assert!(msg.contains("\"enrich\""), "got: {msg}");
        assert!(msg.contains("agent"), "must list agent as valid: {msg}");
    }

    #[test]
    fn try_from_empty_string_returns_err() {
        assert!(MemorySource::try_from("").is_err());
    }

    #[test]
    fn try_from_string_owned_works() {
        let src: MemorySource = String::from("agent").try_into().unwrap();
        assert_eq!(src, MemorySource::Agent);
    }

    #[test]
    fn display_matches_as_str() {
        for v in MemorySource::ALL {
            assert_eq!(format!("{v}"), v.as_str());
        }
    }

    #[test]
    fn serialize_round_trip_preserves_variant() {
        let v = MemorySource::Import;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"import\"");
        let back: MemorySource = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn all_slice_has_exactly_five_variants() {
        assert_eq!(MemorySource::ALL.len(), 5);
    }
}
