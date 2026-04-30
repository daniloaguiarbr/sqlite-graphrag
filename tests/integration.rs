#![cfg(feature = "slow-tests")]

use assert_cmd::Command;
use tempfile::TempDir;

/// Cria um Command isolado com db em TempDir dedicado e cache de modelos compartilhado.
fn cmd(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c
}

fn init_db(tmp: &TempDir) {
    cmd(tmp).arg("init").assert().success();
}

fn isolated_cmd_in(dir: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.current_dir(dir);
    c.env_remove("SQLITE_GRAPHRAG_NAMESPACE");
    c.env_remove("SQLITE_GRAPHRAG_DB_PATH");
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", dir.join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c
}

// ---------------------------------------------------------------------------
// Database path resolution via SQLITE_GRAPHRAG_HOME
// ---------------------------------------------------------------------------

/// Isolated helper that does NOT inject `SQLITE_GRAPHRAG_DB_PATH`, letting
/// resolution fall back to `SQLITE_GRAPHRAG_HOME` or `current_dir`. Uses
/// `env_clear` to ensure CI environment vars do not leak.
fn home_isolated_cmd(cwd: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env_clear();
    // PATH is required in some environments for binary libs; preserve it minimally.
    if let Ok(path_var) = std::env::var("PATH") {
        c.env("PATH", path_var);
    }
    if let Ok(home_var) = std::env::var("HOME") {
        c.env("HOME", home_var);
    }
    c.current_dir(cwd);
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", cwd.join("cache"));
    c
}

#[test]
fn cli_home_env_creates_db_in_target_dir() {
    let home_dir = TempDir::new().expect("home tempdir");
    let cwd_dir = TempDir::new().expect("cwd tempdir");
    let banco_no_home = home_dir.path().join("graphrag.sqlite");
    let banco_no_cwd = cwd_dir.path().join("graphrag.sqlite");

    home_isolated_cmd(cwd_dir.path())
        .env("SQLITE_GRAPHRAG_HOME", home_dir.path())
        .arg("init")
        .assert()
        .success();

    assert!(
        banco_no_home.exists(),
        "init com SQLITE_GRAPHRAG_HOME deve criar o banco no diretório indicado"
    );
    assert!(
        !banco_no_cwd.exists(),
        "init com SQLITE_GRAPHRAG_HOME NÃO deve criar banco no current_dir"
    );
}

#[test]
fn cli_home_traversal_rejected() {
    let cwd_dir = TempDir::new().expect("cwd tempdir");

    home_isolated_cmd(cwd_dir.path())
        .env("SQLITE_GRAPHRAG_HOME", "/tmp/../etc")
        .arg("init")
        .assert()
        .failure();
}

#[test]
fn cli_db_path_overrides_home_env() {
    let home_dir = TempDir::new().expect("home tempdir");
    let db_dir = TempDir::new().expect("db tempdir");
    let cwd_dir = TempDir::new().expect("cwd tempdir");
    let db_explicito = db_dir.path().join("explicito.sqlite");
    let banco_no_home = home_dir.path().join("graphrag.sqlite");

    home_isolated_cmd(cwd_dir.path())
        .env("SQLITE_GRAPHRAG_HOME", home_dir.path())
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_explicito)
        .arg("init")
        .assert()
        .success();

    assert!(
        db_explicito.exists(),
        "SQLITE_GRAPHRAG_DB_PATH deve vencer SQLITE_GRAPHRAG_HOME"
    );
    assert!(
        !banco_no_home.exists(),
        "HOME não deve ser usado quando DB_PATH está presente"
    );
}

#[test]
fn cli_flag_db_overrides_home_env() {
    let home_dir = TempDir::new().expect("home tempdir");
    let flag_dir = TempDir::new().expect("flag tempdir");
    let cwd_dir = TempDir::new().expect("cwd tempdir");
    let db_flag = flag_dir.path().join("via-flag.sqlite");
    let banco_no_home = home_dir.path().join("graphrag.sqlite");

    home_isolated_cmd(cwd_dir.path())
        .env("SQLITE_GRAPHRAG_HOME", home_dir.path())
        .args(["init", "--db", db_flag.to_str().unwrap()])
        .assert()
        .success();

    assert!(
        db_flag.exists(),
        "flag --db deve vencer SQLITE_GRAPHRAG_HOME"
    );
    assert!(
        !banco_no_home.exists(),
        "HOME não deve ser usado quando --db está presente"
    );
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

#[test]
fn test_init_creates_sqlite_file() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    assert!(!db_path.exists(), "banco nao deve existir antes do init");

    cmd(&tmp).arg("init").assert().success();

    assert!(db_path.exists(), "banco deve existir apos o init");
}

