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

/// Builds a fresh `Command` with the mock LLM PATH prepended.
///
/// v1.0.76 spawns `claude` or `codex` on every `remember` / `ingest` /
/// `edit`. The bundled mocks under `tests/mock-llm/` return a fixed
/// 64-dim zero vector so the binary finishes without a real OAuth
/// login. The mock directory is leaked (no TempDir cleanup) so the
/// spawned subprocess always finds the mocks.
fn sgr_cmd() -> Command {
    let mock_dir = common::mock_llm_path();
    let mut c = Command::cargo_bin("sqlite-graphrag").expect("sqlite-graphrag binary not found");
    c.env("PATH", common::prepend_path(&mock_dir));
    c
}

#[path = "common/mod.rs"]
mod common;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Runs `sqlite-graphrag init` on an isolated temporary database and returns
/// the `TempDir` (to keep the database alive) and the SQLite file path.
fn init_isolated_db() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().expect("TempDir must be created");
    let db_path = tmp.path().join("test.sqlite");

    sgr_cmd()
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
// Test 1 — init applies exactly 13 migrations V001 through V013
// ---------------------------------------------------------------------------
// v1.0.76 added V012 and V013 on top of the historical V001-V011 set.

#[test]
#[serial]
fn init_creates_15_migrations_v001_to_v015() {
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
        15,
        "exactly 15 migrations must be applied, found: {versions:?}"
    );
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        "expected versions V001-V015"
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
// Test 4 — trigger trg_fts_au is INTENTIONALLY ABSENT (FTS5 sync handled in Rust)
// ---------------------------------------------------------------------------
// v1.0.76 removed sqlite-vec, but the design choice of handling FTS5 sync
// in Rust (edit.rs, rename.rs, restore.rs) instead of a trigger is kept.
// trg_fts_ai and trg_fts_ad are created by V004; trg_fts_au is NOT,
// because the Rust handlers cover UPDATE-equivalent operations explicitly
// and we avoid the historical sqlite-vec / FTS5 conflict inside the
// trigger body for symmetry with the v1.0.74 design.

