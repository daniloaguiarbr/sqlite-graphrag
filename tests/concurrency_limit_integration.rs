// E2E concurrency limit tests for the sqlite-graphrag slot semaphore.
//
// ISOLATION: `SQLITE_GRAPHRAG_CACHE_DIR` points to a `TempDir` unique per test.
// `#[serial]` is required in all tests to avoid filesystem races between tests
// that share the same compiled binary.
//
// `--skip-memory-guard` is used in all tests so that the available RAM check
// does not abort before the semaphore is exercised.

use assert_cmd::Command;
use serial_test::serial;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Test 1 — concurrency limit is respected under 10-process load
// ---------------------------------------------------------------------------
// Spawns 10 parallel invocations with --max-concurrency 4 and --wait-lock 30.
// Verifies that ALL complete successfully (the 6 that cannot acquire a slot
// keep polling until one of the initial 4 finishes and releases its slot).

#[test]
#[serial]
fn limite_respeitado_sob_carga() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");
    let bin = assert_cmd::cargo::cargo_bin("sqlite-graphrag");

    // Spawn 10 invocations in parallel using std::process::Command for
    // direct PID control (assert_cmd does not expose spawn).
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
                .expect("failure ao spawnar invocação paralela")
        })
        .collect();

    // Aguarda todas e coleta exit codes.
    let resultados: Vec<_> = handles
        .into_iter()
        .map(|h| h.wait_with_output().expect("wait failed"))
        .collect();

    let successos = resultados.iter().filter(|r| r.status.success()).count();
    let falhas = resultados.iter().filter(|r| !r.status.success()).count();

    // All 10 invocations must complete successfully when --wait-lock=30.
    assert_eq!(
        successos, 10,
        "todas as 10 invocações devem completar com sucesso (--wait-lock 30), \
         obtivemos {successos} sucessos e {falhas} falhas"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — --max-concurrency 0 is rejected with exit 2
// ---------------------------------------------------------------------------
// Validates that the validation guard in `Cli::validate_flags` rejects N=0
// antes de tentar adquirir qualquer slot.

#[test]
#[serial]
fn max_concurrency_zero_rejected_with_exit_2() {
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
// Occupy N slots directly via fs4 and verify that invocation with --wait-lock 0
// retorna exit 75 (AllSlotsFull) imediatamente sem timeout.

#[test]
#[serial]
fn all_slots_busy_return_75() {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    let tmp = TempDir::new().expect("TempDir deve ser criado");
    let max: usize = 4;

    // Lock all slots directly to simulate 4 active instances.
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

    // Invocation with --wait-lock 0 must fail immediately with exit 75.
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
// Test 4 — --skip-memory-guard bypasses the available-memory check
// ---------------------------------------------------------------------------
// Verifies that `--skip-memory-guard` allows the command to run without going
// through the RAM check. Without the flag, the command could fail with
// exit 77 in CI environments with limited available memory. With the flag, it
// should complete normally regardless of available RAM.

#[test]
#[serial]
fn skip_memory_guard_bypasses_ram_check() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");

    // With --skip-memory-guard, the command must complete successfully even in
    // environments where available RAM could cause exit 77.
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário sqlite-graphrag não encontrado")
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "namespace-detect"])
        .assert()
        .success();
}
