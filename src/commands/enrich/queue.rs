//! Enrichment queue — SQLite-backed scan/retry/dead-letter DB.

use super::*;

// ---------------------------------------------------------------------------
// Queue DB
// ---------------------------------------------------------------------------

/// Opens or creates the enrichment queue database.
///
/// The queue schema mirrors `ingest_claude` for resume/retry parity.
/// Uses a different filename (`.enrich-queue.sqlite`) to avoid collision.
///
/// # DRY note
///
/// This is a near-verbatim copy of `open_queue_db` in `ingest_claude.rs`.
/// Both should be unified in a shared `llm_runner.rs` module by the
/// Integration stream.
pub(super) fn open_queue_db<P: AsRef<std::path::Path>>(path: P) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "wal")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS queue (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            item_key    TEXT NOT NULL UNIQUE,
            item_type   TEXT NOT NULL DEFAULT 'memory',
            status      TEXT NOT NULL DEFAULT 'pending',
            memory_id   INTEGER,
            entity_id   INTEGER,
            entities    INTEGER DEFAULT 0,
            rels        INTEGER DEFAULT 0,
            error       TEXT,
            cost_usd    REAL DEFAULT 0.0,
            attempt     INTEGER DEFAULT 0,
            elapsed_ms  INTEGER,
            created_at  TEXT DEFAULT (datetime('now')),
            done_at     TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_enrich_queue_status ON queue(status);",
    )?;
    // GAP-ENRICH-BACKLOG-CONVERGE (v1.0.96): dead-letter columns. The legacy
    // `.enrich-queue.sqlite` predates these columns and `CREATE TABLE IF NOT
    // EXISTS` never alters an existing table, so add them idempotently here.
    let mut has_error_class = false;
    let mut has_next_retry_at = false;
    // GAP-SG-12/42: the `operation` column scopes queue rows to the enrich
    // operation that enqueued them, so `--status` can segment counts per
    // operation instead of conflating a shared `item_key` space. Migrated
    // idempotently here for the same reason as the v1.0.96 columns.
    let mut has_operation = false;
    {
        let mut stmt = conn.prepare("PRAGMA table_info(queue)")?;
        let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
        for name in names {
            match name?.as_str() {
                "error_class" => has_error_class = true,
                "next_retry_at" => has_next_retry_at = true,
                "operation" => has_operation = true,
                _ => {}
            }
        }
    }
    if !has_error_class {
        conn.execute_batch("ALTER TABLE queue ADD COLUMN error_class TEXT")?;
    }
    if !has_next_retry_at {
        conn.execute_batch("ALTER TABLE queue ADD COLUMN next_retry_at TEXT")?;
    }
    if !has_operation {
        conn.execute_batch("ALTER TABLE queue ADD COLUMN operation TEXT")?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_enrich_queue_eligible ON queue(status, next_retry_at);
         CREATE INDEX IF NOT EXISTS idx_enrich_queue_operation ON queue(operation, status);
         CREATE INDEX IF NOT EXISTS idx_enrich_queue_memory ON queue(memory_id)",
    )?;
    Ok(conn)
}

/// GAP-SG-12: enqueue one scan candidate, linking it to its `memory_id` and
/// tagging it with the originating `operation`. For memory-keyed operations the
/// id is resolved from `main_conn` so the cascade cleanup (GAP-SG-13) can target
/// the queue row by `memory_id` even before the item is processed. Entity/id
/// keyed operations leave `memory_id` NULL (the `item_key` carries the link).
/// `INSERT OR IGNORE` preserves the v1.0.96 invariant that a dead-letter row is
/// never resurrected by re-enqueue (item_key is UNIQUE).
pub(super) fn enqueue_candidate(
    queue_conn: &Connection,
    main_conn: &Connection,
    namespace: &str,
    key: &str,
    item_type: &str,
    operation: &str,
) {
    let memory_id: Option<i64> = if item_type == "memory" {
        main_conn
            .query_row(
                "SELECT id FROM memories WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
                rusqlite::params![namespace, key],
                |r| r.get(0),
            )
            .ok()
    } else {
        None
    };
    if let Err(e) = queue_conn.execute(
        "INSERT OR IGNORE INTO queue (item_key, item_type, status, operation, memory_id) \
         VALUES (?1, ?2, 'pending', ?3, ?4)",
        rusqlite::params![key, item_type, operation, memory_id],
    ) {
        tracing::warn!(target: "enrich", error = %e, "queue insert failed");
    }
}