#[test]
fn test_init_creates_local_db_in_invocation_directory() {
    let pasta_a = TempDir::new().unwrap();
    let pasta_b = TempDir::new().unwrap();
    let banco_a = pasta_a.path().join("graphrag.sqlite");
    let banco_b = pasta_b.path().join("graphrag.sqlite");

    assert!(
        !banco_a.exists(),
        "banco local nao deve existir antes do init em a"
    );
    assert!(
        !banco_b.exists(),
        "banco local nao deve existir antes do init em b"
    );

    isolated_cmd_in(pasta_a.path())
        .arg("init")
        .assert()
        .success();
    isolated_cmd_in(pasta_b.path())
        .arg("init")
        .assert()
        .success();

    assert!(banco_a.exists(), "init deve criar graphrag.sqlite em a");
    assert!(banco_b.exists(), "init deve criar graphrag.sqlite em b");
}

#[test]
fn test_crud_uses_graphrag_sqlite_in_invocation_directory() {
    let pasta = TempDir::new().unwrap();
    let banco = pasta.path().join("graphrag.sqlite");

    assert!(
        !banco.exists(),
        "banco local nao deve existir antes do init no diretorio da invocacao"
    );

    isolated_cmd_in(pasta.path()).arg("init").assert().success();

    assert!(
        banco.exists(),
        "init deve criar graphrag.sqlite no diretorio da invocacao"
    );

    isolated_cmd_in(pasta.path())
        .args([
            "remember",
            "--name",
            "memoria-cwd",
            "--type",
            "user",
            "--description",
            "crud cwd",
            "--body",
            "conteudo salvo no banco local da pasta atual",
        ])
        .assert()
        .success();

    let read_output = isolated_cmd_in(pasta.path())
        .args(["read", "--name", "memoria-cwd"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let read_json: serde_json::Value = serde_json::from_slice(&read_output).unwrap();
    assert_eq!(read_json["name"], "memoria-cwd");
    assert_eq!(read_json["description"], "crud cwd");

    let list_output = isolated_cmd_in(pasta.path())
        .arg("list")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list_json: serde_json::Value = serde_json::from_slice(&list_output).unwrap();
    let itens = list_json["items"].as_array().unwrap();
    assert!(
        itens.iter().any(|item| item["name"] == "memoria-cwd"),
        "list deve ler a memoria persistida em ./graphrag.sqlite"
    );

    isolated_cmd_in(pasta.path())
        .args(["forget", "--name", "memoria-cwd"])
        .assert()
        .success();

    let purge_output = isolated_cmd_in(pasta.path())
        .args(["purge", "--retention-days", "0", "--yes"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let purge_json: serde_json::Value = serde_json::from_slice(&purge_output).unwrap();
    assert_eq!(purge_json["purged_count"], 1);
}

#[test]
fn test_remember_without_init_creates_migrated_local_db() {
    let pasta = TempDir::new().unwrap();
    let banco = pasta.path().join("graphrag.sqlite");

    assert!(
        !banco.exists(),
        "banco local nao deve existir antes do remember"
    );

    isolated_cmd_in(pasta.path())
        .args([
            "remember",
            "--name",
            "memoria-sem-init",
            "--type",
            "user",
            "--description",
            "create sem init",
            "--body",
            "conteudo salvo sem init explicito",
            "--skip-extraction",
            "--json",
        ])
        .assert()
        .success();

    assert!(
        banco.exists(),
        "remember deve criar graphrag.sqlite migrado no cwd"
    );

    let read_output = isolated_cmd_in(pasta.path())
        .args(["read", "--name", "memoria-sem-init", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let read_json: serde_json::Value = serde_json::from_slice(&read_output).unwrap();
    assert_eq!(read_json["name"], "memoria-sem-init");
    assert_eq!(read_json["body"], "conteudo salvo sem init explicito");
}

#[test]
fn test_init_returns_json_with_status_ok() {
    let tmp = TempDir::new().unwrap();
    let output = cmd(&tmp)
        .arg("init")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["model"], "multilingual-e5-small");
    assert!(json["dim"].as_u64().unwrap() > 0);
}

// ---------------------------------------------------------------------------
// health
// ---------------------------------------------------------------------------

#[test]
fn test_health_fails_without_init() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp).arg("health").assert().failure();
}

#[test]
fn test_health_ok_after_init() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .arg("health")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["integrity"], "ok");
}

// ---------------------------------------------------------------------------
// daemon
// ---------------------------------------------------------------------------

#[test]
fn test_daemon_help_lists_db_and_json() {
    let tmp = TempDir::new().unwrap();

    let output = Command::cargo_bin("sqlite-graphrag")
        .unwrap()
        .current_dir(tmp.path())
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"))
        .args(["daemon", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help = String::from_utf8(output).unwrap();
    assert!(help.contains("--db"));
    assert!(help.contains("--json"));
}

#[test]
fn test_daemon_accepts_db_ping_json_without_parse_error() {
    let tmp = TempDir::new().unwrap();

    Command::cargo_bin("sqlite-graphrag")
        .unwrap()
        .current_dir(tmp.path())
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"))
        .args(["daemon", "--db", "foo.sqlite", "--ping", "--json"])
        .assert()
        .failure()
        .code(4);
}

// ---------------------------------------------------------------------------
// remember
// ---------------------------------------------------------------------------

#[test]
fn test_remember_creates_memory() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-teste",
            "--type",
            "user",
            "--description",
            "Descricao de teste",
            "--body",
            "Conteudo do corpo da memoria de teste",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["action"], "created");
    assert_eq!(json["name"], "memoria-teste");
    assert!(json["memory_id"].as_i64().unwrap() > 0);
}

#[test]
fn test_remember_duplicate_returns_exit_2() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "dup-memoria",
            "--type",
            "user",
            "--description",
            "Primeira versao",
            "--body",
            "Corpo da primeira versao",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "dup-memoria",
            "--type",
            "user",
            "--description",
            "Segunda versao",
            "--body",
            "Corpo da segunda versao",
        ])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn test_remember_force_merge_updates() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-merge",
            "--type",
            "feedback",
            "--description",
            "Descricao original",
            "--body",
            "Corpo original da memoria",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-merge",
            "--type",
            "feedback",
            "--description",
            "Descricao atualizada",
            "--body",
            "Corpo atualizado da memoria",
            "--force-merge",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["action"], "updated");
    assert_eq!(json["name"], "memoria-merge");
}

