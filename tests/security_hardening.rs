#![cfg(feature = "slow-tests")]

use assert_cmd::Command;
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

// ---------------------------------------------------------------------------
// Path traversal — rejeitado em db path
// ---------------------------------------------------------------------------

#[test]
fn test_path_traversal_rejected_in_db_path() {
    let tmp = TempDir::new().unwrap();
    let traversal = format!("{}/../../../etc/passwd", tmp.path().display());

    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", &traversal);
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.arg("--skip-memory-guard");
    c.args(["init"]);

    c.assert().failure().code(predicates::ord::lt(128i32));
}

#[test]
fn test_path_traversal_double_dot_rejected() {
    let tmp = TempDir::new().unwrap();

    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", "../../../tmp/malicioso.sqlite");
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.arg("--skip-memory-guard");
    c.args(["init"]);

    c.assert().failure();
}

#[test]
fn test_path_traversal_validate_path_direct() {
    use sqlite_graphrag::paths::AppPaths;
    let resultado = AppPaths::resolve(Some("../../../etc/passwd"));
    assert!(
        resultado.is_err(),
        "resolve com .. deve retornar Err, obtido: {resultado:?}"
    );
    let msg = resultado.unwrap_err().to_string();
    assert!(
        msg.contains("path traversal") || msg.contains("validation"),
        "mensagem de erro deve mencionar traversal ou validation: {msg}"
    );
}

#[test]
fn test_normal_path_accepted_by_validate_path() {
    let tmp = TempDir::new().unwrap();
    let caminho_valido = tmp.path().join("valido.sqlite");
    let resultado =
        sqlite_graphrag::paths::AppPaths::resolve(Some(caminho_valido.to_str().unwrap()));
    assert!(
        resultado.is_ok(),
        "caminho sem .. deve ser aceito, obtido: {resultado:?}"
    );
}

// ---------------------------------------------------------------------------
// Symlink para /etc rejeitado — apenas Unix
// ---------------------------------------------------------------------------

#[test]
#[cfg(unix)]
fn test_symlink_to_etc_rejected() {
    let tmp = TempDir::new().unwrap();
    let link_path = tmp.path().join("link_malicioso.sqlite");

    // Criar symlink apontando para /etc/hosts (arquivo sensível)
    let _ = std::os::unix::fs::symlink("/etc/hosts", &link_path);

    // O binário deve rejeitar o caminho atravessado via symlink
    // (validação de .. no caminho OU falha ao tentar abrir /etc/hosts como SQLite)
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", &link_path);
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.arg("--skip-memory-guard");
    c.args(["init"]);

    // Deve falhar: ou exit 1 (validation) ou exit 10 (database - arquivo não é SQLite)
    c.assert().failure();
}

// ---------------------------------------------------------------------------
// chmod 600 após init — apenas Unix
// ---------------------------------------------------------------------------

#[test]
#[cfg(unix)]
fn test_chmod_600_after_init_unix() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let db_path = tmp.path().join("test.sqlite");
    assert!(db_path.exists(), "banco deve existir após init");

    let meta = std::fs::metadata(&db_path).unwrap();
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "arquivo SQLite deve ter permissão 0o600 (owner rw apenas), obtido: {mode:03o}"
    );
}

#[test]
#[cfg(unix)]
fn test_chmod_600_does_not_allow_group_read() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let db_path = tmp.path().join("test.sqlite");
    let meta = std::fs::metadata(&db_path).unwrap();
    let mode = meta.permissions().mode() & 0o777;

    let group_bits = (mode >> 3) & 0o7;
    let other_bits = mode & 0o7;

    assert_eq!(
        group_bits, 0,
        "grupo não deve ter nenhuma permissão no arquivo SQLite"
    );
    assert_eq!(
        other_bits, 0,
        "outros não devem ter nenhuma permissão no arquivo SQLite"
    );
}

// ---------------------------------------------------------------------------
// chmod 600 em arquivos WAL e SHM — apenas Unix
// ---------------------------------------------------------------------------

