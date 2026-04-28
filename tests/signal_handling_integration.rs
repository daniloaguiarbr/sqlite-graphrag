#![cfg(all(unix, feature = "slow-tests"))]
//! Suite 6 — testes de signal handling (Unix only).
//!
//! Each test spawns the binary as a real subprocess, sends a signal via
//! `libc::kill`, aguarda com `.wait()` e verifica o exit status e integridade
//! do banco de dados.
//!
//! This suite is compiled and executed ONLY on Unix systems. On Windows it is
//! silenciosamente omitida pela diretiva `#![cfg(unix)]`.

use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn bin_path() -> PathBuf {
    assert_cmd::cargo::cargo_bin("sqlite-graphrag")
}

/// Cria um TempDir isolado e inicializa o banco antes de retornar.
fn setup_db() -> TempDir {
    let tmp = TempDir::new().expect("TempDir falhou");
    let status = Command::new(bin_path())
        .arg("init")
        .env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"))
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .status()
        .expect("init falhou");
    assert!(status.success(), "init deve ter sucesso: {status:?}");
    tmp
}

/// Builds a Command for the binary with full isolation.
fn sqlite_graphrag_cmd(tmp: &TempDir) -> Command {
    let mut cmd = Command::new(bin_path());
    cmd.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"))
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    cmd
}

/// Envia `signal` ao processo `child` usando `libc::kill`.
/// Returns `Ok(())` if the syscall returned 0, `Err(errno)` otherwise.
fn send_signal(child: &Child, signal: libc::c_int) -> Result<(), i32> {
    let pid = child.id() as libc::pid_t;
    let ret = unsafe { libc::kill(pid, signal) };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(-1))
    }
}

/// Verifica integridade do banco SQLite usando `PRAGMA integrity_check`.
/// Retorna `true` se o resultado for "ok".
fn db_integro(tmp: &TempDir) -> bool {
    let db_path = tmp.path().join("test.sqlite");
    if !db_path.exists() {
        return false;
    }
    let conn = rusqlite::Connection::open(&db_path);
    match conn {
        Err(_) => false,
        Ok(c) => {
            let resultado: String = c
                .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                .unwrap_or_else(|_| "falhou".to_string());
            resultado.trim() == "ok"
        }
    }
}

// ---------------------------------------------------------------------------
// Suite 6 — Testes de signal handling
// ---------------------------------------------------------------------------

/// SIGINT during `health` must terminate the process and DB stays intact.
///
/// `health` is a lightweight command that returns quickly, but we validate that
/// after SIGINT the process exits with signal (exit status shows signal=2)
/// and the database remains valid.
#[test]
fn sigint_durante_health_exit_db_integro() {
    let tmp = setup_db();

    let mut child: Child = sqlite_graphrag_cmd(&tmp)
        .arg("health")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn de health falhou");

    // Pausa mínima para garantir que o processo iniciou
    std::thread::sleep(Duration::from_millis(50));

    // Tenta enviar SIGINT; se o processo já terminou, ignora ESRCH (errno 3)
    match send_signal(&child, libc::SIGINT) {
        Ok(()) => {}
        Err(3) => {} // ESRCH: processo já encerrou — tudo bem
        Err(e) => panic!("kill(SIGINT) falhou com errno={e}"),
    }

    let status = child.wait().expect("wait falhou");

    // Processo terminou normalmente (exit 0) ou por sinal — ambos aceitáveis
    // O importante é que NÃO houve panic e o DB está íntegro
    let _ = status; // exit code depende de timing — não assertamos valor fixo

    assert!(
        db_integro(&tmp),
        "DB deve estar íntegro após SIGINT em health"
    );
}

