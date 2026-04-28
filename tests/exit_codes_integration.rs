#![cfg(feature = "slow-tests")]

use assert_cmd::Command;
use serial_test::serial;
use tempfile::TempDir;

fn cmd_base(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.arg("--skip-memory-guard");
    c
}

fn init_db(tmp: &TempDir) {
    cmd_base(tmp).arg("init").assert().success();
}

fn remember_ok(tmp: &TempDir, name: &str, body: &str) {
    cmd_base(tmp)
        .args([
            "remember",
            "--name",
            name,
            "--type",
            "user",
            "--description",
            "desc",
            "--namespace",
            "global",
            "--body",
            body,
        ])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Exit code 1 — Validation
// ---------------------------------------------------------------------------

#[test]
fn test_exit_01_validation_nome_invalido() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_base(&tmp)
        .args([
            "remember",
            "--name",
            "NOME_INVALIDO_UPPERCASE",
            "--type",
            "user",
            "--description",
            "desc",
            "--body",
            "corpo de teste",
        ])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn test_exit_01_validation_namespace_invalido() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // Namespace com espaços é inválido (deve conter apenas alfanuméricos + hífens/underscores)
    cmd_base(&tmp)
        .args([
            "remember",
            "--namespace",
            "namespace com espaco",
            "--name",
            "mem-valida",
            "--type",
            "user",
            "--description",
            "desc",
            "--body",
            "corpo",
        ])
        .assert()
        .failure()
        .code(1);
}

// ---------------------------------------------------------------------------
// Exit code 2 — Duplicate
// ---------------------------------------------------------------------------

#[test]
fn test_exit_02_duplicate_memoria_repetida() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    remember_ok(&tmp, "mem-duplicada", "primeiro conteudo unico aqui");

    // Duplicate é disparado quando o NOME já existe no namespace (sem --force-merge)
    cmd_base(&tmp)
        .args([
            "remember",
            "--name",
            "mem-duplicada",
            "--type",
            "user",
            "--description",
            "desc",
            "--namespace",
            "global",
            "--body",
            "outro corpo diferente",
        ])
        .assert()
        .failure()
        .code(2);
}

// ---------------------------------------------------------------------------
// Exit code 3 — Conflict (optimistic update)
// ---------------------------------------------------------------------------

#[test]
fn test_exit_03_conflict_updated_at_stale() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    remember_ok(&tmp, "mem-conflito", "conteudo inicial para conflito");

    let out = cmd_base(&tmp)
        .args(["read", "--name", "mem-conflito"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let updated_at = json["updated_at"]
        .as_str()
        .unwrap_or("1970-01-01T00:00:00Z")
        .to_owned();

    cmd_base(&tmp)
        .args([
            "edit",
            "--name",
            "mem-conflito",
            "--body",
            "novo corpo para conflito",
        ])
        .assert()
        .success();

    cmd_base(&tmp)
        .args([
            "edit",
            "--name",
            "mem-conflito",
            "--body",
            "segunda edicao com timestamp stale",
            "--expected-updated-at",
            &updated_at,
        ])
        .assert()
        .failure()
        .code(3);
}

// ---------------------------------------------------------------------------
// Exit code 4 — Not Found
// ---------------------------------------------------------------------------

#[test]
fn test_exit_04_not_found_memoria_ausente() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_base(&tmp)
        .args(["read", "--name", "memoria-inexistente-xyz"])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn test_exit_04_not_found_forget_inexistente() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_base(&tmp)
        .args(["forget", "--name", "nao-existe-jamais"])
        .assert()
        .failure()
        .code(4);
}

// ---------------------------------------------------------------------------
// Exit code 5 — NamespaceError (testado via exit_code() da variante)
// ---------------------------------------------------------------------------

#[test]
fn test_exit_05_namespace_error_exit_code_correto() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::NamespaceError("limite excedido".into());
    assert_eq!(err.exit_code(), 5, "NamespaceError deve mapear para exit 5");
}

// ---------------------------------------------------------------------------
// Exit code 6 — LimitExceeded
// ---------------------------------------------------------------------------

#[test]
fn test_exit_06_limit_exceeded_exit_code_correto() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::LimitExceeded("body excede limite de 512000 bytes".into());
    assert_eq!(err.exit_code(), 6, "LimitExceeded deve mapear para exit 6");
}