#[test]
fn test_remember_rejects_body_and_body_stdin_together() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "entrada-ambigua",
            "--type",
            "project",
            "--description",
            "fontes ambiguas",
            "--body",
            "corpo explicito",
            "--body-stdin",
        ])
        .write_stdin("corpo stdin")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn test_remember_graph_stdin_invalid_fails_without_saving_memory() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "grafo-invalido",
            "--type",
            "project",
            "--description",
            "json invalido",
            "--graph-stdin",
        ])
        .write_stdin("{not-json")
        .assert()
        .failure()
        .code(1);

    cmd(&tmp)
        .args(["read", "--name", "grafo-invalido"])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn test_remember_graph_stdin_semantic_invalid_fails_without_saving_memory() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let casos = [
        (
            "tipo-invalido",
            r#"{"entities":[{"name":"bad-agent","entity_type":"agent"}],"relationships":[]}"#,
        ),
        (
            "relacao-invalida",
            r#"{"entities":[{"name":"a","entity_type":"tool"},{"name":"b","entity_type":"file"}],"relationships":[{"source":"a","target":"b","relation":"writes","strength":0.5}]}"#,
        ),
        (
            "peso-invalido",
            r#"{"entities":[{"name":"c","entity_type":"tool"},{"name":"d","entity_type":"file"}],"relationships":[{"source":"c","target":"d","relation":"uses","strength":2.0}]}"#,
        ),
        (
            "campo-desconhecido",
            r#"{"entities":[{"name":"e","entity_type":"tool","extra":"nao"}],"relationships":[]}"#,
        ),
    ];

    for (name, payload) in casos {
        cmd(&tmp)
            .args([
                "remember",
                "--name",
                name,
                "--type",
                "project",
                "--description",
                "grafo invalido",
                "--graph-stdin",
                "--json",
            ])
            .write_stdin(payload)
            .assert()
            .failure()
            .code(1);

        cmd(&tmp)
            .args(["read", "--name", name, "--json"])
            .assert()
            .failure()
            .code(4);
    }
}

// ---------------------------------------------------------------------------
// read
// ---------------------------------------------------------------------------

#[test]
fn test_read_existing_memory() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-legivel",
            "--type",
            "project",
            "--description",
            "Uma memoria legivel",
            "--body",
            "O conteudo do corpo da memoria",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["read", "--name", "memoria-legivel"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["name"], "memoria-legivel");
    assert_eq!(json["memory_type"], "project");
    assert_eq!(json["description"], "Uma memoria legivel");
}

#[test]
fn test_read_nonexistent_returns_exit_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args(["read", "--name", "nao-existe"])
        .assert()
        .failure()
        .code(4);
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

#[test]
fn test_list_memories() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "lista-mem-1",
            "--type",
            "user",
            "--description",
            "desc1",
            "--body",
            "corpo1",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "lista-mem-2",
            "--type",
            "feedback",
            "--description",
            "desc2",
            "--body",
            "corpo2",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .arg("list")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json["items"].as_array().unwrap().len() >= 2);
}

// ---------------------------------------------------------------------------
// forget
// ---------------------------------------------------------------------------

#[test]
fn test_forget_soft_delete() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "esquecivel",
            "--type",
            "user",
            "--description",
            "sera deletada",
            "--body",
            "corpo",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["forget", "--name", "esquecivel"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["forgotten"], true);

    cmd(&tmp)
        .args(["read", "--name", "esquecivel"])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn test_forget_nonexistent_returns_exit_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args(["forget", "--name", "nao-existe"])
        .assert()
        .failure()
        .code(4);
}

// ---------------------------------------------------------------------------
// purge
// ---------------------------------------------------------------------------