/// GAP-SG-69: item_keys vetoed `status='skipped'` for an operation. The
/// body-enrich scan selects candidates purely by `LENGTH(body) <
/// min_output_chars`, so a short body whose rewrite the preservation guard keeps
/// rejecting would be re-scanned every pass and `--until-empty` would never
/// converge. Callers exclude these keys so the scan returns only actionable
/// items; `cleanup_queue_entry` clears the veto when the body actually changes,
/// restoring the memory as a candidate.
pub(super) fn skipped_item_keys(
    conn: &Connection,
    operation: &str,
) -> Result<std::collections::HashSet<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT item_key FROM queue WHERE status='skipped' AND (operation = ?1 OR operation IS NULL)",
    )?;
    let keys = stmt
        .query_map(rusqlite::params![operation], |r| r.get::<_, String>(0))?
        .collect::<Result<std::collections::HashSet<String>, _>>()?;
    Ok(keys)
}

/// Queue `item_type` for an operation: entity-keyed operations use `"entity"`,
/// every other (memory/id-keyed) operation uses `"memory"`.
pub(super) fn item_type_for(operation: &EnrichOperation) -> &'static str {
    match operation {
        EnrichOperation::EntityDescriptions => "entity",
        _ => "memory",
    }
}

/// GAP-SG-13: remove a memory's enrich-queue entry when the memory is deleted or
/// force-merged, so the dead-letter / pending sidecar never references a row
/// that no longer exists. Best-effort and a no-op when the queue file is absent
/// (the common case after a clean run, which removes it). Targets BOTH
/// `memory_id` (populated at enqueue for memory ops, GAP-SG-12) and `item_key`
/// (the memory name) so pending rows enqueued before id resolution are also
/// cleared. Errors are logged, never propagated — cleanup must not fail the
/// caller's delete/upsert.
pub fn cleanup_queue_entry(db_path: &std::path::Path, memory_id: i64, name: &str) {
    let queue_path = crate::paths::sidecar_path(db_path, ".enrich-queue.sqlite");
    if !queue_path.exists() {
        return;
    }
    match open_queue_db(&queue_path) {
        Ok(conn) => {
            if let Err(e) = conn.execute(
                "DELETE FROM queue WHERE memory_id = ?1 OR item_key = ?2",
                rusqlite::params![memory_id, name],
            ) {
                tracing::warn!(target: "enrich", error = %e, memory_id, "enrich-queue cleanup failed");
            }
        }
        Err(e) => {
            tracing::warn!(target: "enrich", error = %e, "enrich-queue cleanup skipped (open failed)");
        }
    }
}