#[test]
#[serial]
fn trigger_trg_fts_au_absent_handled_in_rust() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    assert!(
        !trigger_exists(&conn, "trg_fts_au"),
        "trigger trg_fts_au must NOT exist — FTS5 sync is handled in Rust (edit.rs, rename.rs, restore.rs)"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — memory_embeddings uses BLOB and dim=384 (v1.0.76 replacement for vec_memories)
// ---------------------------------------------------------------------------
// v1.0.76 dropped vec_memories (sqlite-vec virtual table) and replaced it with
// a regular BLOB-backed memory_embeddings table. The embedding dimensionality
// is recorded in the dim column rather than in the DDL. Cosine similarity is
// computed in pure Rust at query time (src/similarity.rs).

#[test]
#[serial]
fn memory_embeddings_blob_dim_384() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    let ddl: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'memory_embeddings'",
            [],
            |row| row.get(0),
        )
        .expect("memory_embeddings must exist in sqlite_master");

    assert!(
        ddl.contains("BLOB"),
        "memory_embeddings must declare embedding as BLOB, DDL was: {ddl}"
    );
    assert!(
        ddl.contains("dim"),
        "memory_embeddings must declare a dim column, DDL was: {ddl}"
    );
    assert!(
        ddl.contains("384"),
        "memory_embeddings must default dim to 384, DDL was: {ddl}"
    );

    // Confirm sqlite-vec tables are GONE.
    let vec_present: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'vec_memories'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(1);
    assert_eq!(
        vec_present, 0,
        "vec_memories must NOT exist after V013, but it is still present"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — memory_embeddings has 2 partition-like indexes (namespace, source)
// ---------------------------------------------------------------------------
// vec_memories used sqlite-vec partition keys. memory_embeddings uses regular
// SQLite indexes. The functional requirement is "find embeddings by namespace"
// and "audit embeddings by source".

#[test]
#[serial]
fn memory_embeddings_partition_indexes() {
    let (_tmp, db_path) = init_isolated_db();
    let conn = conn_ro(&db_path);

    let has_ns_index: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'idx_memory_embeddings_ns'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    assert_eq!(
        has_ns_index, 1,
        "idx_memory_embeddings_ns must exist (namespace partition)"
    );

    let has_source_index: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'idx_memory_embeddings_source'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    assert_eq!(
        has_source_index, 1,
        "idx_memory_embeddings_source must exist (source partition)"
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
    sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    // Insert memory with accented text
    sgr_cmd()
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
        "idx_relationships_source",
        "idx_relationships_target",
        "idx_relationships_ns",
        "idx_relationships_ns_relation",
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
fn schema_version_meta_equals_15() {
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
        version, "15",
        "schema_version in schema_meta must be '15' after V015"
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
    sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    // Insert a memory with the new type=document accepted by V009.
    let output = sgr_cmd()
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
    let output = sgr_cmd()
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
    let output = sgr_cmd()
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

    sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    let output = sgr_cmd()
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

    let output = sgr_cmd()
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

    let output = sgr_cmd()
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

    sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    let output = sgr_cmd()
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

// ---------------------------------------------------------------------------
// v1.0.76 — migrate --rehash and --to-llm-only integration tests
// ---------------------------------------------------------------------------
// These tests exercise the CLI subcommands end-to-end through `assert_cmd`.
// They cover three real-world flows:
//   1. --rehash on a healthy fresh DB is a no-op (status = ok_no_changes).
//   2. --rehash rewrites a corrupted V001 checksum and the next `migrate`
//      run no longer fails with "applied migration V1 is different than
//      filesystem one V1".
//   3. --to-llm-only on a fresh v1.0.76 DB reports no vec tables and a
//      successful schema_version 13 (V013 applied).
//   4. --to-llm-only refuses to run without the explicit --drop-vec-tables
//      safety guard (exit code 1, validation error).

#[test]
#[serial]
fn migrate_rehash_is_noop_on_healthy_db() {
    let (_tmp, db_path) = init_isolated_db();

    let output = sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .args(["--skip-memory-guard", "migrate", "--rehash"])
        .output()
        .expect("migrate --rehash must run");

    assert!(
        output.status.success(),
        "migrate --rehash must succeed on a healthy DB. stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert_eq!(
        json["status"], "ok_no_changes",
        "healthy DB must report ok_no_changes, got: {stdout}"
    );
    assert_eq!(json["rewritten"].as_array().unwrap().len(), 0);
    assert_eq!(json["inspected"], 15);
    assert_eq!(json["schema_version"], 15);
}

#[test]
#[serial]
fn migrate_rehash_fixes_corrupted_checksum() {
    let (_tmp, db_path) = init_isolated_db();

    // Corrupt the V001 checksum so the next `migrate` would fail.
    let conn = conn_ro(&db_path);
    conn.execute_batch(
        "UPDATE refinery_schema_history SET checksum = '999999999999' WHERE version = 1",
    )
    .expect("corrupt V001 checksum");
    drop(conn);

    // Sanity: a regular `migrate` should now fail.
    let bad = sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .args(["--skip-memory-guard", "migrate"])
        .output()
        .expect("migrate must run");
    assert!(
        !bad.status.success(),
        "migrate must fail on a corrupted checksum, got: {:?}",
        bad.status
    );

    // `migrate --rehash` should detect the mismatch, rewrite the row,
    // and exit 0 with status=ok_rewritten.
    let good = sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .args(["--skip-memory-guard", "migrate", "--rehash"])
        .output()
        .expect("migrate --rehash must run");
    assert!(
        good.status.success(),
        "migrate --rehash must succeed. stderr={}",
        String::from_utf8_lossy(&good.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&good.stdout).expect("JSON");
    assert_eq!(json["status"], "ok_rewritten");
    assert_eq!(json["rewritten"].as_array().unwrap().len(), 1);
    assert_eq!(json["rewritten"][0]["version"], 1);
    assert_eq!(json["rewritten"][0]["name"], "init");
    assert_eq!(json["rewritten"][0]["old_checksum"], "999999999999");

    // And a subsequent plain `migrate` should now succeed.
    let after = sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .args(["--skip-memory-guard", "migrate"])
        .output()
        .expect("migrate must run");
    assert!(
        after.status.success(),
        "migrate must succeed after rehash. stderr={}",
        String::from_utf8_lossy(&after.stderr)
    );
}

#[test]
#[serial]
fn migrate_to_llm_only_reports_no_vec_tables_on_fresh_db() {
    let (_tmp, db_path) = init_isolated_db();

    let output = sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .args([
            "--skip-memory-guard",
            "migrate",
            "--to-llm-only",
            "--drop-vec-tables",
        ])
        .output()
        .expect("migrate --to-llm-only must run");

    assert!(
        output.status.success(),
        "migrate --to-llm-only must succeed on a fresh v1.0.76 DB. stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(json["status"], "ok");
    assert_eq!(json["schema_version"], 15);
    assert_eq!(json["v013_applied"], true);
    assert_eq!(
        json["vec_tables_were_present"], false,
        "fresh v1.0.76 DBs must not have vec0 virtual tables"
    );
    assert_eq!(json["rehashed"].as_array().unwrap().len(), 0);
}

#[test]
#[serial]
fn migrate_to_llm_only_requires_drop_vec_tables_safety_guard() {
    let (_tmp, db_path) = init_isolated_db();

    let output = sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .args(["--skip-memory-guard", "migrate", "--to-llm-only"])
        .output()
        .expect("migrate --to-llm-only must run");

    assert!(
        !output.status.success(),
        "migrate --to-llm-only without --drop-vec-tables must refuse to run"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("JSON");
    assert_eq!(json["code"], 1, "validation error code 1 expected");
    let msg = json["message"].as_str().unwrap_or("").to_string();
    assert!(
        msg.contains("--drop-vec-tables"),
        "error message must mention --drop-vec-tables, got: {msg}"
    );
}

#[test]
#[serial]
fn migrate_rehash_fixes_null_applied_on() {
    let (_tmp, db_path) = init_isolated_db();

    // NULL out applied_on for all rows to simulate the G40 bug.
    let conn = conn_ro(&db_path);
    conn.execute_batch("UPDATE refinery_schema_history SET applied_on = NULL")
        .expect("nullify applied_on");
    drop(conn);

    // migrate --rehash must succeed and fix the NULL rows.
    let output = sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .args(["--skip-memory-guard", "migrate", "--rehash"])
        .output()
        .expect("migrate --rehash must run");

    assert!(
        output.status.success(),
        "migrate --rehash must succeed on DB with NULL applied_on. stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON");
    assert!(
        json["null_rows_fixed"].as_u64().unwrap_or(0) > 0,
        "must report null_rows_fixed > 0, got: {}",
        json["null_rows_fixed"]
    );

    // A subsequent plain migrate must also succeed (runner reads applied_on).
    let after = sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .args(["--skip-memory-guard", "migrate"])
        .output()
        .expect("migrate must run");
    assert!(
        after.status.success(),
        "migrate must succeed after rehash fixed NULLs. stderr={}",
        String::from_utf8_lossy(&after.stderr)
    );

    // Verify zero NULL rows remain via rusqlite.
    let conn = conn_ro(&db_path);
    let null_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM refinery_schema_history WHERE applied_on IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(null_count, 0, "no NULL applied_on rows must remain");
}

#[test]
#[serial]
fn migrate_to_llm_only_fixes_null_applied_on() {
    let (_tmp, db_path) = init_isolated_db();

    let conn = conn_ro(&db_path);
    conn.execute_batch("UPDATE refinery_schema_history SET applied_on = NULL")
        .expect("nullify applied_on");
    drop(conn);

    let output = sgr_cmd()
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .args([
            "--skip-memory-guard",
            "migrate",
            "--to-llm-only",
            "--drop-vec-tables",
        ])
        .output()
        .expect("migrate --to-llm-only must run");

    assert!(
        output.status.success(),
        "migrate --to-llm-only must succeed with NULL applied_on. stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON");
    assert!(
        json["null_rows_fixed"].as_u64().unwrap_or(0) > 0,
        "must report null_rows_fixed > 0, got: {}",
        json["null_rows_fixed"]
    );
    assert_eq!(json["status"], "ok");
}