#[test]
fn test_purge_removes_soft_deleted_memory() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "purge-target",
            "--type",
            "user",
            "--description",
            "soft delete target",
            "--body",
            "body to purge later",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args(["forget", "--name", "purge-target"])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["purge", "--name", "purge-target", "--retention-days", "0"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["purged_count"], 1);
}

#[test]
fn test_purge_yes_flag_is_noop() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "purge-yes-target",
            "--type",
            "user",
            "--description",
            "alvo para teste --yes",
            "--body",
            "corpo yes noop",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args(["forget", "--name", "purge-yes-target"])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args([
            "purge",
            "--name",
            "purge-yes-target",
            "--retention-days",
            "0",
            "--yes",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["purged_count"], 1);
}

// ---------------------------------------------------------------------------
// namespace-detect
// ---------------------------------------------------------------------------

#[test]
fn test_namespace_detect_returns_global_without_local_config() {
    let tmp = TempDir::new().unwrap();

    let output = isolated_cmd_in(tmp.path())
        .arg("namespace-detect")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["namespace"], "global");
    assert_eq!(json["source"], "default");
}

// ---------------------------------------------------------------------------
// sync-safe-copy
// ---------------------------------------------------------------------------

#[test]
fn test_sync_safe_copy_creates_consistent_snapshot() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);
    let dest = tmp.path().join("snapshot.sqlite");

    let output = cmd(&tmp)
        .args(["sync-safe-copy", "--dest", dest.to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "ok");
    assert!(dest.exists());
    assert!(std::fs::metadata(dest).unwrap().len() > 0);
}

// ---------------------------------------------------------------------------
// stats
// ---------------------------------------------------------------------------

#[test]
fn test_stats_returns_counts() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "stat-mem",
            "--type",
            "user",
            "--description",
            "desc",
            "--body",
            "corpo da stat",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .arg("stats")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json["memories"].as_i64().unwrap() >= 1);
    assert!(json["db_size_bytes"].as_u64().unwrap() > 0);
    assert_eq!(json["schema_version"], 9);
}

#[test]
fn test_stats_fails_without_init() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp).arg("stats").assert().failure();
}

// ---------------------------------------------------------------------------
// rename
// ---------------------------------------------------------------------------

#[test]
fn test_rename_memory_works() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-antiga",
            "--type",
            "user",
            "--description",
            "desc original",
            "--body",
            "corpo original",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args([
            "rename",
            "--name",
            "memoria-antiga",
            "--new-name",
            "memoria-renomeada",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["name"], "memoria-renomeada");
    assert!(json["memory_id"].as_i64().unwrap() > 0);
}

#[test]
fn test_rename_nonexistent_returns_exit_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args(["rename", "--name", "nao-existe", "--new-name", "novo-nome"])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn test_rename_new_name_invalid_returns_exit_1() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-valida",
            "--type",
            "user",
            "--description",
            "desc",
            "--body",
            "corpo",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args([
            "rename",
            "--name",
            "memoria-valida",
            "--new-name",
            "Nome Com Espaco",
        ])
        .assert()
        .failure()
        .code(1);
}

// ---------------------------------------------------------------------------
// edit
// ---------------------------------------------------------------------------

#[test]
fn test_edit_memory_works() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-editavel",
            "--type",
            "user",
            "--description",
            "desc original",
            "--body",
            "corpo original",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args([
            "edit",
            "--name",
            "memoria-editavel",
            "--body",
            "corpo atualizado",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["action"], "updated");
}

#[test]
fn test_edit_rejects_body_and_body_stdin_together() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-edit-ambigua",
            "--type",
            "user",
            "--description",
            "desc",
            "--body",
            "corpo original",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args([
            "edit",
            "--name",
            "memoria-edit-ambigua",
            "--body",
            "corpo explicito",
            "--body-stdin",
        ])
        .write_stdin("corpo stdin")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn test_edit_inexistente_retorna_exit_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args(["edit", "--name", "nao-existe", "--body", "novo corpo"])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn test_edit_com_conflict_retorna_exit_3() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-conflito",
            "--type",
            "user",
            "--description",
            "desc original",
            "--body",
            "corpo original",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let wrong_updated_at = json["version"].as_i64().unwrap() + 999;

    cmd(&tmp)
        .args([
            "edit",
            "--name",
            "memoria-conflito",
            "--body",
            "novo corpo",
            "--expected-updated-at",
            &wrong_updated_at.to_string(),
        ])
        .assert()
        .failure()
        .code(3);
}

// ---------------------------------------------------------------------------
// history
// ---------------------------------------------------------------------------

#[test]
fn test_history_retorna_versoes() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-historico",
            "--type",
            "user",
            "--description",
            "v1",
            "--body",
            "corpo v1",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-historico",
            "--type",
            "user",
            "--description",
            "v2",
            "--body",
            "corpo v2",
            "--force-merge",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["history", "--name", "memoria-historico"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let versions = json["versions"].as_array().unwrap();
    assert!(versions.len() >= 2);
}

