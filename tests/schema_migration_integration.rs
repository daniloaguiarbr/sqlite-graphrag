#![cfg(feature = "slow-tests")]

// Suite 3 — Schema and migrations validation V001-V009
//
// ISOLATION: each test uses `SQLITE_GRAPHRAG_DB_PATH` pointing to a SQLite
// file in an exclusive `TempDir`. Introspection runs through rusqlite directly,
// without depending on any binary output.
//
// NOTE: sqlite-vec uses `sqlite3_auto_extension`, which is process-global.
// To avoid registering the extension multiple times in parallel tests,
// every test that opens a sqlite-vec database does so via `sqlite-graphrag init`
// (external binary), which loads the extension in its own process. Pure
// introspection tests (sqlite_master, triggers, FTS) open the database via
// rusqlite after init for read-only queries — they do not load sqlite-vec
// in the test process.
//
// `#[serial]` is mandatory: although each test uses its own DB, the compiled
// artefact is shared and `TempDir` is only released after the test ends;
// serialising eliminates filesystem races and makes timings predictable.

use assert_cmd::Command;
use rusqlite::Connection;
use serial_test::serial;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Runs `sqlite-graphrag init` on an isolated temporary database and returns
/// the `TempDir` (to keep the database alive) and the SQLite file path.
fn init_isolated_db() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().expect("TempDir must be created");
    let db_path = tmp.path().join("test.sqlite");

    Command::cargo_bin("sqlite-graphrag")
        .expect("sqlite-graphrag binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    (tmp, db_path)
}

/// Opens the database read-only after init (without sqlite-vec in the test process).
fn conn_ro(db_path: &std::path::Path) -> Connection {
    Connection::open(db_path).expect("database connection must work")
}

/// Checks whether a table or view exists in `sqlite_master`.
fn table_exists(conn: &Connection, name: &str) -> bool {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table','view') AND name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        )
        .unwrap_or(0);
    count > 0
}

/// Checks whether a trigger exists in `sqlite_master`.
fn trigger_exists(conn: &Connection, name: &str) -> bool {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        )
        .unwrap_or(0);
    count > 0
}

/// Checks if an index exists in `sqlite_master`.
fn index_exists(conn: &Connection, name: &str) -> bool {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        )
        .unwrap_or(0);
    count > 0
}

// ---------------------------------------------------------------------------
// Test 1 — init applies exactly 9 migrations V001 through V009
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn init_creates_9_migrations_v001_to_v009() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    let versions: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT version FROM refinery_schema_history ORDER BY version ASC")
            .expect("prepare must work");
        stmt.query_map([], |row| row.get(0))
            .expect("query must work")
            .map(|r| r.expect("row must be readable"))
            .collect()
    };

    assert_eq!(
        versions.len(),
        9,
        "exactly 9 migrations must be applied, found: {versions:?}"
    );
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
        "expected versions V001-V009"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — trigger trg_fts_ai exists after V004
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn trigger_trg_fts_ai_exists() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    assert!(
        trigger_exists(&conn, "trg_fts_ai"),
        "trigger trg_fts_ai must exist after V004"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — trigger trg_fts_ad exists after V004
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn trigger_trg_fts_ad_exists() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    assert!(
        trigger_exists(&conn, "trg_fts_ad"),
        "trigger trg_fts_ad must exist after V004"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — trigger trg_fts_au is INTENTIONALLY ABSENT (sqlite-vec conflict)
// ---------------------------------------------------------------------------
// V004 explicitly documents that trg_fts_au is omitted because sqlite-vec
// loaded via sqlite3_auto_extension conflicts with FTS5 on AFTER UPDATE
// triggers. Edit/rename synchronisation lives in Rust code (edit.rs, rename.rs).

#[test]
#[serial]
fn trigger_trg_fts_au_absent_due_to_vec_conflict() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    assert!(
        !trigger_exists(&conn, "trg_fts_au"),
        "trigger trg_fts_au must NOT exist — sqlite-vec conflicts with FTS5 AFTER UPDATE"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — vec_memories uses float[384] and distance_metric=cosine
// ---------------------------------------------------------------------------
// Verifies via DDL from sqlite_master that the vec0 table definition includes
// the correct dimension and distance metric parameters.

#[test]
#[serial]
fn vec_memories_dim_384_cosine() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    let ddl: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'vec_memories'",
            [],
            |row| row.get(0),
        )
        .expect("vec_memories must exist in sqlite_master");

    assert!(
        ddl.contains("float[384]"),
        "vec_memories must declare float[384], DDL was: {ddl}"
    );
    assert!(
        ddl.contains("distance_metric=cosine"),
        "vec_memories must use distance_metric=cosine, DDL was: {ddl}"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — vec_memories has 2 partition keys (namespace, type)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn vec_memories_partition_keys_namespace_type() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    let ddl: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'vec_memories'",
            [],
            |row| row.get(0),
        )
        .expect("vec_memories must exist in sqlite_master");

    // Both columns must appear with 'partition key' in the DDL
    let namespace_pk = ddl.contains("namespace") && ddl.to_lowercase().contains("partition key");
    let type_pk = ddl.contains("type") && ddl.to_lowercase().contains("partition key");

    assert!(
        namespace_pk,
        "vec_memories must declare 'namespace' as partition key, DDL: {ddl}"
    );
    assert!(
        type_pk,
        "vec_memories must declare 'type' as partition key, DDL: {ddl}"
    );
}

// ---------------------------------------------------------------------------
// Test 7 — fts_memories uses tokenizer unicode61 with remove_diacritics 1
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn fts_memories_tokenizer_unicode61_remove_diacritics() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    let ddl: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'fts_memories'",
            [],
            |row| row.get(0),
        )
        .expect("fts_memories must exist in sqlite_master");

    assert!(
        ddl.contains("unicode61"),
        "fts_memories must use the unicode61 tokenizer, DDL: {ddl}"
    );
    assert!(
        ddl.contains("remove_diacritics"),
        "fts_memories must declare remove_diacritics, DDL: {ddl}"
    );
}

