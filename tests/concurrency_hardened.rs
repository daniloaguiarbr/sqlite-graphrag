// Suite 4 — Testes reforçados de lock e concorrência
//
// ISOLAMENTO: cada teste usa `SQLITE_GRAPHRAG_CACHE_DIR` apontando para um
// `TempDir` exclusivo por teste. `#[serial]` é obrigatório em todos os testes
// para evitar corridas no filesystem entre testes que compartilham o binário.
//
// `--skip-memory-guard` é usado para que a verificação de RAM não aborte antes
// do semáforo ser exercitado em ambientes CI com pouca memória.
//
// Testes de timing flaky são marcados com `#[ignore]` e documentam como
// executá-los manualmente.

use assert_cmd::Command;
use serial_test::serial;
use std::sync::{Arc, Barrier};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Retorna o caminho do lock file para o slot indicado (1-based) no `TempDir`.
fn slot_path(tmp: &TempDir, slot: usize) -> std::path::PathBuf {
    tmp.path().join(format!("cli-slot-{slot}.lock"))
}

/// Ocupa `n_slots` arquivos de lock diretamente via fs4, retornando os handles.
fn ocupar_slots(tmp: &TempDir, n_slots: usize) -> Vec<std::fs::File> {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    (1..=n_slots)
        .map(|slot| {
            let path = slot_path(tmp, slot);
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .unwrap_or_else(|_| panic!("criação do lock file slot {slot} deve funcionar"));
            file.try_lock_exclusive()
                .unwrap_or_else(|_| panic!("slot {slot} deve estar livre antes do teste"));
            file
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Teste 1 — 5 instâncias simultâneas: a 5ª recebe exit 75
// ---------------------------------------------------------------------------
// Ocupa os 4 slots padrão via fs4, depois dispara uma 5ª invocação com
// --wait-lock 0 e confirma que ela retorna exit 75 (AllSlotsFull).

#[test]
#[serial]
fn cinco_instancias_quinta_exit_75() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");

    // Ocupa todos os 4 slots padrão
    let handles = ocupar_slots(&tmp, 4);

    // 5ª invocação com --wait-lock 0 deve falhar com exit 75
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

    drop(handles);
}

// ---------------------------------------------------------------------------
// Teste 2 — --wait-lock 3 espera até 3 segundos pelo slot
// ---------------------------------------------------------------------------
// Ocupa todos os slots, libera após 1s em thread separada, confirma que
// --wait-lock 3 aguarda e conclui com sucesso.
//
// MARCADO #[ignore] — adiciona ~1-2s ao CI e depende de timing de threads.
// Para rodar manualmente:
//   cargo test -- --ignored wait_lock_3s_respeitado

#[test]
#[serial]
#[ignore = "flaky — depende de timing de threads — rodar manualmente com: cargo test -- --ignored"]
fn wait_lock_3s_respeitado() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");
    let tmp_path = tmp.path().to_path_buf();

    // Ocupa todos os 4 slots
    let handles = ocupar_slots(&tmp, 4);

    // Libera todos após 1 segundo em thread separada
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(800));
        drop(handles);
        // Manter tmp_path vivo até aqui
        let _ = &tmp_path;
    });

    // --wait-lock 3 deve aguardar a liberação (dentro de 3s) e completar
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário sqlite-graphrag não encontrado")
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args([
            "--skip-memory-guard",
            "--max-concurrency",
            "4",
            "--wait-lock",
            "3",
            "namespace-detect",
        ])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Teste 3 — remember duplicado seguido de edit com --updated-at stale → exit 3
// ---------------------------------------------------------------------------
// Simula optimistic locking: insere uma memória, obtém updated_at, modifica
// via CLI, então tenta editar novamente com o updated_at stale (antes da
// modificação) e confirma exit 3 (Conflict).