#[test]
fn test_history_inexistente_retorna_exit_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args(["history", "--name", "nao-existe"])
        .assert()
        .failure()
        .code(4);
}

// ---------------------------------------------------------------------------
// restore
// ---------------------------------------------------------------------------

#[test]
fn test_restore_memory_works() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-restore",
            "--type",
            "user",
            "--description",
            "v1",
            "--body",
            "corpo v1",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-restore",
            "--type",
            "user",
            "--description",
            "v2",
            "--body",
            "corpo v2",
            "--force-merge",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["restore", "--name", "memoria-restore", "--version", "1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["restored_from"], 1);
    assert!(json["version"].as_i64().unwrap() >= 3);
}

#[test]
fn test_restore_versao_inexistente_retorna_exit_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "memoria-sem-versao",
            "--type",
            "user",
            "--description",
            "desc",
            "--body",
            "corpo",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args(["restore", "--name", "memoria-sem-versao", "--version", "99"])
        .assert()
        .failure()
        .code(4);
}

// ---------------------------------------------------------------------------
// forget+purge regression (FTS5 external-content corruption)
// ---------------------------------------------------------------------------

#[test]
fn test_forget_purge_does_not_corrupt_fts_index() {
    // Regression: forget.rs previously executed `DELETE FROM fts_memories WHERE rowid=?`
    // directly, corrupting the FTS5 external-content index. The corruption only appeared
    // when purge ran a physical DELETE on memories triggering trg_fts_ad.
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    for i in 0..3 {
        let nome = format!("fts-reg-{i}");
        cmd(&tmp)
            .args([
                "remember",
                "--name",
                &nome,
                "--type",
                "user",
                "--description",
                "regression",
                "--body",
                &format!("corpo fts regression {i}"),
            ])
            .assert()
            .success();

        cmd(&tmp)
            .args(["forget", "--name", &nome])
            .assert()
            .success();

        cmd(&tmp)
            .args(["purge", "--name", &nome, "--retention-days", "0"])
            .assert()
            .success();
    }

    let output = cmd(&tmp)
        .arg("health")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        json["integrity"], "ok",
        "PRAGMA integrity_check DEVE permanecer ok após ciclos forget+purge"
    );
}

// ---------------------------------------------------------------------------
// Helpers para testes de grafo (link, unlink, related, graph, cleanup-orphans)
// ---------------------------------------------------------------------------

/// Creates a memory with entities attached via entities-file to populate the graph.
fn seed_memory_with_entities(
    tmp: &TempDir,
    memory_name: &str,
    entities_json: &str,
) -> std::path::PathBuf {
    let entities_path = tmp.path().join(format!("entities-{memory_name}.json"));
    std::fs::write(&entities_path, entities_json).unwrap();

    cmd(tmp)
        .args([
            "remember",
            "--name",
            memory_name,
            "--type",
            "project",
            "--description",
            "seed memory for graph tests",
            "--body",
            "body",
            "--entities-file",
            entities_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    entities_path
}

// ---------------------------------------------------------------------------
// link
// ---------------------------------------------------------------------------

#[test]
fn test_link_creates_explicit_relationship() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    seed_memory_with_entities(
        &tmp,
        "link-seed",
        r#"[
            {"name":"projeto-alpha","entity_type":"project","description":null},
            {"name":"tokio","entity_type":"tool","description":null}
        ]"#,
    );

    let output = cmd(&tmp)
        .args([
            "link",
            "--from",
            "projeto-alpha",
            "--to",
            "tokio",
            "--relation",
            "uses",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["action"], "created");
    assert_eq!(json["from"], "projeto-alpha");
    assert_eq!(json["to"], "tokio");
    assert_eq!(json["relation"], "uses");
    assert!((json["weight"].as_f64().unwrap() - 0.5).abs() < 1e-9);
}

#[test]
fn test_link_idempotente_retorna_already_exists() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    seed_memory_with_entities(
        &tmp,
        "link-idem",
        r#"[
            {"name":"servico-x","entity_type":"project","description":null},
            {"name":"banco-y","entity_type":"tool","description":null}
        ]"#,
    );

    cmd(&tmp)
        .args([
            "link",
            "--from",
            "servico-x",
            "--to",
            "banco-y",
            "--relation",
            "depends-on",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args([
            "link",
            "--from",
            "servico-x",
            "--to",
            "banco-y",
            "--relation",
            "depends-on",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["action"], "already_exists");
}

#[test]
fn test_link_entidade_inexistente_retorna_exit_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "link",
            "--from",
            "nao-existe-a",
            "--to",
            "nao-existe-b",
            "--relation",
            "uses",
        ])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn test_link_reflexivo_retorna_exit_1() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "link",
            "--from",
            "mesmo-nome",
            "--to",
            "mesmo-nome",
            "--relation",
            "uses",
        ])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn test_link_peso_invalido_retorna_exit_1() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "link",
            "--from",
            "a",
            "--to",
            "b",
            "--relation",
            "uses",
            "--weight",
            "1.5",
        ])
        .assert()
        .failure()
        .code(1);
}

