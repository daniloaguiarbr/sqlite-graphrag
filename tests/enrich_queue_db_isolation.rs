//! GAP-SG-64 regression: the enrich queue sidecar must live next to `--db`,
//! not the process CWD. We plant a pending queue in db_a's directory and run
//! `enrich --status` from an unrelated CWD; before the fix the status read the
//! CWD (empty -> 0), after the fix it reads next to `--db` (-> 1).

use rusqlite::Connection;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_sqlite-graphrag");

fn init_db(db: &Path) {
    let st = Command::new(BIN)
        .args([
            "--embedding-backend",
            "openrouter",
            "--embedding-model",
            "qwen/qwen3-embedding-8b",
            "init",
            "--db",
        ])
        .arg(db)
        .args(["--namespace", "t"])
        .status()
        .expect("spawn init");
    assert!(st.success(), "init failed for {}", db.display());
}

fn plant_pending_queue(dir: &Path) {
    let q = dir.join(".enrich-queue.sqlite");
    let conn = Connection::open(&q).expect("open planted queue");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            item_key TEXT NOT NULL UNIQUE,
            item_type TEXT NOT NULL DEFAULT 'memory',
            status TEXT NOT NULL DEFAULT 'pending',
            memory_id INTEGER, entity_id INTEGER, entities INTEGER DEFAULT 0,
            rels INTEGER DEFAULT 0, error TEXT, cost_usd REAL DEFAULT 0.0,
            attempt INTEGER DEFAULT 0, elapsed_ms INTEGER,
            created_at TEXT DEFAULT (datetime('now')), done_at TEXT
        );",
    )
    .expect("create queue schema");
    conn.execute(
        "INSERT INTO queue (item_key, status) VALUES ('regress-mem', 'pending')",
        [],
    )
    .expect("insert pending row");
}

fn status_pending(db: &Path, cwd: &Path) -> i64 {
    let out = Command::new(BIN)
        .current_dir(cwd)
        .args([
            "--embedding-backend",
            "openrouter",
            "--embedding-model",
            "qwen/qwen3-embedding-8b",
            "enrich",
            "--status",
            "--db",
        ])
        .arg(db)
        .args([
            "--namespace",
            "t",
            "--mode",
            "openrouter",
            "--openrouter-model",
            "deepseek/deepseek-v4-flash:nitro",
            "--operation",
            "memory-bindings",
            "--json",
        ])
        .output()
        .expect("spawn status");
    assert!(
        out.status.success(),
        "status exit {:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout
        .lines()
        .rev()
        .find(|l| l.contains("queue_pending"))
        .expect("status json line with queue_pending");
    let v: serde_json::Value = serde_json::from_str(line.trim()).expect("parse status json");
    v["queue_pending"].as_i64().expect("queue_pending i64")
}

#[test]
fn enrich_queue_follows_db_dir_not_cwd() {
    let dir_a = TempDir::new().expect("tmp a");
    let dir_b = TempDir::new().expect("tmp b");
    let cwd = TempDir::new().expect("tmp cwd");
    let db_a = dir_a.path().join("db.sqlite");
    let db_b = dir_b.path().join("db.sqlite");

    init_db(&db_a);
    init_db(&db_b);
    plant_pending_queue(dir_a.path());

    // From an unrelated CWD, --db db_a must read the queue planted in db_a's dir.
    assert_eq!(
        status_pending(&db_a, cwd.path()),
        1,
        "enrich --status must read the queue next to --db (db_a), not the CWD"
    );
    // Control: db_b has no planted queue -> 0 (proves isolation per --db dir).
    assert_eq!(
        status_pending(&db_b, cwd.path()),
        0,
        "db_b dir has no queue; must report 0 regardless of CWD"
    );
}