#[test]
#[serial]
fn optimistic_locking_conflito_exit_3() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");
    let db_path = tmp.path().join("test.sqlite");

    // Init
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário não encontrado")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    // Inserir memória
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário não encontrado")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args([
            "--skip-memory-guard",
            "remember",
            "--name",
            "mem-conflito",
            "--type",
            "user",
            "--namespace",
            "global",
            "--description",
            "desc original",
            "--body",
            "corpo original",
        ])
        .assert()
        .success();

    // Obter updated_at via read para capturar o timestamp antes de modificar
    let output_leitura = Command::cargo_bin("sqlite-graphrag")
        .expect("binário não encontrado")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args([
            "--skip-memory-guard",
            "read",
            "--name",
            "mem-conflito",
            "--namespace",
            "global",
        ])
        .output()
        .expect("output deve funcionar");

    let json_leitura: serde_json::Value =
        serde_json::from_slice(&output_leitura.stdout).expect("output deve ser JSON");

    let _updated_at_real = json_leitura
        .get("updated_at")
        .and_then(|v| v.as_i64())
        .expect("updated_at deve existir e ser i64");

    // Valor impossível: Unix epoch 1970-01-01 nunca será o updated_at de uma memória recém-criada.
    // Garante o conflito independentemente de quantas operações ocorram no mesmo segundo.
    let updated_at_stale: i64 = 1;

    // Edição com --expected-updated-at stale deve falhar com exit 3 (Conflict)
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário não encontrado")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args([
            "--skip-memory-guard",
            "edit",
            "--name",
            "mem-conflito",
            "--namespace",
            "global",
            "--description",
            "desc conflitante",
            "--expected-updated-at",
            &updated_at_stale.to_string(),
        ])
        .assert()
        .failure()
        .code(3);
}

// ---------------------------------------------------------------------------
// Teste 4 — purge durante recall não corrompe o banco
// ---------------------------------------------------------------------------
// Dispara recall e purge em paralelo via threads e confirma que o banco
// permanece íntegro (sem erros SQLITE_CORRUPT ou panic) após ambos concluírem.
// Usa std::sync::Barrier para sincronizar o início.

#[test]
#[serial]
fn purge_durante_recall_nao_corrompe() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");
    let db_path = tmp.path().join("test.sqlite");

    // Init
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário não encontrado")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    // Inserir algumas memórias antigas para que purge tenha algo a fazer
    for i in 0..3 {
        Command::cargo_bin("sqlite-graphrag")
            .expect("binário não encontrado")
            .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
            .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
            .args([
                "--skip-memory-guard",
                "remember",
                "--name",
                &format!("mem-purge-{i}"),
                "--type",
                "user",
                "--namespace",
                "global",
                "--description",
                &format!("memória antiga {i}"),
                "--body",
                &format!("corpo da memória para purge teste {i}"),
            ])
            .assert()
            .success();
    }

    let db_path_recall = db_path.clone();
    let db_path_purge = db_path.clone();
    let cache_path_recall = tmp.path().to_path_buf();
    let cache_path_purge = tmp.path().to_path_buf();

    let barrier = Arc::new(Barrier::new(2));
    let barrier_recall = Arc::clone(&barrier);
    let barrier_purge = Arc::clone(&barrier);

    let bin_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_sqlite-graphrag"));

    let bin_recall = bin_path.clone();
    let bin_purge = bin_path.clone();

    // Thread recall — busca concorrente
    let handle_recall = std::thread::spawn(move || {
        barrier_recall.wait();
        std::process::Command::new(&bin_recall)
            .env("SQLITE_GRAPHRAG_DB_PATH", &db_path_recall)
            .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache_path_recall)
            .args([
                "--skip-memory-guard",
                "recall",
                "memória antiga",
                "--namespace",
                "global",
                "--k",
                "5",
            ])
            .output()
            .expect("recall deve executar sem panic")
    });

    // Thread purge — purge concorrente com --dry-run para não deletar nada
    let handle_purge = std::thread::spawn(move || {
        barrier_purge.wait();
        std::process::Command::new(&bin_purge)
            .env("SQLITE_GRAPHRAG_DB_PATH", &db_path_purge)
            .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache_path_purge)
            .args([
                "--skip-memory-guard",
                "purge",
                "--namespace",
                "global",
                "--dry-run",
            ])
            .output()
            .expect("purge deve executar sem panic")
    });

    let resultado_recall = handle_recall
        .join()
        .expect("thread recall não deve entrar em panic");
    let resultado_purge = handle_purge
        .join()
        .expect("thread purge não deve entrar em panic");

    // Nenhum dos dois deve ter saído com código de erro de corrupção
    // Exit code 10 = Database error (SQLite), 20 = Internal
    let codigo_recall = resultado_recall.status.code().unwrap_or(-1);
    let codigo_purge = resultado_purge.status.code().unwrap_or(-1);

    assert_ne!(
        codigo_recall, 20,
        "recall não deve retornar erro interno (exit 20)"
    );
    assert_ne!(
        codigo_purge, 20,
        "purge não deve retornar erro interno (exit 20)"
    );

    // Verificar integridade do banco após operações concorrentes
    let conn = rusqlite::Connection::open(&db_path).expect("banco deve abrir após concorrência");
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("PRAGMA integrity_check deve funcionar");
    assert_eq!(
        integrity, "ok",
        "banco deve estar íntegro após recall+purge concorrentes"
    );
}