/// GAP-SG-66: prune ORPHAN dead-letter rows — `status='dead'` memory rows whose
/// `item_key` (the memory name) no longer exists in the main DB for `namespace`.
///
/// These are terminal "not found" failures (the memory was renamed/purged after
/// being enqueued): re-processing them just re-fails with the same not-found
/// error, so `--requeue-dead` can never recover them and they inflate
/// `queue_dead` forever. Read-only on the main DB; deletes only the
/// confirmed-orphan rows from the queue sidecar. Entity-keyed dead rows
/// (`item_type='entity'`) are left untouched — their key is an entity name, not
/// a memory name. Returns the number of rows pruned.
pub(super) fn prune_dead_orphans(
    queue_conn: &Connection,
    main_conn: &Connection,
    operation: &str,
    namespace: &str,
) -> Result<i64, AppError> {
    let dead: Vec<(i64, String)> = {
        let mut stmt = queue_conn.prepare(
            "SELECT id, item_key FROM queue \
             WHERE status='dead' AND item_type='memory' \
             AND (operation = ?1 OR operation IS NULL) ORDER BY id",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![operation], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let mut pruned = 0_i64;
    for (id, name) in dead {
        let exists = main_conn
            .query_row(
                "SELECT 1 FROM memories WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
                rusqlite::params![namespace, name],
                |_| Ok(()),
            )
            .is_ok();
        if !exists {
            queue_conn.execute("DELETE FROM queue WHERE id=?1", rusqlite::params![id])?;
            pruned += 1;
        }
    }
    if pruned > 0 {
        let _ = queue_conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }
    Ok(pruned)
}

// ---------------------------------------------------------------------------
// GAP-ENRICH-BACKLOG-CONVERGE — dead-letter classification + queue failure sink
// ---------------------------------------------------------------------------

/// Read-only `enrich --status` report (no LLM, no singleton).
///
/// GAP-SG-42: all queue counts are scoped to the current `--operation` (rows
/// migrated before the `operation` column, which are NULL, are still counted so
/// a legacy queue is not silently reported as empty).
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct EnrichStatus {
    pub(super) status_report: bool,
    pub(super) operation: String,
    pub(super) namespace: String,
    pub(super) unbound_backlog: usize,
    pub(super) queue_pending: i64,
    pub(super) queue_processing: i64,
    pub(super) queue_done: i64,
    pub(super) queue_failed: i64,
    pub(super) queue_skipped: i64,
    pub(super) queue_dead: i64,
    pub(super) eligible_now: i64,
    pub(super) waiting: i64,
    /// GAP-SG-15/46: coarse backlog state, disambiguating an empty queue from a
    /// not-yet-scanned backlog and from a cooldown wait.
    /// `draining` (eligible items now) | `cooldown` (all pending items waiting on
    /// `next_retry_at`) | `pending-scan` (candidates exist but the queue is not
    /// populated — run enrich to scan) | `empty` (nothing left to do).
    pub(super) state: &'static str,
    /// GAP-SG-16: per-item `next_retry_at` for every pending row currently in
    /// backoff, so an operator can see exactly when each will become eligible.
    pub(super) waiting_items: Vec<WaitingItem>,
}

/// GAP-SG-16: one pending queue row waiting on its backoff cooldown.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct WaitingItem {
    pub(super) item_key: String,
    pub(super) attempt: i64,
    pub(super) next_retry_at: Option<String>,
    pub(super) error_class: Option<String>,
}

/// GAP-SG-23: one dead-letter row reported by `--list-dead`.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DeadItem {
    pub(super) dead_item: bool,
    pub(super) item_key: String,
    pub(super) item_type: String,
    pub(super) attempt: i64,
    pub(super) error_class: Option<String>,
    pub(super) error: Option<String>,
}

/// GAP-SG-23/11: summary footer for `--list-dead` and `--requeue-dead`.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DeadSummary {
    pub(super) summary: bool,
    pub(super) operation: String,
    pub(super) namespace: String,
    /// `list-dead` | `requeue-dead` | `prune-dead-orphans`
    pub(super) action: &'static str,
    pub(super) dead_total: i64,
    pub(super) requeued: i64,
    /// GAP-SG-66: `prune-dead-orphans` — dead rows removed because their
    /// referenced memory no longer exists in the main DB for the namespace.
    /// Zero for `list-dead` / `requeue-dead`.
    pub(super) pruned: i64,
}