// ---------------------------------------------------------------------------
// unlink
// ---------------------------------------------------------------------------

#[test]
fn test_unlink_removes_existing_relationship() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    seed_memory_with_entities(
        &tmp,
        "unlink-seed",
        r#"[
            {"name":"ent-u-a","entity_type":"project","description":null},
            {"name":"ent-u-b","entity_type":"tool","description":null}
        ]"#,
    );

    cmd(&tmp)
        .args([
            "link",
            "--from",
            "ent-u-a",
            "--to",
            "ent-u-b",
            "--relation",
            "uses",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args([
            "unlink",
            "--from",
            "ent-u-a",
            "--to",
            "ent-u-b",
            "--relation",
            "uses",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["action"], "deleted");
    assert_eq!(json["from_name"], "ent-u-a");
    assert_eq!(json["to_name"], "ent-u-b");
}

#[test]
fn test_unlink_relacao_inexistente_retorna_exit_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    seed_memory_with_entities(
        &tmp,
        "unlink-inexistente-seed",
        r#"[
            {"name":"ent-ui-a","entity_type":"project","description":null},
            {"name":"ent-ui-b","entity_type":"tool","description":null}
        ]"#,
    );

    cmd(&tmp)
        .args([
            "unlink",
            "--from",
            "ent-ui-a",
            "--to",
            "ent-ui-b",
            "--relation",
            "uses",
        ])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn test_unlink_entidade_ausente_retorna_exit_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "unlink",
            "--from",
            "nenhuma-a",
            "--to",
            "nenhuma-b",
            "--relation",
            "uses",
        ])
        .assert()
        .failure()
        .code(4);
}

// ---------------------------------------------------------------------------
// regression: INSERT OR REPLACE on vec_entities (vec0 does not support REPLACE)
// ---------------------------------------------------------------------------

#[test]
fn test_remember_does_not_duplicate_vec_entities_for_shared_entity() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // First memory with entity "entidade-comum".
    seed_memory_with_entities(
        &tmp,
        "memoria-primeiro",
        r#"[{"name":"entidade-comum","entity_type":"concept","description":null}]"#,
    );

    // Second memory reuses the SAME entity — vec0 does not tolerate duplicate INSERT OR REPLACE.
    // DEVE ter sucesso sem UNIQUE constraint error.
    seed_memory_with_entities(
        &tmp,
        "memoria-segundo",
        r#"[{"name":"entidade-comum","entity_type":"concept","description":null}]"#,
    );

    // Third memory also reuses it, ensuring robustness with multiple duplicates.
    seed_memory_with_entities(
        &tmp,
        "memoria-terceiro",
        r#"[{"name":"entidade-comum","entity_type":"concept","description":null}]"#,
    );
}

// ---------------------------------------------------------------------------
// related
// ---------------------------------------------------------------------------

#[test]
fn test_related_finds_memories_via_graph() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // Memory 1 and 2 share the entity "projeto-compartilhado".
    seed_memory_with_entities(
        &tmp,
        "memoria-um",
        r#"[{"name":"projeto-compartilhado","entity_type":"project","description":null}]"#,
    );
    seed_memory_with_entities(
        &tmp,
        "memoria-dois",
        r#"[{"name":"projeto-compartilhado","entity_type":"project","description":null}]"#,
    );

    // Relacionamento artificial para garantir hop>=1.
    seed_memory_with_entities(
        &tmp,
        "memoria-link",
        r#"[
            {"name":"projeto-compartilhado","entity_type":"project","description":null},
            {"name":"ferramenta-x","entity_type":"tool","description":null}
        ]"#,
    );
    cmd(&tmp)
        .args([
            "link",
            "--from",
            "projeto-compartilhado",
            "--to",
            "ferramenta-x",
            "--relation",
            "uses",
            "--weight",
            "0.9",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["related", "--name", "memoria-um"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json["results"]
        .as_array()
        .expect("related retorna results array");
    // should contain at least one of the other two memories via hop
    let names: Vec<&str> = arr.iter().filter_map(|v| v["name"].as_str()).collect();
    assert!(
        names.contains(&"memoria-link"),
        "esperava memoria-link em {names:?}"
    );
}

#[test]
fn test_related_nonexistent_memory_returns_exit_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args(["related", "--name", "nao-existe-mem"])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn test_related_returns_empty_when_memory_has_no_entities() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "sem-entidades",
            "--type",
            "user",
            "--description",
            "memoria solitaria",
            "--body",
            "corpo",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["related", "--name", "sem-entidades"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["results"].as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// graph (export)
// ---------------------------------------------------------------------------

#[test]
fn test_graph_export_json_estrutura_correta() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    seed_memory_with_entities(
        &tmp,
        "graph-seed-json",
        r#"[
            {"name":"graph-ent-a","entity_type":"project","description":null},
            {"name":"graph-ent-b","entity_type":"tool","description":null}
        ]"#,
    );
    cmd(&tmp)
        .args([
            "link",
            "--from",
            "graph-ent-a",
            "--to",
            "graph-ent-b",
            "--relation",
            "uses",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["graph", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json["nodes"].is_array());
    assert!(json["edges"].is_array());
    assert!(json["nodes"].as_array().unwrap().len() >= 2);
    assert!(!json["edges"].as_array().unwrap().is_empty());
}

#[test]
fn test_graph_stdin_preserves_entity_type_when_creating_relationships() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let payload = r#"{
        "entities": [
            {"name": "tipo-tool", "entity_type": "tool"},
            {"name": "tipo-file", "entity_type": "file"}
        ],
        "relationships": [
            {"source": "tipo-tool", "target": "tipo-file", "relation": "uses", "strength": 0.9}
        ]
    }"#;

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "grafo-tipado",
            "--type",
            "project",
            "--description",
            "grafo tipado via stdin",
            "--graph-stdin",
        ])
        .write_stdin(payload)
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["graph", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let nodes = json["nodes"].as_array().unwrap();
    let tipo_tool = nodes
        .iter()
        .find(|node| node["name"] == "tipo-tool")
        .expect("tipo-tool deve existir");
    let tipo_file = nodes
        .iter()
        .find(|node| node["name"] == "tipo-file")
        .expect("tipo-file deve existir");

    assert_eq!(tipo_tool["type"], "tool");
    assert_eq!(tipo_file["type"], "file");
}

