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
// init
// ---------------------------------------------------------------------------

#[test]
fn test_init_cria_arquivo_sqlite() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    assert!(!db_path.exists(), "banco nao deve existir antes do init");

    cmd(&tmp).arg("init").assert().success();

    assert!(db_path.exists(), "banco deve existir apos o init");
}

#[test]
fn test_init_cria_banco_local_no_diretorio_da_invocacao() {
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
fn test_crud_usa_graphrag_sqlite_no_diretorio_da_invocacao() {
    let pasta = TempDir::new().unwrap();
    let banco = pasta.path().join("graphrag.sqlite");

    assert!(
        !banco.exists(),
        "banco local nao deve existir antes do init no diretorio da invocacao"
    );

    isolated_cmd_in(pasta.path())
        .arg("init")
        .assert()
        .success();

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
fn test_init_retorna_json_com_status_ok() {
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
fn test_health_falha_sem_init() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp).arg("health").assert().failure();
}

#[test]
fn test_health_ok_apos_init() {
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
// remember
// ---------------------------------------------------------------------------

#[test]
fn test_remember_cria_memoria() {
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
fn test_remember_duplicata_retorna_exit_2() {
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
fn test_remember_force_merge_atualiza() {
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

// ---------------------------------------------------------------------------
// read
// ---------------------------------------------------------------------------

#[test]
fn test_read_memoria_existente() {
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
fn test_read_inexistente_retorna_exit_4() {
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
fn test_list_memorias() {
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
fn test_forget_inexistente_retorna_exit_4() {
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
fn test_purge_remove_memoria_soft_deleted() {
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
fn test_purge_yes_flag_e_noop() {
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
fn test_namespace_detect_retorna_global_sem_config_local() {
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
fn test_sync_safe_copy_cria_snapshot_consistente() {
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
fn test_stats_retorna_contagens() {
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
    assert_eq!(json["schema_version"], "5");
}

#[test]
fn test_stats_falha_sem_init() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp).arg("stats").assert().failure();
}

// ---------------------------------------------------------------------------
// rename
// ---------------------------------------------------------------------------

#[test]
fn test_rename_memoria_funciona() {
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
fn test_rename_inexistente_retorna_exit_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args(["rename", "--name", "nao-existe", "--new-name", "novo-nome"])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn test_rename_new_name_invalido_retorna_exit_1() {
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
fn test_edit_memoria_funciona() {
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
fn test_restore_memoria_funciona() {
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
// regressão forget+purge (FTS5 external-content corruption)
// ---------------------------------------------------------------------------

#[test]
fn test_forget_purge_nao_corrompe_fts_index() {
    // Regressão: forget.rs previamente executava `DELETE FROM fts_memories WHERE rowid=?`
    // direto, corrompendo índice FTS5 external-content. A corrupção só aparecia
    // quando purge executava DELETE físico em memories disparando trg_fts_ad.
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

/// Cria uma memória com entidades anexadas via entities-file para popular o grafo.
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
fn test_link_cria_relacao_explicita() {
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
fn test_unlink_remove_relacao_existente() {
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
// regressão: INSERT OR REPLACE em vec_entities (vec0 não suporta REPLACE)
// ---------------------------------------------------------------------------

#[test]
fn test_remember_nao_duplica_vec_entities_para_entidade_compartilhada() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // Primeira memória com entidade "entidade-comum".
    seed_memory_with_entities(
        &tmp,
        "memoria-primeiro",
        r#"[{"name":"entidade-comum","entity_type":"concept","description":null}]"#,
    );

    // Segunda memória reutiliza a MESMA entidade — vec0 não tolera INSERT OR REPLACE duplicado.
    // DEVE ter sucesso sem UNIQUE constraint error.
    seed_memory_with_entities(
        &tmp,
        "memoria-segundo",
        r#"[{"name":"entidade-comum","entity_type":"concept","description":null}]"#,
    );

    // Terceira memória também reutiliza, garantindo robustez com múltiplas duplicações.
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
fn test_related_encontra_memorias_via_grafo() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // Memória 1 e 2 compartilham a entidade "projeto-compartilhado".
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
    // deve conter pelo menos uma das outras duas memórias via hop
    let names: Vec<&str> = arr.iter().filter_map(|v| v["name"].as_str()).collect();
    assert!(
        names.contains(&"memoria-link"),
        "esperava memoria-link em {names:?}"
    );
}

#[test]
fn test_related_memoria_inexistente_retorna_exit_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args(["related", "--name", "nao-existe-mem"])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn test_related_retorna_vazio_quando_memoria_sem_entidades() {
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

    // Cria uma memória com entidades vinculadas
    seed_memory_with_entities(
        &tmp,
        "co-mem-ligada",
        r#"[{"name":"co-ent-ligada","entity_type":"project","description":null}]"#,
    );

    // Cria uma memória com entidades adicionais e remove a memória, deixando as entidades órfãs
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

    // Dry-run conta órfãos sem remover
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

    // Execução real remove os órfãos
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
fn test_cleanup_orphans_sem_orfaos_retorna_zero() {
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