/// Classifies an enrich item failure into a retry/dead-letter outcome.
///
/// Transient errors (rate-limit, timeout, db-busy, or a message that smells
/// like a recoverable network/5xx hiccup) are rescheduled with backoff.
/// Everything else — validation, parse, invalid body, unknown — is a permanent
/// `HardFailure` routed to the dead-letter sink so the backlog can converge.
pub(super) fn classify_enrich_outcome(e: &AppError) -> crate::retry::AttemptOutcome {
    use crate::retry::AttemptOutcome;
    match e {
        AppError::RateLimited { .. } | AppError::Timeout { .. } | AppError::DbBusy(_) => {
            AttemptOutcome::Transient
        }
        // GAP-SG-09: errors that are genuinely PERMANENT for this item and must
        // dead-letter immediately (retrying cannot help): a structured provider
        // rejection (context-length overflow / refusal carried as ProviderError),
        // or a memory/entity that no longer exists (deleted between scan and
        // processing).
        AppError::ProviderError { .. }
        | AppError::NotFound(_)
        | AppError::MemoryNotFound { .. }
        | AppError::MemoryNotFoundById { .. } => AttemptOutcome::HardFailure,
        _ => {
            let msg = format!("{e}").to_lowercase();
            if msg.contains("server error")
                || msg.contains("timed out")
                || msg.contains("timeout")
                || msg.contains("connection")
                || msg.contains("5xx")
                || msg.contains("502")
                || msg.contains("503")
                || msg.contains("504")
            {
                AttemptOutcome::Transient
            } else if msg.contains("json")
                || msg.contains("no structured content")
                || msg.contains("non-object")
                || msg.contains("missing '")
            {
                // GAP-SG-09: malformed / non-JSON / shape-invalid LLM output is a
                // model HICCUP, not a permanent fault. deepseek-v4-flash:nitro
                // emits the occasional non-JSON or shape-wrong generation; with
                // strict-parse + repair (GAP-SG-10) most are recovered, and the
                // rest must be RESCHEDULED with backoff (bounded by
                // --max-attempts) instead of dead-lettering on the first try.
                AttemptOutcome::Transient
            } else {
                AttemptOutcome::HardFailure
            }
        }
    }
}

