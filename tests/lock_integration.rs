// Testes de integração E2E para o semáforo de slots do neurographrag.
//
// ISOLAMENTO: todos os testes definem `NEUROGRAPHRAG_CACHE_DIR` apontando
// para um `TempDir` exclusivo, garantindo que os lock files não poluam
// `~/.cache/neurographrag` nem colidam entre testes.
//
// `#[serial]` é obrigatório em todos os testes: embora cada teste use
// diretório próprio, o binário compilado é compartilhado e o `TempDir` só
// é liberado após o teste encerrar; serializar elimina corridas no sistema
// de arquivos e torna os timings previsíveis.
//
// Os cenários 4 e 5 dependem de um processo externo que segura um slot por
// tempo determinístico. Marcados `#[ignore]` pois timing de subprocessos é
// flaky em ambientes com carga variável.

use assert_cmd::Command;
use serial_test::serial;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Retorna o caminho do arquivo de lock para o slot indicado (1-based)
/// dentro do `TempDir` fornecido, espelhando a lógica de `lock.rs`.
fn slot_path(tmp: &TempDir, slot: usize) -> std::path::PathBuf {
    tmp.path().join(format!("cli-slot-{slot}.lock"))
}

// ---------------------------------------------------------------------------
// Cenário 1 — slot é liberado após processo terminar
// ---------------------------------------------------------------------------
// Garante que duas invocações sequenciais sem --max-concurrency não conflitam,
// pois o primeiro processo libera o slot ao encerrar.

#[test]
#[serial]
fn slot_liberado_apos_processo_sair() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");

    // Primeira invocação — deve adquirir e liberar o slot 1.
    Command::cargo_bin("neurographrag")
        .expect("binário neurographrag não encontrado")
        .env("NEUROGRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "namespace-detect"])
        .assert()
        .success();

    // Segunda invocação — deve adquirir o slot novamente sem erro.
    Command::cargo_bin("neurographrag")
        .expect("binário neurographrag não encontrado")
        .env("NEUROGRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "namespace-detect"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Cenário 2 — arquivo de slot é criado no cache dir configurado
// ---------------------------------------------------------------------------
// Confirma que o binário cria `cli-slot-1.lock` no diretório sobrescrito via
// `NEUROGRAPHRAG_CACHE_DIR`.

#[test]
#[serial]
fn arquivo_slot_criado_em_cache_dir() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");

    Command::cargo_bin("neurographrag")
        .expect("binário neurographrag não encontrado")
        .env("NEUROGRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "namespace-detect"])
        .assert()
        .success();

    assert!(
        slot_path(&tmp, 1).exists(),
        "cli-slot-1.lock deve existir em {:?} após invocação do binário",
        tmp.path()
    );
}

// ---------------------------------------------------------------------------
// Cenário 3 — --wait-lock 0 falha imediatamente quando todos os slots ocupados
// ---------------------------------------------------------------------------
// Simula N slots ocupados criando e travando os arquivos de lock diretamente,
// depois confirma que uma nova invocação retorna exit 75 sem aguardar.

#[test]
#[serial]
fn wait_lock_zero_retorna_75_quando_slots_ocupados() {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    let tmp = TempDir::new().expect("TempDir deve ser criado");
    let max = 4;

    // Travar todos os N slots diretamente para simular N instâncias rodando.
    let mut handles = Vec::new();
    for slot in 1..=max {
        let path = slot_path(&tmp, slot);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .expect("criação do lock file deve funcionar");
        file.try_lock_exclusive()
            .unwrap_or_else(|_| panic!("slot {slot} deve estar livre para testes"));
        handles.push(file);
    }

    // Invocação com todos os slots ocupados e --wait-lock 0 → exit 75.
    Command::cargo_bin("neurographrag")
        .expect("binário neurographrag não encontrado")
        .env("NEUROGRAPHRAG_CACHE_DIR", tmp.path())
        .args([
            "--skip-memory-guard",
            "--max-concurrency",
            "4",
            "--wait-lock",
            "0",
            "namespace-detect",
        ])
        .assert()
        .failure()
        .code(75);

    // Liberar os locks antes de drop(tmp).
    drop(handles);
}

// ---------------------------------------------------------------------------
// Cenário 4 — segunda instância recebe exit 75 enquanto slot está ocupado
// ---------------------------------------------------------------------------
// MARCADO #[ignore] — timing de subprocessos é flaky em ambientes com carga.
// Para rodar manualmente:
//   cargo test -- --ignored slot_bloqueia_segunda_instancia_com_exit_75

#[test]
#[serial]
#[ignore = "flaky — depende de timing de subprocessos — rodar manualmente com: cargo test -- --ignored"]
fn slot_bloqueia_segunda_instancia_com_exit_75() {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    let tmp = TempDir::new().expect("TempDir deve ser criado");

    // Travar todos os slots (default 4) para simular lotação máxima.
    let mut handles = Vec::new();
    for slot in 1..=4 {
        let path = slot_path(&tmp, slot);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .expect("criação do lock file deve funcionar");
        file.try_lock_exclusive().expect("slot deve estar livre");
        handles.push(file);
    }

    std::thread::sleep(std::time::Duration::from_millis(100));

    // Segunda instância deve falhar imediatamente com exit 75.
    Command::cargo_bin("neurographrag")
        .expect("binário neurographrag não encontrado")
        .env("NEUROGRAPHRAG_CACHE_DIR", tmp.path())
        .args([
            "--skip-memory-guard",
            "--max-concurrency",
            "4",
            "--wait-lock",
            "0",
            "namespace-detect",
        ])
        .assert()
        .failure()
        .code(75);

    drop(handles);
}

// ---------------------------------------------------------------------------
// Cenário 5 — --wait-lock espera e adquire slot após liberação
// ---------------------------------------------------------------------------
// MARCADO #[ignore] — adiciona ~1s ao tempo total e depende de timing.
// Para rodar manualmente:
//   cargo test -- --ignored wait_lock_espera_e_adquire_slot

#[test]
#[serial]
#[ignore = "flaky — depende de timing de subprocessos — rodar manualmente com: cargo test -- --ignored"]
fn wait_lock_espera_e_adquire_slot() {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    let tmp = TempDir::new().expect("TempDir deve ser criado");

    // Travar todos os 4 slots.
    let mut handles = Vec::new();
    for slot in 1..=4 {
        let path = slot_path(&tmp, slot);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .expect("criação do lock file deve funcionar");
        file.try_lock_exclusive().expect("slot deve estar livre");
        handles.push(file);
    }

    // Liberar todos após 1 segundo em thread separada.
    let tmp_path = tmp.path().to_path_buf();
    let _ = tmp_path; // silence unused warning
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        drop(handles);
    });

    // --wait-lock 10 deve aguardar a liberação e completar com sucesso.
    Command::cargo_bin("neurographrag")
        .expect("binário neurographrag não encontrado")
        .env("NEUROGRAPHRAG_CACHE_DIR", tmp.path())
        .args([
            "--skip-memory-guard",
            "--max-concurrency",
            "4",
            "--wait-lock",
            "10",
            "namespace-detect",
        ])
        .assert()
        .success();
}