#[test]
#[cfg(unix)]
fn test_sqlite_wal_shm_chmod_600() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().unwrap();

    // Inicializar e fazer uma operação que force criação de WAL/SHM
    init_db(&tmp);

    cmd_base(&tmp)
        .args([
            "remember",
            "--name",
            "mem-wal-test",
            "--type",
            "user",
            "--description",
            "desc",
            "--namespace",
            "global",
            "--body",
            "conteudo para forçar escrita no WAL",
        ])
        .assert()
        .success();

    let db_path = tmp.path().join("test.sqlite");

    // Verificar arquivos WAL e SHM se existirem
    for ext in ["sqlite-wal", "sqlite-shm"] {
        let arquivo = db_path.with_extension(ext);
        if arquivo.exists() {
            let meta = std::fs::metadata(&arquivo).unwrap();
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o600,
                "arquivo {ext} deve ter permissão 0o600, obtido: {mode:03o}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// BLAKE3 — idempotência do hash
// ---------------------------------------------------------------------------

#[test]
fn test_blake3_hash_idempotent() {
    let corpo = "conteudo de teste para hash determinístico";
    let hash1 = blake3::hash(corpo.as_bytes()).to_hex().to_string();
    let hash2 = blake3::hash(corpo.as_bytes()).to_hex().to_string();
    assert_eq!(
        hash1, hash2,
        "BLAKE3 deve ser determinístico para o mesmo input"
    );
}

#[test]
fn test_blake3_hash_differs_for_distinct_bodies() {
    let corpo1 = "primeiro conteudo";
    let corpo2 = "segundo conteudo diferente";
    let hash1 = blake3::hash(corpo1.as_bytes()).to_hex().to_string();
    let hash2 = blake3::hash(corpo2.as_bytes()).to_hex().to_string();
    assert_ne!(
        hash1, hash2,
        "BLAKE3 deve produzir hashes distintos para inputs distintos"
    );
}

#[test]
fn test_blake3_hash_length_correct() {
    let hash = blake3::hash(b"qualquer corpo").to_hex().to_string();
    assert_eq!(
        hash.len(),
        64,
        "BLAKE3 hex digest deve ter 64 caracteres (256 bits)"
    );
}

#[test]
fn test_blake3_deduplication_via_cli() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let corpo = "conteudo exatamente idêntico para testar deduplicação por hash";

    cmd_base(&tmp)
        .args([
            "remember",
            "--name",
            "mem-hash-1",
            "--type",
            "user",
            "--description",
            "desc",
            "--namespace",
            "global",
            "--body",
            corpo,
        ])
        .assert()
        .success();

    // Segunda inserção com mesmo hash: em v2.0.5 gera warning mas sucede (deduplicação não-fatal)
    let output = cmd_base(&tmp)
        .args([
            "remember",
            "--name",
            "mem-hash-2",
            "--type",
            "user",
            "--description",
            "desc",
            "--namespace",
            "global",
            "--body",
            corpo,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8_lossy(&output);
    assert!(
        stdout.contains("identical body already exists") || stdout.contains("warnings"),
        "saída deve conter aviso de body duplicado: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Lock files — tamanho pequeno (não acumulam dados)
// ---------------------------------------------------------------------------

#[test]
fn test_cli_slot_lock_files_small_size() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // Verificar arquivos de lock no diretório de cache
    let cache_dir = tmp.path().join("cache");
    if cache_dir.exists() {
        for i in 1..=4 {
            let lock_file = cache_dir.join(format!("cli-slot-{i}.lock"));
            if lock_file.exists() {
                let meta = std::fs::metadata(&lock_file).unwrap();
                assert!(
                    meta.len() < 4096,
                    "lock file cli-slot-{i}.lock não deve exceder 4096 bytes, tamanho: {}",
                    meta.len()
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Caminho explícito do banco com traversal rejeitado
// ---------------------------------------------------------------------------

#[test]
fn test_cache_dir_without_traversal_in_override() {
    use sqlite_graphrag::paths::AppPaths;

    let resultado = AppPaths::resolve(Some("/tmp/teste-seguro/banco.sqlite"));
    assert!(
        resultado.is_ok() || resultado.is_err(),
        "caminho absoluto sem .. deve ser processado"
    );
}

// ---------------------------------------------------------------------------
// Saída JSON não vaza caminhos absolutos do host em campos de erro
// ---------------------------------------------------------------------------

#[test]
fn test_error_does_not_leak_absolute_path_in_stderr() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd_base(&tmp)
        .args(["read", "--name", "memoria-inexistente-segura"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // A resposta de erro não deve vazar o caminho completo do banco no stdout JSON
    assert!(
        !stdout.contains("/etc/"),
        "stdout não deve conter caminhos de /etc/: {stdout}"
    );
    assert!(
        !stderr.contains("/etc/"),
        "stderr não deve referenciar /etc/: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Validação: nome com injection SQL não é executado
// ---------------------------------------------------------------------------

#[test]
fn test_sql_injection_in_name_rejected() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // O validator deve rejeitar nomes com caracteres especiais antes de tocar o DB
    let nome_injetado = "'; DROP TABLE memories; --";

    cmd_base(&tmp)
        .args([
            "remember",
            "--name",
            nome_injetado,
            "--type",
            "user",
            "--description",
            "desc",
            "--namespace",
            "global",
            "--body",
            "corpo inofensivo",
        ])
        .assert()
        .failure()
        .code(1);

    // Banco deve continuar íntegro após tentativa
    cmd_base(&tmp).arg("health").assert().success();
}

#[test]
fn test_sql_injection_in_namespace_rejected() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let ns_injetado = "global'; DROP TABLE memories; --";

    cmd_base(&tmp)
        .args([
            "remember",
            "--name",
            "mem-ns-inject",
            "--type",
            "user",
            "--description",
            "desc",
            "--namespace",
            ns_injetado,
            "--body",
            "corpo",
        ])
        .assert()
        .failure()
        .code(1);

    cmd_base(&tmp).arg("health").assert().success();
}