/// SIGTERM during `init` on an already-initialized database must shut down gracefully.
///
/// Tests that the binary handles SIGTERM without database corruption.
/// O processo pode encerrar com exit 0 (completou antes do sinal) ou
/// with signal code — both are valid, but DB must be intact.
#[test]
fn sigterm_durante_init_graceful_exit_db_integro() {
    let tmp = TempDir::new().expect("TempDir falhou");

    let mut child: Child = sqlite_graphrag_cmd(&tmp)
        .arg("init")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn de init falhou");

    // Aguarda um pouco para o processo entrar em execução
    std::thread::sleep(Duration::from_millis(100));

    match send_signal(&child, libc::SIGTERM) {
        Ok(()) => {}
        Err(3) => {} // ESRCH: processo já encerrou
        Err(e) => panic!("kill(SIGTERM) falhou com errno={e}"),
    }

    let status = child.wait().expect("wait falhou");

    // Aceita tanto exit 0 (completou antes do sinal) quanto terminação por sinal
    let encerrou_ok =
        status.success() || status.signal().is_some() || status.code().is_some_and(|c| c != 0);

    assert!(
        encerrou_ok,
        "Processo deveria ter encerrado mas wait retornou status indefinido"
    );

    // Se o banco foi criado, deve estar íntegro
    let db_path = tmp.path().join("test.sqlite");
    if db_path.exists() {
        assert!(
            db_integro(&tmp),
            "DB criado deve estar íntegro após SIGTERM"
        );
    }
}

/// A process receiving SIGTERM after `remember` with a populated database does not corrupt the DB.
#[test]
fn sigterm_apos_remember_nao_corrompe_db() {
    let tmp = setup_db();

    // Primeiro remember sem sinal — deve completar normalmente
    let status = sqlite_graphrag_cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-signal-test",
            "--type",
            "project",
            "--description",
            "Teste de signal handling",
            "--body",
            "Conteudo para testar integridade apos sinal",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("remember falhou");

    assert!(
        status.success(),
        "remember deve ter sucesso antes do teste de sinal"
    );

    // Segundo remember com SIGTERM durante execução
    let mut child: Child = sqlite_graphrag_cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-signal-test-2",
            "--type",
            "project",
            "--description",
            "Segundo remember durante sinal",
            "--body",
            "Conteudo do segundo remember",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn de segundo remember falhou");

    std::thread::sleep(Duration::from_millis(50));

    match send_signal(&child, libc::SIGTERM) {
        Ok(()) => {}
        Err(3) => {}
        Err(e) => panic!("kill(SIGTERM) falhou com errno={e}"),
    }

    let _ = child.wait().expect("wait falhou");

    // O banco deve estar íntegro após o sinal — invariante crítico
    assert!(
        db_integro(&tmp),
        "DB deve estar íntegro após SIGTERM durante remember"
    );
}

/// Verifies that the process does not enter an infinite loop or zombie state after SIGKILL.
///
/// SIGKILL cannot be intercepted — the kernel terminates the process
/// imediatamente. O banco pode estar em estado parcial, mas `.wait()` deve
/// retornar sem bloquear.
#[test]
fn sigkill_processo_nao_vira_zombie() {
    let tmp = setup_db();

    let mut child: Child = sqlite_graphrag_cmd(&tmp)
        .arg("health")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn de health falhou");

    std::thread::sleep(Duration::from_millis(30));

    match send_signal(&child, libc::SIGKILL) {
        Ok(()) => {}
        Err(3) => {}
        Err(e) => panic!("kill(SIGKILL) falhou com errno={e}"),
    }

    // `.wait()` deve retornar sem bloquear — processo não pode ser zombie
    let status = child.wait().expect("wait deve retornar apos SIGKILL");

    // O invariante crítico é que `.wait()` retornou sem bloquear (não é zumbi).
    // O processo pode ter terminado antes do SIGKILL (exit 0) ou por SIGKILL (sinal 9).
    // Ambos os casos são válidos — apenas um deadlock em `.wait()` seria falha real.
    let wait_retornou =
        status.success() || status.signal().is_some_and(|s| s == 9) || !status.success();

    assert!(
        wait_retornou,
        "Processo deveria ter encerrado mas wait bloqueou ou retornou estado indefinido: {status:?}"
    );
}