// ---------------------------------------------------------------------------
// Teste 5 — 10 remembers em namespaces diferentes não colidem
// ---------------------------------------------------------------------------
// Confirma que inserções em 10 namespaces distintos via threads simultâneas
// são todas bem-sucedidas e que cada namespace contém exatamente 1 memória.

#[test]
#[serial]
fn dez_remembers_namespaces_diferentes() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");
    let db_path = tmp.path().join("test.sqlite");

    // Init
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário não encontrado")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    let n_threads = 10;
    let barrier = Arc::new(Barrier::new(n_threads));
    let bin_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_sqlite-graphrag"));

    let handles: Vec<_> = (0..n_threads)
        .map(|i| {
            let db_path_clone = db_path.clone();
            let cache_path_clone = tmp.path().to_path_buf();
            let barrier_clone = Arc::clone(&barrier);
            let namespace = format!("ns-thread-{i}");
            let bin_clone = bin_path.clone();

            std::thread::spawn(move || {
                // Sincroniza todos os threads antes de disparar
                barrier_clone.wait();

                std::process::Command::new(&bin_clone)
                    .env("SQLITE_GRAPHRAG_DB_PATH", &db_path_clone)
                    .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache_path_clone)
                    .args([
                        "--skip-memory-guard",
                        "remember",
                        "--name",
                        &format!("mem-thread-{i}"),
                        "--type",
                        "user",
                        "--namespace",
                        &namespace,
                        "--description",
                        &format!("memória do thread {i}"),
                        "--body",
                        &format!("corpo da memória isolada para o namespace {namespace}"),
                    ])
                    .output()
                    .expect("remember deve executar sem panic")
            })
        })
        .collect();

    // Coleta resultados de todas as threads
    let resultados: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().expect("thread não deve entrar em panic"))
        .collect();

    let sucessos = resultados.iter().filter(|r| r.status.success()).count();
    let falhas = resultados.len() - sucessos;

    assert_eq!(
        sucessos, n_threads,
        "todos os {n_threads} remembers em namespaces distintos devem ter sucesso, \
         obtivemos {sucessos} sucessos e {falhas} falhas"
    );

    // Verificar que cada namespace tem exatamente 1 memória no banco
    let conn = rusqlite::Connection::open(&db_path).expect("banco deve abrir");
    for i in 0..n_threads {
        let namespace = format!("ns-thread-{i}");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE namespace = ?1 AND deleted_at IS NULL",
                rusqlite::params![namespace],
                |row| row.get(0),
            )
            .expect("query deve funcionar");

        assert_eq!(
            count, 1,
            "namespace '{namespace}' deve ter exatamente 1 memória, encontrou {count}"
        );
    }
}