#[test]
fn test_graph_stdin_accepts_from_to_aliases_and_hyphenated_relation() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let payload = r#"{
        "entities": [
            {"name": "alias-tool", "entity_type": "tool"},
            {"name": "alias-file", "entity_type": "file"}
        ],
        "relationships": [
            {"from": "alias-tool", "to": "alias-file", "relation": "depends-on", "strength": 0.7}
        ]
    }"#;

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "grafo-aliases",
            "--type",
            "project",
            "--description",
            "grafo com aliases de relacionamento",
            "--graph-stdin",
        ])
        .write_stdin(payload)
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["graph", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert!(edges.iter().any(|edge| {
        edge["from"] == "alias-tool"
            && edge["to"] == "alias-file"
            && edge["relation"] == "depends_on"
    }));
}

#[test]
fn test_graph_stdin_with_skip_extraction_persists_explicit_graph() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let payload = r#"{
        "entities": [
            {"name": "skip-tool", "entity_type": "tool"},
            {"name": "skip-file", "entity_type": "file"}
        ],
        "relationships": [
            {"source": "skip-tool", "target": "skip-file", "relation": "uses", "strength": 0.8}
        ]
    }"#;

    let remember_output = cmd(&tmp)
        .args([
            "remember",
            "--name",
            "grafo-skip",
            "--type",
            "project",
            "--description",
            "grafo explicito com skip",
            "--skip-extraction",
            "--graph-stdin",
            "--json",
        ])
        .write_stdin(payload)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let remember_json: serde_json::Value = serde_json::from_slice(&remember_output).unwrap();
    assert_eq!(remember_json["entities_persisted"], 2);
    assert_eq!(remember_json["relationships_persisted"], 1);

    let output = cmd(&tmp)
        .args(["graph", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["name"] == "skip-tool" && node["type"] == "tool"));
    assert_eq!(json["edges"].as_array().unwrap().len(), 1);
}

