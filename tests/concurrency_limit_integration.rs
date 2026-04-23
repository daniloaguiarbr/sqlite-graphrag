// Testes E2E de limite de concorrência para o semáforo de slots do sqlite-graphrag.
//
// ISOLAMENTO: `SQLITE_GRAPHRAG_CACHE_DIR` aponta para um `TempDir` exclusivo
// por teste. `#[serial]` é obrigatório em todos os testes para evitar corridas
// no sistema de arquivos entre testes que usam o mesmo binário compilado.
//
// `--skip-memory-guard` é usado em todos os testes para que a verificação de
// RAM disponível não aborte antes do semáforo ser exercitado.

use assert_cmd::Command;
use serial_test::serial;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Teste 1 — limite de concorrência é respeitado sob carga de 10 processos
// ---------------------------------------------------------------------------
// Dispara 10 invocações paralelas com --max-concurrency 4 e --wait-lock 30.
// Verifica que TODAS completam com sucesso (os 6 que não conseguem slot ficam
// aguardando em polling até um dos 4 iniciais terminar e liberar o slot).

#[test]
#[serial]
fn limite_respeitado_sob_carga() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");
    let bin = assert_cmd::cargo::cargo_bin("sqlite-graphrag");

    // Spawna 10 invocações em paralelo usando std::process::Command para
    // controle direto sobre PIDs (assert_cmd não expõe spawn).
    let handles: Vec<_> = (0..10)
        .map(|_| {
            std::process::Command::new(&bin)
                .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
                .args([
                    "--skip-memory-guard",
                    "--max-concurrency",
                    "4",
                    "--wait-lock",
                    "30",
                    "namespace-detect",
                ])
                .spawn()
                .expect("falha ao spawnar invocação paralela")
        })
        .collect();

    // Aguarda todas e coleta exit codes.
    let resultados: Vec<_> = handles
        .into_iter()
        .map(|h| h.wait_with_output().expect("wait falhou"))
        .collect();

    let successos = resultados.iter().filter(|r| r.status.success()).count();
    let falhas = resultados.iter().filter(|r| !r.status.success()).count();

    // Todas as 10 invocações devem completar com sucesso quando --wait-lock=30.
    assert_eq!(
        successos, 10,
        "todas as 10 invocações devem completar com sucesso (--wait-lock 30), \
         obtivemos {successos} sucessos e {falhas} falhas"
    );
}

// ---------------------------------------------------------------------------
// Teste 2 — --max-concurrency 0 é rejeitado com exit 2
// ---------------------------------------------------------------------------
// Valida que o guard de validação em `Cli::validate_flags` rejeita N=0
// antes de tentar adquirir qualquer slot.

#[test]
#[serial]
fn max_concurrency_zero_rejeitado_com_exit_2() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");

    Command::cargo_bin("sqlite-graphrag")
        .expect("binário sqlite-graphrag não encontrado")
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args([
            "--skip-memory-guard",
            "--max-concurrency",
            "0",
            "namespace-detect",
        ])
        .assert()
        .failure()
        .code(2);
}

// ---------------------------------------------------------------------------
// Teste 3 — todos os slots ocupados retornam exit 75 sem espera
// ---------------------------------------------------------------------------
// Ocupa N slots diretamente via fs4 e verifica que invocação com --wait-lock 0
// retorna exit 75 (AllSlotsFull) imediatamente sem timeout.

#[test]
#[serial]
fn todos_slots_ocupados_retornam_75() {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    let tmp = TempDir::new().expect("TempDir deve ser criado");
    let max: usize = 4;

    // Travar todos os slots diretamente para simular 4 instâncias ativas.
    let mut handles = Vec::new();
    for slot in 1..=max {
        let path = tmp.path().join(format!("cli-slot-{slot}.lock"));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .expect("criação de lock file deve funcionar");
        file.try_lock_exclusive()
            .unwrap_or_else(|_| panic!("slot {slot} deve estar livre antes do teste"));
        handles.push(file);
    }

    // Invocação com --wait-lock 0 deve falhar imediatamente com exit 75.
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário sqlite-graphrag não encontrado")
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
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

    // Libera os locks antes de drop(tmp).
    drop(handles);
}

// ---------------------------------------------------------------------------
// Teste 4 — --skip-memory-guard bypassa a verificação de memória disponível
// ---------------------------------------------------------------------------
// Verifica que `--skip-memory-guard` permite que o comando execute mesmo sem
// passar pela verificação de RAM. Sem a flag, o comando poderia falhar com
// exit 77 em ambientes CI com pouca memória disponível. Com a flag, deve
// completar normalmente independente da RAM disponível.

#[test]
#[serial]
fn skip_memory_guard_bypassa_verificacao_de_ram() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");

    // Com --skip-memory-guard, o comando deve completar com sucesso mesmo em
    // ambientes onde a RAM disponível poderia causar exit 77.
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário sqlite-graphrag não encontrado")
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "namespace-detect"])
        .assert()
        .success();
}