#[test]
fn test_exit_06_limit_exceeded_body_gigante_via_cli() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let corpo_gigante = "a".repeat(512_001);
    let body_path = tmp.path().join("body-grande.txt");
    std::fs::write(&body_path, corpo_gigante).unwrap();

    cmd_base(&tmp)
        .args([
            "remember",
            "--name",
            "mem-grande",
            "--type",
            "user",
            "--description",
            "desc",
            "--namespace",
            "global",
            "--body-file",
            body_path.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .code(6);
}

#[test]
fn remember_name_over_80_bytes_returns_exit_6() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let long_name = "a".repeat(81);
    cmd_base(&tmp)
        .args([
            "remember",
            "--name",
            &long_name,
            "--type",
            "project",
            "--description",
            "x",
            "--namespace",
            "global",
            "--body",
            "y",
        ])
        .assert()
        .failure()
        .code(6);
}

// ---------------------------------------------------------------------------
// Exit code 10 — Database (DB corrompido)
// ---------------------------------------------------------------------------

#[test]
fn test_exit_10_database_arquivo_corrompido() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("corrompido.sqlite");

    std::fs::write(&db_path, b"isto nao e um sqlite valido!!!").unwrap();

    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", &db_path);
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.arg("--skip-memory-guard");
    c.args(["health"]);

    c.assert().failure();
}

// ---------------------------------------------------------------------------
// Exit code 11 — Embedding
// ---------------------------------------------------------------------------

#[test]
fn test_exit_11_embedding_exit_code_correto() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::Embedding("falha no modelo de embedding".into());
    assert_eq!(err.exit_code(), 11, "Embedding deve mapear para exit 11");
}

// ---------------------------------------------------------------------------
// Exit code 12 — VecExtension
// ---------------------------------------------------------------------------

#[test]
fn test_exit_12_vec_extension_exit_code_correto() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::VecExtension("falha na extensao vec".into());
    assert_eq!(err.exit_code(), 12, "VecExtension deve mapear para exit 12");
}

// ---------------------------------------------------------------------------
// Exit code 13 — BatchPartialFailure
// ---------------------------------------------------------------------------

#[test]
fn test_exit_13_batch_partial_failure_exit_code_correto() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::BatchPartialFailure {
        total: 10,
        failed: 3,
    };
    assert_eq!(
        err.exit_code(),
        13,
        "BatchPartialFailure deve mapear para exit 13"
    );
}

// ---------------------------------------------------------------------------
// Exit code 14 — IO (diretório sem permissão de escrita, Unix only)
// ---------------------------------------------------------------------------

#[test]
#[cfg(unix)]
fn test_exit_14_io_sem_permissao_escrita() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().unwrap();
    let dir_sem_perm = tmp.path().join("readonly");
    std::fs::create_dir_all(&dir_sem_perm).unwrap();
    std::fs::set_permissions(&dir_sem_perm, std::fs::Permissions::from_mode(0o444)).unwrap();

    let db_path = dir_sem_perm.join("test.sqlite");

    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", &db_path);
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.arg("--skip-memory-guard");
    c.args(["init"]);

    c.assert().failure();

    std::fs::set_permissions(&dir_sem_perm, std::fs::Permissions::from_mode(0o755)).unwrap();
}

// ---------------------------------------------------------------------------
// Exit code 15 — DbBusy
// ---------------------------------------------------------------------------

#[test]
fn test_exit_15_db_busy_exit_code_correto() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::DbBusy("retries esgotados".into());
    assert_eq!(err.exit_code(), 15, "DbBusy deve mapear para exit 15");
}

// ---------------------------------------------------------------------------
// Exit code 75 — LockBusy / AllSlotsFull
// ---------------------------------------------------------------------------

#[test]
fn test_exit_75_lock_busy_exit_code_correto() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::LockBusy("outra instancia ativa".into());
    assert_eq!(err.exit_code(), 75, "LockBusy deve mapear para exit 75");
}

#[test]
fn test_exit_75_all_slots_full_exit_code_correto() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::AllSlotsFull {
        max: 4,
        waited_secs: 0,
    };
    assert_eq!(err.exit_code(), 75, "AllSlotsFull deve mapear para exit 75");
}

// ---------------------------------------------------------------------------
// Exit code 77 — LowMemory
// ---------------------------------------------------------------------------

#[test]
fn test_exit_77_low_memory_exit_code_correto() {
    use sqlite_graphrag::errors::AppError;
    let err = AppError::LowMemory {
        available_mb: 512,
        required_mb: 2048,
    };
    assert_eq!(err.exit_code(), 77, "LowMemory deve mapear para exit 77");
}

