use assert_cmd::cargo::cargo_bin;
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn run_with_env(cache_dir: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(cargo_bin("sqlite-graphrag"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", cache_dir)
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .args(args)
        .output()
        .expect("subprocesso sqlite-graphrag falhou")
}

fn ping_until_ready(cache_dir: &PathBuf) -> Value {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let out = run_with_env(cache_dir, &["daemon", "--ping"]);
        if out.status.success() {
            return serde_json::from_slice(&out.stdout).expect("ping json invalido");
        }
        assert!(Instant::now() < deadline, "daemon nao ficou pronto a tempo");
        thread::sleep(Duration::from_millis(200));
    }
}

fn wait_child_exit(child: &mut std::process::Child) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait().expect("try_wait falhou") {
            Some(status) => {
                assert!(status.success(), "daemon terminou com erro: {status}");
                return;
            }
            None => {
                assert!(Instant::now() < deadline, "daemon nao encerrou a tempo");
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn start_daemon(cache_dir: &PathBuf) -> std::process::Child {
    Command::new(cargo_bin("sqlite-graphrag"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", cache_dir)
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .arg("daemon")
        .arg("--idle-shutdown-secs")
        .arg("300")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn do daemon falhou")
}

fn run_heavy_command(
    cache_dir: &PathBuf,
    db_path: &PathBuf,
    args: &[&str],
) -> std::process::Output {
    Command::new(cargo_bin("sqlite-graphrag"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", cache_dir)
        .env("SQLITE_GRAPHRAG_DB_PATH", db_path)
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .arg("--skip-memory-guard")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn daemon_ping_and_stop_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let cache_dir = tmp.path().join("cache");
    let mut child = start_daemon(&cache_dir);

    let ping = ping_until_ready(&cache_dir);
    assert_eq!(ping["status"], "ok");
    assert_eq!(ping["handled_embed_requests"], 0);

    let stop = run_with_env(&cache_dir, &["daemon", "--stop"]);
    assert!(stop.status.success(), "stop falhou: {stop:?}");
    let stop_json: Value = serde_json::from_slice(&stop.stdout).unwrap();
    assert_eq!(stop_json["status"], "shutting_down");

    wait_child_exit(&mut child);
}

#[test]
fn init_remember_recall_and_hybrid_increment_daemon_counter() {
    let tmp = TempDir::new().unwrap();
    let cache_dir = tmp.path().join("cache");
    let db_path = tmp.path().join("graphrag.sqlite");
    let mut child = start_daemon(&cache_dir);

    let initial = ping_until_ready(&cache_dir);
    assert_eq!(initial["handled_embed_requests"], 0);

    let init = Command::new(cargo_bin("sqlite-graphrag"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache_dir)
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .arg("--skip-memory-guard")
        .arg("init")
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "init falhou: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let after_init = ping_until_ready(&cache_dir);
    let count_after_init = after_init["handled_embed_requests"].as_u64().unwrap();
    assert!(count_after_init >= 1);

    let remember = Command::new(cargo_bin("sqlite-graphrag"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache_dir)
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .arg("--skip-memory-guard")
        .args([
            "remember",
            "--name",
            "daemon-note",
            "--type",
            "reference",
            "--description",
            "daemon integration",
            "--body",
            "persistent daemon should reuse the embedding model",
        ])
        .output()
        .unwrap();
    assert!(
        remember.status.success(),
        "remember falhou: {}",
        String::from_utf8_lossy(&remember.stderr)
    );

    let after_remember = ping_until_ready(&cache_dir);
    let count_after_remember = after_remember["handled_embed_requests"].as_u64().unwrap();
    assert!(count_after_remember > count_after_init);

    let recall = Command::new(cargo_bin("sqlite-graphrag"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache_dir)
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .env("SQLITE_GRAPHRAG_LANG", "en")
        .arg("--skip-memory-guard")
        .args(["recall", "embedding model", "--json", "--k", "3"])
        .output()
        .unwrap();
    assert!(
        recall.status.success(),
        "recall falhou: {}",
        String::from_utf8_lossy(&recall.stderr)
    );

    let after_recall = ping_until_ready(&cache_dir);
    let count_after_recall = after_recall["handled_embed_requests"].as_u64().unwrap();
    assert!(count_after_recall > count_after_remember);

    let hybrid = Command::new(cargo_bin("sqlite-graphrag"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache_dir)
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .arg("--skip-memory-guard")
        .args(["hybrid-search", "embedding model", "--json", "--k", "3"])
        .output()
        .unwrap();
    assert!(
        hybrid.status.success(),
        "hybrid-search falhou: {}",
        String::from_utf8_lossy(&hybrid.stderr)
    );

    let after_hybrid = ping_until_ready(&cache_dir);
    let count_after_hybrid = after_hybrid["handled_embed_requests"].as_u64().unwrap();
    assert!(count_after_hybrid > count_after_recall);

    let stop = run_with_env(&cache_dir, &["daemon", "--stop"]);
    assert!(stop.status.success());
    wait_child_exit(&mut child);
}

#[test]
fn init_autospawns_daemon_when_missing() {
    let tmp = TempDir::new().unwrap();
    let cache_dir = tmp.path().join("cache");
    let db_path = tmp.path().join("graphrag.sqlite");

    let init = run_heavy_command(&cache_dir, &db_path, &["init"]);
    assert!(
        init.status.success(),
        "init falhou: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let ping = ping_until_ready(&cache_dir);
    assert_eq!(ping["status"], "ok");
    assert!(ping["handled_embed_requests"].as_u64().unwrap() >= 1);

    let stop = run_with_env(&cache_dir, &["daemon", "--stop"]);
    assert!(stop.status.success(), "stop falhou: {stop:?}");
}

#[test]
fn daemon_respawns_automatically_after_stop() {
    let tmp = TempDir::new().unwrap();
    let cache_dir = tmp.path().join("cache");
    let db_path = tmp.path().join("graphrag.sqlite");

    let init = run_heavy_command(&cache_dir, &db_path, &["init"]);
    assert!(
        init.status.success(),
        "init falhou: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let first_ping = ping_until_ready(&cache_dir);
    let first_pid = first_ping["pid"].as_u64().unwrap();

    let stop = run_with_env(&cache_dir, &["daemon", "--stop"]);
    assert!(stop.status.success(), "stop falhou: {stop:?}");

    let stopped_ping = run_with_env(&cache_dir, &["daemon", "--ping"]);
    assert!(
        !stopped_ping.status.success(),
        "daemon ainda respondeu a ping apos stop"
    );

    let recall = run_heavy_command(
        &cache_dir,
        &db_path,
        &["recall", "autospawn", "--json", "--k", "3"],
    );
    assert!(
        recall.status.success(),
        "recall falhou: {}",
        String::from_utf8_lossy(&recall.stderr)
    );

    let second_ping = ping_until_ready(&cache_dir);
    let second_pid = second_ping["pid"].as_u64().unwrap();
    assert_ne!(
        first_pid, second_pid,
        "daemon nao reiniciou com novo processo apos stop"
    );

    let stop_again = run_with_env(&cache_dir, &["daemon", "--stop"]);
    assert!(
        stop_again.status.success(),
        "stop final falhou: {stop_again:?}"
    );
}
