// E2E integration tests for the sqlite-graphrag slot semaphore.
//
// ISOLATION: every test sets `SQLITE_GRAPHRAG_CACHE_DIR` pointing
// to an exclusive `TempDir`, ensuring lock files do not pollute
// `~/.cache/sqlite-graphrag` nor collide between tests.
//
// `#[serial]` is mandatory in all tests: although each test uses
// its own directory, the compiled binary is shared and `TempDir` is only
// released after the test ends; serializing eliminates filesystem races
// and makes timings predictable.
//
// Scenarios 4 and 5 depend on an external process that holds a slot for
// a deterministic duration. Marked `#[ignore]` because subprocess timing is
// flaky in environments with variable load.

use assert_cmd::Command;
use serial_test::serial;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Retorna o caminho do arquivo de lock para o slot indicado (1-based)
/// within the provided `TempDir`, mirroring the logic of `lock.rs`.
fn slot_path(tmp: &TempDir, slot: usize) -> std::path::PathBuf {
    tmp.path().join(format!("cli-slot-{slot}.lock"))
}

// ---------------------------------------------------------------------------
// Scenario 1 — slot is released after process exits
// ---------------------------------------------------------------------------
// Ensures that two sequential invocations without --max-concurrency do not conflict,
// since the first process releases the slot on exit.

#[test]
#[serial]
fn slot_released_after_process_exits() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");

    // First invocation — must acquire and release slot 1.
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário sqlite-graphrag não encontrado")
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "namespace-detect"])
        .assert()
        .success();

    // Second invocation — must acquire the slot again without error.
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário sqlite-graphrag não encontrado")
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "namespace-detect"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Scenario 2 — slot file is created in the configured cache dir
// ---------------------------------------------------------------------------
// Confirms that the binary creates `cli-slot-1.lock` in the directory overridden via
// `SQLITE_GRAPHRAG_CACHE_DIR`.

#[test]
#[serial]
fn slot_file_created_in_cache_dir() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");

    Command::cargo_bin("sqlite-graphrag")
        .expect("binário sqlite-graphrag não encontrado")
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
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
// Scenario 3 — --wait-lock 0 fails immediately when all slots are busy
// ---------------------------------------------------------------------------
// Simulates N busy slots by creating and locking the lock files directly,
// then confirms that a new invocation returns exit 75 without waiting.

#[test]
#[serial]
fn wait_lock_zero_returns_75_when_slots_busy() {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    let tmp = TempDir::new().expect("TempDir deve ser criado");
    let max = 4;

    // Lock all N slots directly to simulate N running instances.
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

    // Invocation with all slots busy and --wait-lock 0 → exit 75.
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

    // Release the locks before drop(tmp).
    drop(handles);
}

// ---------------------------------------------------------------------------
// Scenario 4 — second instance receives exit 75 while slot is busy
// ---------------------------------------------------------------------------
// MARKED #[ignore] — subprocess timing is flaky in environments with variable load.
// Para rodar manualmente:
//   cargo test -- --ignored slot_bloqueia_segunda_instancia_com_exit_75

#[test]
#[serial]
#[ignore = "flaky — depende de timing de subprocessos — rodar manualmente com: cargo test -- --ignored"]
fn slot_bloqueia_segunda_instancia_com_exit_75() {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    let tmp = TempDir::new().expect("TempDir deve ser criado");

    // Lock all slots (default 4) to simulate maximum saturation.
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

    // Second instance must fail immediately with exit 75.
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
// Scenario 5 — --wait-lock waits and acquires the slot after release
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

    // Release all after 1 second in a separate thread.
    let tmp_path = tmp.path().to_path_buf();
    let _ = tmp_path; // silence unused warning
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        drop(handles);
    });

    // --wait-lock 10 must wait for release and complete successfully.
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário sqlite-graphrag não encontrado")
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
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
