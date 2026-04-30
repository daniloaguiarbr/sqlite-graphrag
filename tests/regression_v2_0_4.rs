/// Regression tests guarding the 3 doc/code consistency fixes from v2.0.4 → v2.0.5.
///
/// Inconsistency 1: exit 13 (BatchPartialFailure) and exit 15 (DbBusy) were documented as
///   "exit 13 = Batch partial or DB busy" — they are now documented separately.
///
/// Inconsistency 2: exit 73 appeared in AGENTS.md as "lock busy across slots" but the code
///   uses exit 75 (LockBusy/AllSlotsFull) — the reference to 73 was removed from docs.
///
/// Inconsistency 3: PURGE_RETENTION_DAYS was documented as 30 days but the code uses 90.

// ---------------------------------------------------------------------------
// Regression 1a — BatchPartialFailure MUST have exit code 13 (and not 15)
// ---------------------------------------------------------------------------

#[test]
fn regression_v2_0_4_exit_13_apenas_batch_partial() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::BatchPartialFailure {
        total: 5,
        failed: 2,
    };
    assert_eq!(
        err.exit_code(),
        13,
        "BatchPartialFailure DEVE usar exit 13 — não compartilha exit 13 com DbBusy"
    );
}

// ---------------------------------------------------------------------------
// Regression 1b — DbBusy MUST have exit code 15 (separate from BatchPartialFailure)
// ---------------------------------------------------------------------------

#[test]
fn regression_v2_0_4_exit_15_db_busy() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::DbBusy("database is locked".into());
    assert_eq!(
        err.exit_code(),
        15,
        "DbBusy DEVE usar exit 15 — separado de BatchPartialFailure (13)"
    );
}

#[test]
fn regression_v2_0_4_exit_13_e_15_sao_distintos() {
    use sqlite_graphrag::errors::AppError;
    let batch = AppError::BatchPartialFailure {
        total: 3,
        failed: 1,
    };
    let busy = AppError::DbBusy("lock".into());
    assert_ne!(
        batch.exit_code(),
        busy.exit_code(),
        "BatchPartialFailure (13) e DbBusy (15) DEVEM ter exit codes distintos"
    );
}

// ---------------------------------------------------------------------------
// Regression 2a — LockBusy uses exit 75, NOT 73
// ---------------------------------------------------------------------------

#[test]
fn regression_v2_0_4_exit_75_lock_busy_nao_73() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::LockBusy("outra instância segura o lock".into());
    assert_eq!(err.exit_code(), 75, "LockBusy DEVE usar exit 75 (não 73)");
    assert_ne!(err.exit_code(), 73, "LockBusy NÃO deve usar exit 73");
}

#[test]
fn regression_v2_0_4_exit_75_all_slots_full_nao_73() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::AllSlotsFull {
        max: 4,
        waited_secs: 30,
    };
    assert_eq!(
        err.exit_code(),
        75,
        "AllSlotsFull DEVE usar exit 75 (não 73)"
    );
    assert_ne!(err.exit_code(), 73, "AllSlotsFull NÃO deve usar exit 73");
}

// ---------------------------------------------------------------------------
// Regression 2b — AGENTS.md does not contain references to exit 73
// ---------------------------------------------------------------------------

#[test]
fn regression_v2_0_4_docs_agents_nao_menciona_exit_73() {
    let caminho = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/AGENTS.md");
    let conteudo = std::fs::read_to_string(caminho).expect("docs/AGENTS.md deve existir");
    assert!(
        !conteudo.contains("exit 73")
            && !conteudo.contains("code 73")
            && !conteudo.contains("= 73"),
        "docs/AGENTS.md NÃO deve mencionar exit 73 — o código usa 75 para LockBusy/AllSlotsFull"
    );
}

// ---------------------------------------------------------------------------
// Regression 3 — PURGE_RETENTION_DAYS_DEFAULT is 90, not 30
// ---------------------------------------------------------------------------

#[test]
fn regression_v2_0_4_purge_retention_days_default_eh_90() {
    use sqlite_graphrag::constants::PURGE_RETENTION_DAYS_DEFAULT;
    assert_eq!(
        PURGE_RETENTION_DAYS_DEFAULT, 90,
        "PURGE_RETENTION_DAYS_DEFAULT DEVE ser 90 — documentação foi corrigida de 30 para 90"
    );
}

#[test]
fn regression_v2_0_4_purge_retention_days_nao_eh_30() {
    use sqlite_graphrag::constants::PURGE_RETENTION_DAYS_DEFAULT;
    assert_ne!(
        PURGE_RETENTION_DAYS_DEFAULT, 30,
        "PURGE_RETENTION_DAYS_DEFAULT NÃO deve ser 30 (valor antigo da documentação desatualizada)"
    );
}
