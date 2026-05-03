//! Canonical entity type taxonomy used across extraction, storage and CLI.
//!
//! `EntityType` is the single source of truth for the 13 graph entity kinds.
//! It derives `clap::ValueEnum` so CLI flags can use it directly, and derives
//! `serde::{Serialize, Deserialize}` with `rename_all = "lowercase"` so JSON
//! round-trips remain backward-compatible with the pre-enum string format.

use crate::errors::AppError;

/// The 13 canonical graph entity classifications.
///
/// Values are serialized as lowercase strings (`"person"`, `"organization"`,
/// etc.) matching the pre-enum wire format and the SQLite `type` column.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, clap::ValueEnum,
)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
pub enum EntityType {
    Concept,
    Date,
    Dashboard,
    Decision,
    File,
    Incident,
    IssueTracker,
    Location,
    Memory,
    Organization,
    Person,
    Project,
    Tool,
}

impl EntityType {
    /// Returns the canonical lowercase string representation stored in SQLite.
    pub fn as_str(self) -> &'static str {
        match self {
            EntityType::Concept => "concept",
            EntityType::Date => "date",
            EntityType::Dashboard => "dashboard",
            EntityType::Decision => "decision",
            EntityType::File => "file",
            EntityType::Incident => "incident",
            EntityType::IssueTracker => "issue_tracker",
            EntityType::Location => "location",
            EntityType::Memory => "memory",
            EntityType::Organization => "organization",
            EntityType::Person => "person",
            EntityType::Project => "project",
            EntityType::Tool => "tool",
        }
    }
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for EntityType {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "concept" => Ok(EntityType::Concept),
            "date" => Ok(EntityType::Date),
            "dashboard" => Ok(EntityType::Dashboard),
            "decision" => Ok(EntityType::Decision),
            "file" => Ok(EntityType::File),
            "incident" => Ok(EntityType::Incident),
            "issue_tracker" => Ok(EntityType::IssueTracker),
            "location" => Ok(EntityType::Location),
            "memory" => Ok(EntityType::Memory),
            "organization" => Ok(EntityType::Organization),
            "person" => Ok(EntityType::Person),
            "project" => Ok(EntityType::Project),
            "tool" => Ok(EntityType::Tool),
            other => Err(AppError::Validation(format!(
                "invalid entity type: {other}; expected one of: concept, date, dashboard, decision, file, incident, issue_tracker, location, memory, organization, person, project, tool"
            ))),
        }
    }
}

impl rusqlite::types::FromSql for EntityType {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let s = String::column_result(value)?;
        s.parse::<EntityType>().map_err(|e| {
            rusqlite::types::FromSqlError::Other(Box::new(std::io::Error::other(e.to_string())))
        })
    }
}

impl rusqlite::types::ToSql for EntityType {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_lowercase_roundtrip() {
        assert_eq!("person".parse::<EntityType>().unwrap(), EntityType::Person);
        assert_eq!(
            "organization".parse::<EntityType>().unwrap(),
            EntityType::Organization
        );
        assert_eq!(
            "issue_tracker".parse::<EntityType>().unwrap(),
            EntityType::IssueTracker
        );
    }

    #[test]
    fn from_str_uppercase_is_case_insensitive() {
        assert_eq!("PERSON".parse::<EntityType>().unwrap(), EntityType::Person);
        assert_eq!(
            "Organization".parse::<EntityType>().unwrap(),
            EntityType::Organization
        );
    }

    #[test]
    fn from_str_invalid_returns_err() {
        let result = "invalid".parse::<EntityType>();
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("invalid entity type"));
    }

    #[test]
    fn as_str_returns_canonical_lowercase() {
        assert_eq!(EntityType::Person.as_str(), "person");
        assert_eq!(EntityType::IssueTracker.as_str(), "issue_tracker");
    }

    #[test]
    fn serde_json_serializes_as_lowercase_string() {
        let json = serde_json::to_string(&EntityType::Person).unwrap();
        assert_eq!(json, "\"person\"");
        let json = serde_json::to_string(&EntityType::IssueTracker).unwrap();
        assert_eq!(json, "\"issue_tracker\"");
    }

    #[test]
    fn serde_json_deserializes_from_lowercase_string() {
        let et: EntityType = serde_json::from_str("\"person\"").unwrap();
        assert_eq!(et, EntityType::Person);
    }
}