/// Applies a failure outcome to a single queue row. Shared by the parallel
/// worker and the serial loop (DRY). A `HardFailure`, or a transient failure
/// whose attempt count reached `max_attempts`, lands in the dead-letter status
/// (`status='dead'`) so it is never re-selected. A transient failure below the
/// cap is rescheduled to `pending` with an exponential-backoff `next_retry_at`.
/// Returns the [`crate::retry::AttemptOutcome`] so the caller can feed the
/// existing circuit breaker.
pub(super) fn record_item_failure(
    queue_conn: &rusqlite::Connection,
    queue_id: i64,
    attempt: i64,
    max_attempts: u32,
    err: &AppError,
) -> crate::retry::AttemptOutcome {
    use crate::retry::AttemptOutcome;
    let outcome = classify_enrich_outcome(err);
    let err_str = format!("{err}");
    let error_class = match outcome {
        AttemptOutcome::Transient => "transient",
        AttemptOutcome::HardFailure => "permanent",
        AttemptOutcome::Success => "success",
    };

    let terminal = matches!(outcome, AttemptOutcome::HardFailure) || attempt >= max_attempts as i64;
    if terminal {
        let _ = queue_conn.execute(
            "UPDATE queue SET status='dead', error=?1, error_class=?2, done_at=datetime('now') WHERE id=?3",
            rusqlite::params![err_str, error_class, queue_id],
        );
    } else {
        let delay = crate::retry::compute_delay(
            &crate::retry::RetryConfig::llm_rate_limit(),
            attempt.max(0) as u32,
        );
        let secs = delay.as_secs().max(1);
        let modifier = format!("+{secs} seconds");
        let _ = queue_conn.execute(
            "UPDATE queue SET status='pending', error=?1, error_class=?2, next_retry_at=datetime('now', ?3) WHERE id=?4",
            rusqlite::params![err_str, error_class, modifier, queue_id],
        );
    }
    outcome
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn open_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE memories (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                namespace   TEXT NOT NULL DEFAULT 'global',
                name        TEXT NOT NULL,
                type        TEXT NOT NULL DEFAULT 'note',
                description TEXT NOT NULL DEFAULT '',
                body        TEXT NOT NULL DEFAULT '',
                body_hash   TEXT NOT NULL DEFAULT '',
                session_id  TEXT,
                source      TEXT NOT NULL DEFAULT 'agent',
                metadata    TEXT NOT NULL DEFAULT '{}',
                created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                deleted_at  INTEGER,
                UNIQUE(namespace, name)
            );",
        )
        .expect("schema creation must succeed");
        conn
    }

    fn open_temp_queue() -> (Connection, String) {
        let path = format!(
            "/tmp/test-enrich-dl-{}-{}.sqlite",
            std::process::id(),
            fastrand::u64(..)
        );
        let conn = open_queue_db(&path).expect("queue db must open");
        (conn, path)
    }

    fn insert_pending(conn: &Connection, key: &str) -> i64 {
        conn.execute(
            "INSERT INTO queue (item_key, item_type, status) VALUES (?1, 'memory', 'pending')",
            rusqlite::params![key],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn queue_db_schema_creates_correctly() {
        let tmp_path = format!("/tmp/test-enrich-queue-{}.sqlite", std::process::id());
        let conn = open_queue_db(&tmp_path).expect("queue db must open");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM queue", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        let _ = std::fs::remove_file(&tmp_path);
    }

    #[test]
    fn classify_rate_limit_is_transient() {
        let e = AppError::RateLimited {
            detail: "429".into(),
        };
        assert_eq!(
            classify_enrich_outcome(&e),
            crate::retry::AttemptOutcome::Transient
        );
    }

    #[test]
    fn classify_timeout_and_dbbusy_are_transient() {
        let t = AppError::Timeout {
            operation: "judge".into(),
            duration_secs: 30,
        };
        let b = AppError::DbBusy("locked".into());
        assert_eq!(
            classify_enrich_outcome(&t),
            crate::retry::AttemptOutcome::Transient
        );
        assert_eq!(
            classify_enrich_outcome(&b),
            crate::retry::AttemptOutcome::Transient
        );
    }

    #[test]
    fn classify_validation_and_parse_are_hard_failure() {
        let v = AppError::Validation("failed to parse entities array: bad".into());
        assert_eq!(
            classify_enrich_outcome(&v),
            crate::retry::AttemptOutcome::HardFailure
        );
    }

    #[test]
    fn open_queue_db_alter_is_idempotent() {
        let path = format!(
            "/tmp/test-enrich-idem-{}-{}.sqlite",
            std::process::id(),
            fastrand::u64(..)
        );
        let _ = open_queue_db(&path).expect("first open");
        let conn = open_queue_db(&path).expect("second open is idempotent");
        let cols: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(queue)").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert!(cols.iter().any(|c| c == "error_class"));
        assert!(cols.iter().any(|c| c == "next_retry_at"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn record_item_failure_hard_marks_dead() {
        let (conn, path) = open_temp_queue();
        let id = insert_pending(&conn, "mem-hard");
        let outcome = record_item_failure(
            &conn,
            id,
            1,
            5,
            &AppError::Validation("invalid body".into()),
        );
        assert_eq!(outcome, crate::retry::AttemptOutcome::HardFailure);
        let status: String = conn
            .query_row(
                "SELECT status FROM queue WHERE id=?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "dead");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn record_item_failure_transient_reschedules_pending() {
        let (conn, path) = open_temp_queue();
        let id = insert_pending(&conn, "mem-transient");
        let outcome = record_item_failure(
            &conn,
            id,
            1,
            5,
            &AppError::RateLimited {
                detail: "429".into(),
            },
        );
        assert_eq!(outcome, crate::retry::AttemptOutcome::Transient);
        let (status, future): (String, i64) = conn
            .query_row(
                "SELECT status, (next_retry_at > datetime('now')) FROM queue WHERE id=?1",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "pending");
        assert_eq!(future, 1, "next_retry_at must be in the future");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn record_item_failure_transient_at_cap_marks_dead() {
        let (conn, path) = open_temp_queue();
        let id = insert_pending(&conn, "mem-cap");
        let outcome = record_item_failure(
            &conn,
            id,
            5,
            5,
            &AppError::RateLimited {
                detail: "429".into(),
            },
        );
        assert_eq!(outcome, crate::retry::AttemptOutcome::Transient);
        let status: String = conn
            .query_row(
                "SELECT status FROM queue WHERE id=?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "dead");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dequeue_skips_future_retry_and_dead() {
        let (conn, path) = open_temp_queue();
        let eligible = insert_pending(&conn, "mem-eligible");
        let waiting = insert_pending(&conn, "mem-waiting");
        conn.execute(
            "UPDATE queue SET next_retry_at=datetime('now', '+3600 seconds') WHERE id=?1",
            rusqlite::params![waiting],
        )
        .unwrap();
        let dead = insert_pending(&conn, "mem-dead");
        conn.execute(
            "UPDATE queue SET status='dead' WHERE id=?1",
            rusqlite::params![dead],
        )
        .unwrap();

        let claimed: Option<i64> = conn
            .query_row(
                "UPDATE queue SET status='processing', attempt=attempt+1 \
                 WHERE id = (SELECT id FROM queue WHERE status='pending' \
                               AND (next_retry_at IS NULL OR next_retry_at <= datetime('now')) \
                             ORDER BY id LIMIT 1) \
                 RETURNING id",
                [],
                |r| r.get(0),
            )
            .ok();
        assert_eq!(claimed, Some(eligible));

        let second: Option<i64> = conn
            .query_row(
                "UPDATE queue SET status='processing', attempt=attempt+1 \
                 WHERE id = (SELECT id FROM queue WHERE status='pending' \
                               AND (next_retry_at IS NULL OR next_retry_at <= datetime('now')) \
                             ORDER BY id LIMIT 1) \
                 RETURNING id",
                [],
                |r| r.get(0),
            )
            .ok();
        assert_eq!(second, None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn classify_non_json_and_shape_errors_are_transient() {
        for msg in [
            "model 'x' returned non-object JSON after repair (got string)",
            "model 'x' returned content that could not be parsed even after JSON repair",
            "model 'x' returned no structured content",
            "LLM result missing 'description' field",
            "LLM result missing 'enriched_body' field",
        ] {
            assert_eq!(
                classify_enrich_outcome(&AppError::Validation(msg.into())),
                crate::retry::AttemptOutcome::Transient,
                "expected transient for: {msg}"
            );
        }
    }

    #[test]
    fn classify_provider_error_and_not_found_are_hard() {
        assert_eq!(
            classify_enrich_outcome(&AppError::ProviderError {
                code: "400".into(),
                message: "context length exceeded".into(),
            }),
            crate::retry::AttemptOutcome::HardFailure
        );
        assert_eq!(
            classify_enrich_outcome(&AppError::NotFound("memory 'gone' not found".into())),
            crate::retry::AttemptOutcome::HardFailure
        );
    }

    #[test]
    fn open_queue_db_migrates_operation_column() {
        let (conn, path) = open_temp_queue();
        drop(conn);
        let conn = open_queue_db(&path).expect("second open is idempotent");
        let cols: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(queue)").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert!(cols.iter().any(|c| c == "operation"));
        assert!(cols.iter().any(|c| c == "memory_id"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn enqueue_candidate_tags_operation_and_memory_id() {
        let main = open_test_db();
        main.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'mem-x', 'body')",
            [],
        )
        .unwrap();
        let mem_id: i64 = main
            .query_row("SELECT id FROM memories WHERE name='mem-x'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let (queue, path) = open_temp_queue();
        enqueue_candidate(&queue, &main, "global", "mem-x", "memory", "MemoryBindings");
        let (op, mid): (String, i64) = queue
            .query_row(
                "SELECT operation, memory_id FROM queue WHERE item_key='mem-x'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(op, "MemoryBindings");
        assert_eq!(mid, mem_id);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn requeue_dead_resurrects_dead_rows() {
        let (conn, path) = open_temp_queue();
        conn.execute(
            "INSERT INTO queue (item_key, item_type, status, operation, attempt, error, error_class, next_retry_at) \
             VALUES ('mem-dead', 'memory', 'dead', 'MemoryBindings', 8, 'boom', 'permanent', datetime('now'))",
            [],
        )
        .unwrap();
        let n = conn
            .execute(
                "UPDATE queue SET status='pending', attempt=0, next_retry_at=NULL, \
                 error=NULL, error_class=NULL \
                 WHERE status='dead' AND (operation = ?1 OR operation IS NULL)",
                rusqlite::params!["MemoryBindings"],
            )
            .unwrap();
        assert_eq!(n, 1);
        let (status, attempt, nra): (String, i64, Option<String>) = conn
            .query_row(
                "SELECT status, attempt, next_retry_at FROM queue WHERE item_key='mem-dead'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(status, "pending");
        assert_eq!(attempt, 0);
        assert!(nra.is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn skipped_item_keys_excludes_only_skipped_for_operation() {
        // GAP-SG-69: the body-enrich scan must drop memories already vetoed
        // `status='skipped'` so `--until-empty` converges instead of re-scanning a
        // non-expandable short body forever (the detached worker reported a
        // stuck backlog for 30+ min).
        let (conn, path) = open_temp_queue();
        conn.execute(
            "INSERT INTO queue (item_key, item_type, status, operation) VALUES ('mem-vetoed', 'memory', 'skipped', 'BodyEnrich')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO queue (item_key, item_type, status, operation) VALUES ('mem-pending', 'memory', 'pending', 'BodyEnrich')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO queue (item_key, item_type, status, operation) VALUES ('mem-other-op', 'memory', 'skipped', 'MemoryBindings')",
            [],
        )
        .unwrap();
        let keys = skipped_item_keys(&conn, "BodyEnrich").unwrap();
        assert!(
            keys.contains("mem-vetoed"),
            "vetoed BodyEnrich item must be excluded from scan"
        );
        assert!(
            !keys.contains("mem-pending"),
            "pending item is still actionable"
        );
        assert!(
            !keys.contains("mem-other-op"),
            "skipped item from another operation must not leak"
        );
        assert_eq!(keys.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cascade_cleanup_delete_targets_memory_id_and_name() {
        let (conn, path) = open_temp_queue();
        conn.execute(
            "INSERT INTO queue (item_key, item_type, status, memory_id) VALUES ('by-id', 'memory', 'done', 42)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO queue (item_key, item_type, status) VALUES ('by-name', 'memory', 'pending')",
            [],
        )
        .unwrap();
        let removed = conn
            .execute(
                "DELETE FROM queue WHERE memory_id = ?1 OR item_key = ?2",
                rusqlite::params![42_i64, "by-name"],
            )
            .unwrap();
        assert_eq!(removed, 2);
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM queue", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn item_type_for_maps_entity_and_memory() {
        assert_eq!(
            item_type_for(&EnrichOperation::EntityDescriptions),
            "entity"
        );
        assert_eq!(item_type_for(&EnrichOperation::MemoryBindings), "memory");
        assert_eq!(item_type_for(&EnrichOperation::AugmentBindings), "memory");
        assert_eq!(item_type_for(&EnrichOperation::BodyExtract), "memory");
    }

    #[test]
    fn prune_dead_orphans_removes_only_orphan_memory_rows() {
        let main = open_test_db();
        // One live memory whose dead row must be KEPT (it still exists).
        main.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'alive', 'b')",
            [],
        )
        .unwrap();
        let (queue, path) = open_temp_queue();
        // Orphan dead memory row (no matching memory) -> pruned.
        queue
            .execute(
                "INSERT INTO queue (item_key, item_type, status, operation, error_class) \
                 VALUES ('gone', 'memory', 'dead', 'MemoryBindings', 'permanent')",
                [],
            )
            .unwrap();
        // Live dead memory row (memory exists) -> kept.
        queue
            .execute(
                "INSERT INTO queue (item_key, item_type, status, operation, error_class) \
                 VALUES ('alive', 'memory', 'dead', 'MemoryBindings', 'permanent')",
                [],
            )
            .unwrap();
        // Entity dead row -> never touched (key is not a memory name).
        queue
            .execute(
                "INSERT INTO queue (item_key, item_type, status, operation) \
                 VALUES ('some-entity', 'entity', 'dead', 'EntityDescriptions')",
                [],
            )
            .unwrap();

        let pruned = prune_dead_orphans(&queue, &main, "MemoryBindings", "global").unwrap();
        assert_eq!(pruned, 1, "only the orphan memory row is pruned");

        let remaining: Vec<String> = {
            let mut stmt = queue
                .prepare("SELECT item_key FROM queue ORDER BY item_key")
                .unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(remaining, vec!["alive", "some-entity"]);
        let _ = std::fs::remove_file(&path);
    }
}