#[test]
fn test_exit_77_low_memory_guard_direto() {
    use sqlite_graphrag::memory_guard::check_available_memory;
    let resultado = check_available_memory(u64::MAX);
    assert!(
        matches!(
            resultado,
            Err(sqlite_graphrag::errors::AppError::LowMemory { .. })
        ),
        "check_available_memory com u64::MAX deve retornar LowMemory"
    );
}

// ---------------------------------------------------------------------------
// Exit code 0 — Sucesso
// ---------------------------------------------------------------------------

#[test]
fn test_exit_00_sucesso_init_retorna_zero() {
    let tmp = TempDir::new().unwrap();
    cmd_base(&tmp).arg("init").assert().success().code(0);
}

#[test]
fn test_exit_00_sucesso_health_apos_init() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);
    cmd_base(&tmp).arg("health").assert().success().code(0);
}

// ---------------------------------------------------------------------------
// Consistência entre constantes e exit_code()
// ---------------------------------------------------------------------------

#[test]
fn test_constantes_exit_codes_alinhadas() {
    use sqlite_graphrag::constants::{
        BATCH_PARTIAL_FAILURE_EXIT_CODE, CLI_LOCK_EXIT_CODE, DB_BUSY_EXIT_CODE,
        LOW_MEMORY_EXIT_CODE,
    };
    use sqlite_graphrag::errors::AppError;

    assert_eq!(
        AppError::BatchPartialFailure {
            total: 1,
            failed: 1
        }
        .exit_code(),
        BATCH_PARTIAL_FAILURE_EXIT_CODE
    );
    assert_eq!(AppError::DbBusy("x".into()).exit_code(), DB_BUSY_EXIT_CODE);
    assert_eq!(
        AppError::LockBusy("x".into()).exit_code(),
        CLI_LOCK_EXIT_CODE
    );
    assert_eq!(
        AppError::LowMemory {
            available_mb: 1,
            required_mb: 2
        }
        .exit_code(),
        LOW_MEMORY_EXIT_CODE
    );
}

// ---------------------------------------------------------------------------
// Mensagens de erro são não-vazias em PT e EN
// ---------------------------------------------------------------------------

#[test]
fn test_exit_codes_mensagens_nao_vazias_em_todos_idiomas() {
    use sqlite_graphrag::errors::AppError;
    use sqlite_graphrag::i18n::Language;

    let variantes: Vec<AppError> = vec![
        AppError::Validation("campo".into()),
        AppError::Duplicate("ns/mem".into()),
        AppError::Conflict("stale".into()),
        AppError::NotFound("id".into()),
        AppError::NamespaceError("sem ns".into()),
        AppError::LimitExceeded("limite".into()),
        AppError::Embedding("dim".into()),
        AppError::VecExtension("falha".into()),
        AppError::DbBusy("busy".into()),
        AppError::BatchPartialFailure {
            total: 5,
            failed: 2,
        },
        AppError::LockBusy("lock".into()),
        AppError::AllSlotsFull {
            max: 4,
            waited_secs: 10,
        },
        AppError::LowMemory {
            available_mb: 100,
            required_mb: 2048,
        },
    ];

    for variante in variantes {
        let msg_en = variante.localized_message_for(Language::English);
        let msg_pt = variante.localized_message_for(Language::Portugues);
        assert!(!msg_en.is_empty(), "mensagem EN vazia para: {variante:?}");
        assert!(!msg_pt.is_empty(), "mensagem PT vazia para: {variante:?}");
    }
}

// ---------------------------------------------------------------------------
// Fluxo completo remember → read → edit → forget
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_fluxo_remember_edit_read_forget() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    remember_ok(&tmp, "mem-fluxo-ok", "corpo do fluxo completo");

    let out = cmd_base(&tmp)
        .args(["read", "--name", "mem-fluxo-ok"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(json["name"].as_str().unwrap_or(""), "mem-fluxo-ok");

    cmd_base(&tmp)
        .args(["edit", "--name", "mem-fluxo-ok", "--body", "corpo editado"])
        .assert()
        .success();

    cmd_base(&tmp)
        .args(["forget", "--name", "mem-fluxo-ok"])
        .assert()
        .success();

    cmd_base(&tmp)
        .args(["read", "--name", "mem-fluxo-ok"])
        .assert()
        .failure()
        .code(4);
}