#[test]
fn test_graph_stdin_accepts_body_in_same_payload() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let payload = r#"{
        "body": "corpo textual enviado junto com grafo explicito",
        "entities": [
            {"name": "payload-tool", "entity_type": "tool"},
            {"name": "payload-file", "entity_type": "file"}
        ],
        "relationships": [
            {"source": "payload-tool", "target": "payload-file", "relation": "uses", "strength": 0.8}
        ]
    }"#;

    let remember_output = cmd(&tmp)
        .args([
            "remember",
            "--name",
            "grafo-com-body",
            "--type",
            "project",
            "--description",
            "grafo com body via stdin",
            "--skip-extraction",
            "--graph-stdin",
            "--json",
        ])
        .write_stdin(payload)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let remember_json: serde_json::Value = serde_json::from_slice(&remember_output).unwrap();
    assert_eq!(remember_json["entities_persisted"], 2);
    assert_eq!(remember_json["relationships_persisted"], 1);

    let read_output = cmd(&tmp)
        .args(["read", "--name", "grafo-com-body", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let read_json: serde_json::Value = serde_json::from_slice(&read_output).unwrap();
    assert_eq!(
        read_json["body"],
        "corpo textual enviado junto com grafo explicito"
    );
}

#[test]
fn test_remember_accepts_document_above_old_limit_with_chunks() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let body = (0..900)
        .map(|i| format!("termo{i} documento real para chunk seguro"))
        .collect::<Vec<_>>()
        .join(" ");

    let output = cmd(&tmp)
        .args([
            "remember",
            "--name",
            "doc-acima-limite-antigo",
            "--type",
            "reference",
            "--description",
            "documento acima do limite antigo",
            "--body",
            &body,
            "--skip-extraction",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(
        json["chunks_created"].as_u64().unwrap_or_default() > 1,
        "documento deve usar caminho multi-chunk"
    );
}

#[test]
fn test_remember_rejects_body_above_new_operational_limit() {
    let tmp = TempDir::new().unwrap();
    let body_path = tmp.path().join("body-grande.txt");
    std::fs::write(&body_path, "x".repeat(512_001)).unwrap();

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "body-grande",
            "--type",
            "reference",
            "--description",
            "body acima do limite novo",
            "--body-file",
            body_path.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .failure()
        .code(6);
}

#[test]
fn test_graph_json_flag_vence_format_dot() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .args(["graph", "--json", "--format", "dot"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json["nodes"].is_array());
    assert!(json["edges"].is_array());
}

#[test]
fn test_graph_json_flag_vence_format_mermaid() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .args(["graph", "--json", "--format", "mermaid"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json["nodes"].is_array());
    assert!(json["edges"].is_array());
}

#[test]
fn test_graph_json_flag_keeps_stdout_even_with_output() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output_path = tmp.path().join("graph.dot");
    let output_path_str = output_path.to_str().unwrap();

    let output = cmd(&tmp)
        .args([
            "graph",
            "--json",
            "--format",
            "dot",
            "--output",
            output_path_str,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json["nodes"].is_array());
    assert!(
        !output_path.exists(),
        "--json deve manter o contrato stdout em vez de gravar DOT"
    );
}

#[test]
fn test_graph_stats_json_flag_vence_format_text() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .args(["graph", "stats", "--json", "--format", "text"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json["node_count"].is_number());
    assert!(json["edge_count"].is_number());
}

#[test]
fn test_graph_export_dot_contem_digraph() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    seed_memory_with_entities(
        &tmp,
        "graph-seed-dot",
        r#"[
            {"name":"dot-a","entity_type":"project","description":null},
            {"name":"dot-b","entity_type":"tool","description":null}
        ]"#,
    );
    cmd(&tmp)
        .args([
            "link",
            "--from",
            "dot-a",
            "--to",
            "dot-b",
            "--relation",
            "uses",
        ])
        .assert()
        .success();

    let out = cmd(&tmp)
        .args(["graph", "--format", "dot"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let rendered = String::from_utf8(out).unwrap();
    assert!(rendered.contains("digraph sqlite-graphrag"));
    assert!(rendered.contains("dot_a"));
    assert!(rendered.contains("dot_b"));
    assert!(rendered.contains("uses"));
}

#[test]
fn test_graph_export_mermaid_contem_graph_lr() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    seed_memory_with_entities(
        &tmp,
        "graph-seed-mermaid",
        r#"[{"name":"mer-a","entity_type":"project","description":null}]"#,
    );

    let out = cmd(&tmp)
        .args(["graph", "--format", "mermaid"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let rendered = String::from_utf8(out).unwrap();
    assert!(rendered.contains("graph LR"));
    assert!(rendered.contains("mer_a"));
}

// ---------------------------------------------------------------------------
// cleanup-orphans
// ---------------------------------------------------------------------------

#[test]
fn test_cleanup_orphans_remove_entidades_orfas() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // Create a memory with linked entities
    seed_memory_with_entities(
        &tmp,
        "co-mem-ligada",
        r#"[{"name":"co-ent-ligada","entity_type":"project","description":null}]"#,
    );

    // Create a memory with additional entities and remove it, leaving orphan entities
    seed_memory_with_entities(
        &tmp,
        "co-mem-descartada",
        r#"[{"name":"co-ent-orfa","entity_type":"project","description":null}]"#,
    );
    cmd(&tmp)
        .args(["forget", "--name", "co-mem-descartada"])
        .assert()
        .success();
    cmd(&tmp)
        .args([
            "purge",
            "--name",
            "co-mem-descartada",
            "--retention-days",
            "0",
        ])
        .assert()
        .success();

    // Dry-run counts orphans without removing
    let output = cmd(&tmp)
        .args(["cleanup-orphans", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let dry: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(dry["dry_run"], true);
    assert!(dry["orphan_count"].as_u64().unwrap() >= 1);
    assert_eq!(dry["deleted"].as_u64().unwrap(), 0);

    // Real execution removes the orphans
    let output = cmd(&tmp)
        .args(["cleanup-orphans", "--yes"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let done: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(done["dry_run"], false);
    assert!(done["deleted"].as_u64().unwrap() >= 1);
}

#[test]
fn test_cleanup_orphans_without_orphans_returns_zero() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    seed_memory_with_entities(
        &tmp,
        "co-limpo",
        r#"[{"name":"co-ent-limpa","entity_type":"project","description":null}]"#,
    );

    let output = cmd(&tmp)
        .args(["cleanup-orphans"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["orphan_count"], 0);
    assert_eq!(json["deleted"], 0);
}