// ---------------------------------------------------------------------------
// Test 8 — FTS5 search 'cafe' matches text containing 'café' (remove_diacritics)
// ---------------------------------------------------------------------------
// Inserts a memory with an accented body via the CLI and verifies that an
// unaccented search succeeds, confirming that remove_diacritics is active.

#[test]
#[serial]
fn fts5_matching_with_accents_cafe_cafe() {
    let tmp = TempDir::new().expect("TempDir must be created");
    let db_path = tmp.path().join("test.sqlite");

    // DB init
    Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    // Insert memory with accented text
    Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .env("SQLITE_GRAPHRAG_NAMESPACE", "global")
        .args([
            "--skip-memory-guard",
            "remember",
            "--name",
            "nota-cafe",
            "--type",
            "user",
            "--description",
            "note about café",
            "--body",
            "Brazilian café is famous worldwide for its quality",
        ])
        .assert()
        .success();

    // Unaccented search must find the accented memory (remove_diacritics=1)
    let conn = conn_ro(&db_path);
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM fts_memories WHERE fts_memories MATCH 'cafe'",
            [],
            |row| row.get(0),
        )
        .expect("FTS5 query must work");

    assert!(
        count >= 1,
        "FTS5 with remove_diacritics must match 'café' when searching 'cafe', count={count}"
    );
}

// ---------------------------------------------------------------------------
// Test 9 — main tables exist after init
// ---------------------------------------------------------------------------
// Verifies all 7 regular tables plus virtual vec/fts tables created by migrations.

