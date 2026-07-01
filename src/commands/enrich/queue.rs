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
    // GAP-SG-76: without an explicit busy_timeout, a lock contention window
    // between the dequeue claim and a concurrent worker/main-DB writer
    // surfaces as SQLITE_BUSY immediately instead of retrying briefly.
    // Reuses the project-wide canonical value (see rules_rust_sqlite.md —
    // "DEFINIR busy_timeout em milissegundos explícitos por conexão").
    conn.pragma_update(None, "busy_timeout", crate::constants::BUSY_TIMEOUT_MILLIS)?;
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
    // GAP-SG-72: dead-letter diagnostics carried from a typed OpenRouter
    // `ChatError` (finish_reason + token counts) so `--list-dead` can show
    // WHY an item died (e.g. truncated by max_tokens) instead of only the
    // formatted error string. Migrated idempotently for the same reason as
    // the columns above.
    let mut has_finish_reason = false;
    let mut has_input_tokens = false;
    let mut has_output_tokens = false;
    {
        let mut stmt = conn.prepare("PRAGMA table_info(queue)")?;
        let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
        for name in names {
            match name?.as_str() {
                "error_class" => has_error_class = true,
                "next_retry_at" => has_next_retry_at = true,
                "operation" => has_operation = true,
                "finish_reason" => has_finish_reason = true,
                "input_tokens" => has_input_tokens = true,
                "output_tokens" => has_output_tokens = true,
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
    if !has_finish_reason {
        conn.execute_batch("ALTER TABLE queue ADD COLUMN finish_reason TEXT")?;
    }
    if !has_input_tokens {
        conn.execute_batch("ALTER TABLE queue ADD COLUMN input_tokens INTEGER")?;
    }
    if !has_output_tokens {
        conn.execute_batch("ALTER TABLE queue ADD COLUMN output_tokens INTEGER")?;
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
    /// GAP-SG-77: DATABASE-semantics backlog for the queried operation, computed
    /// by `scan::count_operation_backlog` via a `SELECT COUNT(*)` over the real
    /// store. This is distinct from `queue_pending`/`queue_dead` (FILE/sidecar
    /// queue semantics) and from the legacy `unbound_backlog` (memory-bindings
    /// only). It fixes the false `pending=0` that db-backed operations
    /// (entity-descriptions/body-enrich/re-embed) previously reported.
    pub(super) scan_backlog: i64,
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
    /// GAP-SG-72: `choices[0].finish_reason` from the OpenRouter response
    /// that produced this failure, when one was decoded (e.g. `"length"`
    /// for a max_tokens truncation). `None` for subprocess-provider modes
    /// or failures that never reached a decoded response.
    pub(super) finish_reason: Option<String>,
    /// GAP-SG-72: `usage.prompt_tokens` from the same response, when known.
    pub(super) input_tokens: Option<i64>,
    /// GAP-SG-72: `usage.completion_tokens` from the same response, when known.
    pub(super) output_tokens: Option<i64>,
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
/// This is the FALLBACK classifier: it is only consulted when the failure
/// did not already carry a typed [`crate::retry::AttemptOutcome`] computed at
/// its origin (see [`record_item_failure_typed`], fed by
/// [`crate::commands::enrich::extraction::take_last_openrouter_failure`] for
/// OpenRouter chat/embedding calls). Classification is TYPED by `AppError`
/// variant only — NEVER by matching the formatted message — per
/// `rules_rust_retry_com_backoff.md` ("NUNCA usar string matching em
/// mensagens de erro").
pub(super) fn classify_enrich_outcome(e: &AppError) -> crate::retry::AttemptOutcome {
    use crate::retry::AttemptOutcome;
    match e {
        AppError::RateLimited { .. } | AppError::Timeout { .. } | AppError::DbBusy(_) => {
            AttemptOutcome::Transient
        }
        // GAP-SG-78: a referenced entity that is not yet materialized is a
        // TRANSITORY absence — a later enrich pass creates the entity — so the
        // item is rescheduled, not dead-lettered on the first miss. Matched on
        // the typed variant, never a message substring (rules_rust_retry: NUNCA
        // string matching). The `--max-attempts` floor (default 8) still ends
        // the item if the entity never materializes, mirroring the `Embedding`
        // floor below.
        AppError::EntityNotYetMaterialized { .. } => AttemptOutcome::Transient,
        // GAP-SG-09: errors that are genuinely PERMANENT for this item and must
        // dead-letter immediately (retrying cannot help): a structured provider
        // rejection (context-length overflow / refusal carried as ProviderError),
        // or a MEMORY that no longer exists (deleted or renamed between scan and
        // processing). Entity absence is handled above as transitory, NOT here.
        AppError::ProviderError { .. }
        | AppError::NotFound(_)
        | AppError::MemoryNotFound { .. }
        | AppError::MemoryNotFoundById { .. } => AttemptOutcome::HardFailure,
        // GAP-SG-76: SQLITE_BUSY/LOCKED is a lock-contention hiccup between the
        // queue writer and a concurrent claim — retry it; any other database
        // error (constraint violation, corruption, I/O) is permanent.
        AppError::Database(_) => {
            if crate::storage::utils::is_sqlite_busy(e) {
                AttemptOutcome::Transient
            } else {
                AttemptOutcome::HardFailure
            }
        }
        // GAP-SG-73: safe floor for the `re-embed` operation. `AppError::Embedding`
        // reaches here only via `embed_with_fallback`'s backend-chain resolution
        // (`crate::embedder`), which discards the origin-typed
        // `EmbedError::retry_class` through `From<EmbedError> for AppError` before
        // the error surfaces to the queue. Extracting the precise verdict would
        // require bypassing the fallback chain to call the OpenRouter embedding
        // client directly — out of scope here (touches `embedder.rs`, which is
        // off-limits, and removes the multi-backend fallback safety net).
        // Transient is the conservative choice: a persistently permanent failure
        // still terminates via `--max-attempts` instead of retrying forever.
        AppError::Embedding(_) => AttemptOutcome::Transient,
        // Every other variant — including `Validation` without an
        // origin-typed retry verdict attached — is treated as permanent.
        // Previously this branch inspected the formatted message for
        // substrings like "json" / "missing '" to guess at transience; that
        // guesswork is now unnecessary because the OpenRouter chat path
        // (the project's only supported enrich mode) attaches its retry
        // verdict directly via `ChatError::retry_class`, computed at the
        // exact HTTP status / provider code in `chat_api.rs`, and
        // `record_item_failure_typed` consumes it BEFORE ever falling back
        // to this classifier.
        _ => AttemptOutcome::HardFailure,
    }
}

/// Applies a failure outcome to a single queue row. Shared by the parallel
/// worker and the serial loop (DRY). A `HardFailure`, or a transient failure
/// whose attempt count reached `max_attempts`, lands in the dead-letter status
/// (`status='dead'`) so it is never re-selected. A transient failure below the
/// cap is rescheduled to `pending` with an exponential-backoff `next_retry_at`.
/// Returns the [`crate::retry::AttemptOutcome`] so the caller can feed the
/// existing circuit breaker.
///
/// GAP-SG-73: delegates to [`record_item_failure_typed`] with the outcome
/// computed by the untyped fallback classifier and no diagnostics — the
/// entry point for callers that only have a bare `&AppError` (subprocess
/// providers, persistence failures).
pub(super) fn record_item_failure(
    queue_conn: &rusqlite::Connection,
    queue_id: i64,
    attempt: i64,
    max_attempts: u32,
    err: &AppError,
) -> crate::retry::AttemptOutcome {
    let outcome = classify_enrich_outcome(err);
    let err_str = format!("{err}");
    record_item_failure_typed(
        queue_conn,
        queue_id,
        attempt,
        max_attempts,
        outcome,
        &err_str,
        None,
        None,
        None,
    )
}

/// GAP-SG-72/73: applies a failure outcome to a single queue row using an
/// [`crate::retry::AttemptOutcome`] the caller ALREADY computed at the
/// failure's origin (e.g. `ChatError::retry_class` from an OpenRouter chat
/// call), plus whatever truncation diagnostics (`finish_reason` and token
/// counts) were available. This is the precise counterpart to
/// [`record_item_failure`], which falls back to the untyped
/// [`classify_enrich_outcome`] classifier when no origin-typed verdict
/// exists. Both share this single write path (DRY).
#[allow(clippy::too_many_arguments)]
pub(super) fn record_item_failure_typed(
    queue_conn: &rusqlite::Connection,
    queue_id: i64,
    attempt: i64,
    max_attempts: u32,
    outcome: crate::retry::AttemptOutcome,
    err_str: &str,
    finish_reason: Option<&str>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
) -> crate::retry::AttemptOutcome {
    use crate::retry::AttemptOutcome;
    let error_class = match outcome {
        AttemptOutcome::Transient => "transient",
        AttemptOutcome::HardFailure => "permanent",
        AttemptOutcome::Success => "success",
    };

    let terminal = matches!(outcome, AttemptOutcome::HardFailure) || attempt >= max_attempts as i64;
    if terminal {
        let _ = queue_conn.execute(
            "UPDATE queue SET status='dead', error=?1, error_class=?2, done_at=datetime('now'), \
             finish_reason=?3, input_tokens=?4, output_tokens=?5 WHERE id=?6",
            rusqlite::params![
                err_str,
                error_class,
                finish_reason,
                input_tokens,
                output_tokens,
                queue_id
            ],
        );
    } else {
        let delay = crate::retry::compute_delay(
            &crate::retry::RetryConfig::llm_rate_limit(),
            attempt.max(0) as u32,
        );
        let secs = delay.as_secs().max(1);
        let modifier = format!("+{secs} seconds");
        let _ = queue_conn.execute(
            "UPDATE queue SET status='pending', error=?1, error_class=?2, next_retry_at=datetime('now', ?3), \
             finish_reason=?4, input_tokens=?5, output_tokens=?6 WHERE id=?7",
            rusqlite::params![
                err_str,
                error_class,
                modifier,
                finish_reason,
                input_tokens,
                output_tokens,
                queue_id
            ],
        );
    }
    outcome
}

/// GAP-SG-76: outcome of claiming the next pending queue row. Distinguishes
/// a genuinely empty backlog (`QueryReturnedNoRows`) from lock contention
/// (`SQLITE_BUSY`/`SQLITE_LOCKED`) so the caller retries briefly on the
/// latter instead of breaking out of the drain loop early. Both the serial
/// loop and the parallel worker loop share this (DRY) — previously each
/// collapsed every `query_row` error into `.ok()`, silently treating a busy
/// database the same as an empty queue.
pub(super) enum DequeueOutcome {
    Claimed((i64, String, String, i64)),
    Empty,
}

pub(super) fn dequeue_next_pending(
    queue_conn: &rusqlite::Connection,
    backoff_clause: &str,
) -> Result<DequeueOutcome, AppError> {
    let dequeue_sql = format!(
        "UPDATE queue SET status='processing', attempt=attempt+1 \
         WHERE id = (SELECT id FROM queue WHERE status='pending' {backoff_clause} \
                     ORDER BY id LIMIT 1) \
         RETURNING id, item_key, item_type, attempt"
    );
    match queue_conn.query_row(&dequeue_sql, [], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    }) {
        Ok(claimed) => Ok(DequeueOutcome::Claimed(claimed)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(DequeueOutcome::Empty),
        Err(e) => Err(AppError::Database(e)),
    }
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
    fn classify_validation_never_infers_transience_from_message() {
        // GAP-SG-73: the fallback classifier is TYPED-only now. Messages
        // that used to be sniffed for "json" / "missing '" substrings and
        // treated as Transient are HardFailure here — the OpenRouter chat
        // path (the project's only supported enrich mode) attaches its own
        // typed `ChatError::retry_class` for these exact shape failures
        // BEFORE `record_item_failure_typed` ever falls back to this
        // classifier, so no message-based guessing survives in the fallback.
        for msg in [
            "model 'x' returned non-object JSON after repair (got string)",
            "model 'x' returned content that could not be parsed even after JSON repair",
            "model 'x' returned no structured content",
            "LLM result missing 'description' field",
            "LLM result missing 'enriched_body' field",
        ] {
            assert_eq!(
                classify_enrich_outcome(&AppError::Validation(msg.into())),
                crate::retry::AttemptOutcome::HardFailure,
                "expected hard failure for: {msg}"
            );
        }
    }

    #[test]
    fn classify_embedding_error_is_transient_floor() {
        assert_eq!(
            classify_enrich_outcome(&AppError::Embedding("dimension mismatch".into())),
            crate::retry::AttemptOutcome::Transient
        );
    }

    // GAP-SG-78: entity absence is Transient (own typed variant); memory
    // absence and the untyped NotFound string stay HardFailure. No substring.
    #[test]
    fn classify_entity_not_yet_materialized_is_transient() {
        assert_eq!(
            classify_enrich_outcome(&AppError::EntityNotYetMaterialized {
                name: "acme".into(),
                namespace: "global".into(),
            }),
            crate::retry::AttemptOutcome::Transient
        );
    }

    #[test]
    fn classify_memory_absence_stays_hard_failure() {
        assert_eq!(
            classify_enrich_outcome(&AppError::MemoryNotFound {
                name: "mem-x".into(),
                namespace: "global".into(),
            }),
            crate::retry::AttemptOutcome::HardFailure
        );
        assert_eq!(
            classify_enrich_outcome(&AppError::MemoryNotFoundById { id: 42 }),
            crate::retry::AttemptOutcome::HardFailure
        );
        assert_eq!(
            classify_enrich_outcome(&AppError::NotFound("gone".into())),
            crate::retry::AttemptOutcome::HardFailure
        );
    }

    #[test]
    fn classify_database_busy_is_transient_non_busy_is_hard() {
        let busy = AppError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            Some("database is locked".into()),
        ));
        assert_eq!(
            classify_enrich_outcome(&busy),
            crate::retry::AttemptOutcome::Transient
        );
        let constraint = AppError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
            Some("UNIQUE constraint failed".into()),
        ));
        assert_eq!(
            classify_enrich_outcome(&constraint),
            crate::retry::AttemptOutcome::HardFailure
        );
    }

    #[test]
    fn record_item_failure_typed_persists_diagnostics_on_dead_letter() {
        let (conn, path) = open_temp_queue();
        let id = insert_pending(&conn, "mem-diag");
        let outcome = record_item_failure_typed(
            &conn,
            id,
            1,
            5,
            crate::retry::AttemptOutcome::HardFailure,
            "truncated response",
            Some("length"),
            Some(120),
            Some(4096),
        );
        assert_eq!(outcome, crate::retry::AttemptOutcome::HardFailure);
        let (status, finish_reason, input_tokens, output_tokens): (
            String,
            Option<String>,
            Option<i64>,
            Option<i64>,
        ) = conn
            .query_row(
                "SELECT status, finish_reason, input_tokens, output_tokens FROM queue WHERE id=?1",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(status, "dead");
        assert_eq!(finish_reason.as_deref(), Some("length"));
        assert_eq!(input_tokens, Some(120));
        assert_eq!(output_tokens, Some(4096));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn record_item_failure_typed_reschedules_transient_below_max_attempts() {
        // GAP-SG-72-chat: a transient failure (e.g. a truncated OpenRouter
        // response) below max_attempts must stay `pending` with a
        // future `next_retry_at`, not go straight to `dead` — and it must
        // still persist the finish_reason/token diagnostics for later
        // inspection via `--list-dead` / `--status`.
        let (conn, path) = open_temp_queue();
        let id = insert_pending(&conn, "mem-retry");
        let outcome = record_item_failure_typed(
            &conn,
            id,
            1,
            5,
            crate::retry::AttemptOutcome::Transient,
            "truncated response",
            Some("length"),
            Some(120),
            Some(64),
        );
        assert_eq!(outcome, crate::retry::AttemptOutcome::Transient);
        let (status, error_class, finish_reason, next_retry_at): (
            String,
            String,
            Option<String>,
            Option<String>,
        ) = conn
            .query_row(
                "SELECT status, error_class, finish_reason, next_retry_at FROM queue WHERE id=?1",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(status, "pending");
        assert_eq!(error_class, "transient");
        assert_eq!(finish_reason.as_deref(), Some("length"));
        assert!(
            next_retry_at.is_some(),
            "a rescheduled item must carry a next_retry_at"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// GAP-SG-76/v1.1.00 fix: proves the enrich drain loops' composition
    /// `with_busy_retry(|| dequeue_next_pending(...))` is BOUNDED under
    /// sustained lock contention instead of the previous
    /// `loop { ... continue; }`, which retried `SQLITE_BUSY` forever. A
    /// second connection holds an exclusive write lock for the whole test;
    /// the queue connection under test has `busy_timeout=0` so SQLite
    /// reports `SQLITE_BUSY` immediately instead of blocking internally,
    /// isolating `with_busy_retry`'s own bounded backoff (5 attempts) as the
    /// only source of delay.
    #[test]
    fn with_busy_retry_bounds_dequeue_under_sustained_contention() {
        let (conn, path) = open_temp_queue();
        insert_pending(&conn, "mem-busy");
        conn.pragma_update(None, "busy_timeout", 0i64)
            .expect("busy_timeout override must succeed");

        // Second connection holds an EXCLUSIVE write lock so every dequeue
        // attempt on `conn` observes SQLITE_BUSY, never SQLITE_LOCKED-then-
        // clears-up.
        let blocker = Connection::open(&path).expect("blocker connection must open");
        blocker
            .execute_batch("BEGIN EXCLUSIVE;")
            .expect("exclusive lock must be acquired");

        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let calls_clone = std::sync::Arc::clone(&calls);
        let result: Result<DequeueOutcome, AppError> =
            crate::storage::utils::with_busy_retry(|| {
                calls_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                dequeue_next_pending(&conn, "")
            });

        assert!(
            matches!(result, Err(AppError::DbBusy(_))),
            "sustained SQLITE_BUSY must convert to DbBusy, not hang or silently report Empty"
        );
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            crate::constants::MAX_SQLITE_BUSY_RETRIES,
            "must attempt exactly MAX_SQLITE_BUSY_RETRIES times, never retry unbounded"
        );

        blocker
            .execute_batch("ROLLBACK;")
            .expect("releasing the exclusive lock must succeed");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dequeue_next_pending_distinguishes_empty_from_claimed() {
        let (conn, path) = open_temp_queue();
        let id = insert_pending(&conn, "mem-dequeue");
        let claimed = dequeue_next_pending(&conn, "").expect("dequeue must succeed");
        match claimed {
            DequeueOutcome::Claimed((claimed_id, key, _, _)) => {
                assert_eq!(claimed_id, id);
                assert_eq!(key, "mem-dequeue");
            }
            DequeueOutcome::Empty => panic!("expected a claimed row"),
        }
        let empty = dequeue_next_pending(&conn, "").expect("dequeue must succeed");
        assert!(matches!(empty, DequeueOutcome::Empty));
        let _ = std::fs::remove_file(&path);
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
