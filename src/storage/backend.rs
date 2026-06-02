//! Storage backend abstraction layer (G14 — phase 1).
//!
//! Defines a trait that abstracts the database connection, enabling future
//! migration from rusqlite to libSQL embedded replicas or other backends.
//!
//! Phase 1 scope: trait definition + SqliteBackend wrapper only.
//! Phase 2 (v1.0.69+): migrate remaining 43 command handlers to use the trait.

use rusqlite::Connection;

/// Backend-agnostic storage abstraction.
///
/// Phase 1: wraps `rusqlite::Connection` without functional change.
/// Phase 2: will be implemented for `libsql::Connection` with embedded replicas.
pub trait StorageBackend {
    /// Execute a SQL statement and return the number of affected rows.
    fn execute_sql(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::types::ToSql],
    ) -> Result<usize, crate::errors::AppError>;

    /// Query a single row and map it with the provided closure.
    fn query_one<T, F>(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::types::ToSql],
        f: F,
    ) -> Result<Option<T>, crate::errors::AppError>
    where
        F: FnOnce(&rusqlite::Row<'_>) -> Result<T, rusqlite::Error>;

    /// Returns a reference to the underlying rusqlite Connection.
    /// Phase 1 escape hatch — will be removed when full migration is complete.
    fn as_connection(&self) -> &Connection;
}

/// Default implementation wrapping a rusqlite Connection.
pub struct SqliteBackend {
    conn: Connection,
}

impl SqliteBackend {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn into_inner(self) -> Connection {
        self.conn
    }
}

impl StorageBackend for SqliteBackend {
    fn execute_sql(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::types::ToSql],
    ) -> Result<usize, crate::errors::AppError> {
        self.conn
            .execute(sql, params)
            .map_err(crate::errors::AppError::Database)
    }

    fn query_one<T, F>(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::types::ToSql],
        f: F,
    ) -> Result<Option<T>, crate::errors::AppError>
    where
        F: FnOnce(&rusqlite::Row<'_>) -> Result<T, rusqlite::Error>,
    {
        match self.conn.query_row(sql, params, f) {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(crate::errors::AppError::Database(e)),
        }
    }

    fn as_connection(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_backend_wraps_connection() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY, val TEXT)")
            .unwrap();
        let backend = SqliteBackend::new(conn);
        let affected = backend
            .execute_sql(
                "INSERT INTO test (val) VALUES (?1)",
                &[&"hello" as &dyn rusqlite::types::ToSql],
            )
            .unwrap();
        assert_eq!(affected, 1);

        let result: Option<String> = backend
            .query_one("SELECT val FROM test WHERE id = 1", &[], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some("hello".to_string()));
    }
}