#[test]
#[serial]
fn all_main_tables_exist_after_init() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    let tables = [
        "schema_meta",
        "memories",
        "memory_versions",
        "memory_chunks",
        "entities",
        "relationships",
        "memory_entities",
        "memory_relationships",
        "fts_memories",
    ];

    for name in tables {
        assert!(
            table_exists(&conn, name),
            "table '{name}' must exist after init"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 10 — main indexes from V001 and V005 exist
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn main_indexes_exist_after_init() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    let indexes = [
        "idx_memories_ns_type",
        "idx_memories_ns_live",
        "idx_memories_body_hash",
        "idx_entities_ns",
        "idx_me_entity",
        "idx_relationships_source_id",
        "idx_relationships_target_id",
        "idx_relationships_namespace_relation",
        "idx_entities_namespace_degree",
        "idx_memory_chunks_memory_id",
        "idx_memory_relationships_relationship_id",
    ];

    for name in indexes {
        assert!(
            index_exists(&conn, name),
            "index '{name}' must exist after init"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 11 — schema_meta contains required keys after init
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_meta_required_keys_exist() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    let expected_keys = [
        "schema_version",
        "model",
        "dim",
        "created_at",
        "namespace_initial",
    ];

    for key in expected_keys {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_meta WHERE key = ?1",
                rusqlite::params![key],
                |row| row.get(0),
            )
            .expect("schema_meta query must work");

        assert!(count > 0, "schema_meta must contain key '{key}' after init");
    }
}

// ---------------------------------------------------------------------------
// Test 12 — schema_version in schema_meta matches V009 (9)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_version_meta_equals_9() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    let version: String = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .expect("schema_version must exist in schema_meta");

    assert_eq!(
        version, "9",
        "schema_version in schema_meta must be '9' after V009"
    );
}

// ---------------------------------------------------------------------------
// Test 13 — V009 e2e: full lifecycle for the new `document` memory type
// ---------------------------------------------------------------------------
// V009 expanded the `memories.type` CHECK constraint to accept `document`
// and `note` in addition to the seven pre-existing types. This test validates
// the full path: remember -> list (filtered by type) -> recall.

#[test]
#[serial]
fn v009_document_type_lifecycle_e2e() {
    let tmp = TempDir::new().expect("TempDir must be created");
    let db_path = tmp.path().join("test.sqlite");

    // Init applies V001..V009 in a fresh DB.
    Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    // Insert a memory with the new type=document accepted by V009.
    let output = Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .env("SQLITE_GRAPHRAG_NAMESPACE", "global")
        .args([
            "--skip-memory-guard",
            "remember",
            "--name",
            "doc-test",
            "--type",
            "document",
            "--description",
            "test doc",
            "--body",
            "Sample document body for e2e test",
            "--skip-extraction",
        ])
        .output()
        .expect("remember must run");
    assert!(
        output.status.success(),
        "remember failed: status={:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // List filtered by type=document must return the inserted record.
    let output = Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args([
            "--skip-memory-guard",
            "list",
            "--type",
            "document",
            "--json",
        ])
        .output()
        .expect("list must run");
    assert!(
        output.status.success(),
        "list failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list output must be valid JSON");
    let items = json["items"]
        .as_array()
        .expect("list response must contain `items` array");
    assert_eq!(items.len(), 1, "expected exactly 1 document, got {items:?}");
    assert_eq!(items[0]["type"], "document");

    // Recall via FTS5/vector must surface the freshly inserted document.
    let output = Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "recall", "Sample", "--json"])
        .output()
        .expect("recall must run");
    assert!(
        output.status.success(),
        "recall failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("recall output must be valid JSON");
    let results = json["results"]
        .as_array()
        .expect("recall response must contain `results` array");
    assert!(
        !results.is_empty(),
        "recall must return at least one match for 'Sample', got: {results:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 14 — V009 e2e: full lifecycle for the new `note` memory type
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn v009_note_type_lifecycle_e2e() {
    let tmp = TempDir::new().expect("TempDir must be created");
    let db_path = tmp.path().join("test.sqlite");

    Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    let output = Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .env("SQLITE_GRAPHRAG_NAMESPACE", "global")
        .args([
            "--skip-memory-guard",
            "remember",
            "--name",
            "note-test",
            "--type",
            "note",
            "--description",
            "test note",
            "--body",
            "Quick scratch note for e2e validation",
            "--skip-extraction",
        ])
        .output()
        .expect("remember must run");
    assert!(
        output.status.success(),
        "remember failed: status={:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let output = Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "list", "--type", "note", "--json"])
        .output()
        .expect("list must run");
    assert!(
        output.status.success(),
        "list failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list output must be valid JSON");
    let items = json["items"]
        .as_array()
        .expect("list response must contain `items` array");
    assert_eq!(items.len(), 1, "expected exactly 1 note, got {items:?}");
    assert_eq!(items[0]["type"], "note");

    let output = Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "recall", "scratch", "--json"])
        .output()
        .expect("recall must run");
    assert!(
        output.status.success(),
        "recall failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("recall output must be valid JSON");
    let results = json["results"]
        .as_array()
        .expect("recall response must contain `results` array");
    assert!(
        !results.is_empty(),
        "recall must return at least one match for 'scratch', got: {results:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 15 — V009: invalid memory type must be rejected by clap value_enum
// ---------------------------------------------------------------------------
// `--type` is bound to `MemoryType` via `value_enum`, so clap rejects unknown
// variants before reaching the SQLite CHECK constraint. This guards against
// a future regression where the enum drifts away from the migration's CHECK.

#[test]
#[serial]
fn v009_invalid_type_rejected() {
    let tmp = TempDir::new().expect("TempDir must be created");
    let db_path = tmp.path().join("test.sqlite");

    Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    let output = Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .env("SQLITE_GRAPHRAG_NAMESPACE", "global")
        .args([
            "--skip-memory-guard",
            "remember",
            "--name",
            "x",
            "--type",
            "invalid_type_xyz",
            "--description",
            "t",
            "--body",
            "t",
        ])
        .output()
        .expect("remember must run");

    assert!(
        !output.status.success(),
        "remember must reject invalid type 'invalid_type_xyz'"
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    assert!(
        stderr.contains("invalid") || stderr.contains("type") || stderr.contains("possible values"),
        "stderr should mention type rejection, got: {stderr}"
    );
}
