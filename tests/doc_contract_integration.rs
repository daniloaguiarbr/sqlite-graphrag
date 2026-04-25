#![cfg(feature = "slow-tests")]

// Suite 1 — Validação de contrato JSON para todos os 25 subcomandos.
// Ground truth: docs/schemas/*.schema.json (gerados pela task #7).
// Cada teste verifica: exit code esperado + JSON válido + required keys presentes.
use assert_cmd::Command;
use serde_json::Value;
use serial_test::serial;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        Self { tmp }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
        c.env(
            "SQLITE_GRAPHRAG_DB_PATH",
            self.tmp.path().join("test.sqlite"),
        );
        c.env("SQLITE_GRAPHRAG_CACHE_DIR", self.tmp.path().join("cache"));
        c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
        c.arg("--skip-memory-guard");
        c
    }

    fn init(&self) {
        self.cmd().arg("init").assert().success();
    }

    fn remember(&self, name: &str, body: &str) -> Value {
        let out = self
            .cmd()
            .args([
                "remember",
                "--name",
                name,
                "--type",
                "project",
                "--description",
                "desc-contrato",
                "--namespace",
                "global",
                "--body",
                body,
            ])
            .output()
            .unwrap();
        assert!(out.status.success(), "remember falhou: {:?}", out.status);
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn remember_with_entities(&self, name: &str, body: &str) -> (String, String) {
        let ent_a = format!("Ent{}A", name.replace('-', ""));
        let ent_b = format!("Ent{}B", name.replace('-', ""));
        let ents_path = self.tmp.path().join(format!("{name}_ents.json"));
        let ents_json = format!(
            r#"[{{"name":"{ent_a}","entity_type":"concept"}},{{"name":"{ent_b}","entity_type":"concept"}}]"#
        );
        std::fs::write(&ents_path, &ents_json).unwrap();
        let out = self
            .cmd()
            .args([
                "remember",
                "--name",
                name,
                "--type",
                "project",
                "--description",
                "desc-entidades",
                "--body",
                body,
                "--entities-file",
                ents_path.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "remember com entidades falhou: {:?}",
            out.status
        );
        (ent_a, ent_b)
    }

    fn parse_stdout(out: &std::process::Output) -> Value {
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "JSON inválido: {e}\nstdout: {:?}",
                String::from_utf8_lossy(&out.stdout)
            )
        })
    }
}

/// Verifica que todas as `keys` existem no objeto JSON `v`.
fn assert_has_keys(cmd: &str, v: &Value, keys: &[&str]) {
    let obj = v
        .as_object()
        .unwrap_or_else(|| panic!("[{cmd}] esperado JSON object, recebido: {v}"));
    for key in keys {
        assert!(
            obj.contains_key(*key),
            "[{cmd}] key ausente: '{key}'. Keys presentes: {:?}",
            obj.keys().collect::<Vec<_>>()
        );
    }
}

/// Verifica que todas as `keys` existem em cada item de um JSON array.
fn assert_array_items_have_keys(cmd: &str, v: &Value, keys: &[&str]) {
    let arr = v
        .as_array()
        .unwrap_or_else(|| panic!("[{cmd}] esperado JSON array, recebido: {v}"));
    for (i, item) in arr.iter().enumerate() {
        let obj = item
            .as_object()
            .unwrap_or_else(|| panic!("[{cmd}] item[{i}] não é object: {item}"));
        for key in keys {
            assert!(
                obj.contains_key(*key),
                "[{cmd}] item[{i}] key ausente: '{key}'. Keys: {:?}",
                obj.keys().collect::<Vec<_>>()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 01 — init
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_01_init() {
    let env = Env::new();
    let out = env.cmd().arg("init").output().unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "init",
        &json,
        &[
            "db_path",
            "schema_version",
            "model",
            "dim",
            "namespace",
            "status",
        ],
    );
}

// ---------------------------------------------------------------------------
// 02 — remember
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_02_remember() {
    let env = Env::new();
    env.init();
    let json = env.remember("mem-contrato-remember", "corpo do teste de contrato");
    assert_has_keys(
        "remember",
        &json,
        &[
            "memory_id",
            "name",
            "namespace",
            "action",
            "operation",
            "version",
            "entities_persisted",
            "relationships_persisted",
            "chunks_created",
            "warnings",
            "created_at",
            "created_at_iso",
            "elapsed_ms",
        ],
    );
    assert!(json["memory_id"].is_number(), "memory_id deve ser número");
    assert!(
        json["elapsed_ms"].as_u64().unwrap_or(0) < 60_000,
        "elapsed_ms razoável"
    );
}

// ---------------------------------------------------------------------------
// 03 — health
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_03_health() {
    let env = Env::new();
    env.init();
    let out = env.cmd().arg("health").output().unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "health",
        &json,
        &[
            "status",
            "db_path",
            "schema_version",
            "counts",
            "checks",
            "elapsed_ms",
        ],
    );
    assert!(json["counts"]["memories"].is_number());
    assert!(json["counts"]["entities"].is_number());
    assert!(json["counts"]["relationships"].is_number());
    assert!(json["checks"].is_array());
}

// ---------------------------------------------------------------------------
// 04 — stats
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_04_stats() {
    let env = Env::new();
    env.init();
    let out = env.cmd().arg("stats").output().unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "stats",
        &json,
        &[
            "memories",
            "memories_total",
            "entities",
            "entities_total",
            "relationships",
            "relationships_total",
            "edges",
            "chunks_total",
            "avg_body_len",
            "namespaces",
            "db_size_bytes",
            "db_bytes",
            "schema_version",
        ],
    );
}

// ---------------------------------------------------------------------------
// 05 — list
// O contrato publico atual exige objeto com {elapsed_ms, items:[...]}.
// Aceitar array root aqui enfraquece a deteccao de regressao documental.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_05_list() {
    let env = Env::new();
    env.init();
    env.remember("mem-list-01", "conteúdo para listar");

    let out = env.cmd().arg("list").output().unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);

    let items = json
        .get("items")
        .unwrap_or_else(|| panic!("list: esperado objeto com {{items:[...]}}, recebido: {json}"));

    assert!(items.is_array(), "list: 'items' nao e array: {items}");
    let arr = items.as_array().unwrap();
    if !arr.is_empty() {
        assert_array_items_have_keys(
            "list",
            items,
            &[
                "id",
                "memory_id",
                "name",
                "namespace",
                "type",
                "description",
                "snippet",
                "updated_at",
                "updated_at_iso",
            ],
        );
    }
}

// ---------------------------------------------------------------------------
// 06 — read
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_06_read() {
    let env = Env::new();
    env.init();
    env.remember("mem-read-contrato", "corpo para leitura de contrato");

    let out = env
        .cmd()
        .args(["read", "--name", "mem-read-contrato"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "read",
        &json,
        &[
            "id",
            "memory_id",
            "namespace",
            "name",
            "type",
            "memory_type",
            "description",
            "body",
            "body_hash",
            "source",
            "metadata",
            "version",
            "created_at",
            "created_at_iso",
            "updated_at",
            "updated_at_iso",
        ],
    );
}

// ---------------------------------------------------------------------------
// 07 — forget
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_07_forget() {
    let env = Env::new();
    env.init();
    env.remember("mem-forget-contrato", "corpo para soft-delete");

    let out = env
        .cmd()
        .args(["forget", "--name", "mem-forget-contrato"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys("forget", &json, &["forgotten", "name", "namespace"]);
    assert_eq!(json["forgotten"], true);
}

// ---------------------------------------------------------------------------
// 08 — purge
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_08_purge() {
    let env = Env::new();
    env.init();
    env.remember("mem-purge-contrato", "corpo para purge");
    env.cmd()
        .args(["forget", "--name", "mem-purge-contrato"])
        .assert()
        .success();

    let out = env.cmd().args(["purge", "--yes"]).output().unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "purge",
        &json,
        &["purged_count", "bytes_freed", "dry_run", "namespace"],
    );
    assert!(json["purged_count"].is_number());
}

// ---------------------------------------------------------------------------
// 09 — rename
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_09_rename() {
    let env = Env::new();
    env.init();
    env.remember("mem-rename-src", "corpo rename");

    let out = env
        .cmd()
        .args([
            "rename",
            "--name",
            "mem-rename-src",
            "--new-name",
            "mem-rename-dst",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys("rename", &json, &["memory_id", "name", "version"]);
    assert_eq!(json["name"], "mem-rename-dst");
}

// ---------------------------------------------------------------------------
// 10 — edit
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_10_edit() {
    let env = Env::new();
    env.init();
    env.remember("mem-edit-contrato", "corpo original");

    let out = env
        .cmd()
        .args([
            "edit",
            "--name",
            "mem-edit-contrato",
            "--body",
            "corpo editado contrato",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys("edit", &json, &["memory_id", "name", "action", "version"]);
    assert_eq!(json["action"], "updated");
}

// ---------------------------------------------------------------------------
// 11 — history
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_11_history() {
    let env = Env::new();
    env.init();
    env.remember("mem-history-contrato", "corpo versão 1");
    env.cmd()
        .args([
            "edit",
            "--name",
            "mem-history-contrato",
            "--body",
            "corpo versão 2",
        ])
        .assert()
        .success();

    let out = env
        .cmd()
        .args(["history", "--name", "mem-history-contrato"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys("history", &json, &["name", "namespace", "versions"]);
    assert!(json["versions"].is_array());
    let versions = json["versions"].as_array().unwrap();
    assert!(!versions.is_empty(), "deve ter pelo menos 1 versão");
    // Valida keys de cada versão
    for v in versions {
        let obj = v.as_object().unwrap();
        for key in &[
            "version",
            "name",
            "type",
            "description",
            "body",
            "metadata",
            "change_reason",
            "changed_by",
            "created_at",
            "created_at_iso",
        ] {
            assert!(obj.contains_key(*key), "versão sem key '{key}'");
        }
    }
}

// ---------------------------------------------------------------------------
// 12 — restore
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_12_restore() {
    let env = Env::new();
    env.init();
    env.remember("mem-restore-contrato", "corpo versão 1");
    env.cmd()
        .args([
            "edit",
            "--name",
            "mem-restore-contrato",
            "--body",
            "corpo versão 2",
        ])
        .assert()
        .success();

    // Pega versão 1 via history
    let h_out = env
        .cmd()
        .args(["history", "--name", "mem-restore-contrato"])
        .output()
        .unwrap();
    let h_json: Value = serde_json::from_slice(&h_out.stdout).unwrap();
    let ver = h_json["versions"]
        .as_array()
        .and_then(|v| v.iter().find(|e| e["version"] == 1))
        .and_then(|v| v["version"].as_i64())
        .unwrap_or(1);

    let out = env
        .cmd()
        .args([
            "restore",
            "--name",
            "mem-restore-contrato",
            "--version",
            &ver.to_string(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "restore",
        &json,
        &["memory_id", "name", "version", "restored_from"],
    );
}

// ---------------------------------------------------------------------------
// 13 — recall
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_13_recall() {
    let env = Env::new();
    env.init();
    env.remember(
        "mem-recall-contrato",
        "texto de busca semântica de contrato",
    );

    let out = env.cmd().args(["recall", "contrato"]).output().unwrap();
    // exit 0 (encontrou) ou 4 (not found) são válidos
    let code = out.status.code().unwrap_or(1);
    assert!(
        code == 0 || code == 4,
        "recall exit code inesperado: {code}"
    );

    if code == 0 {
        let json = Env::parse_stdout(&out);
        assert_has_keys(
            "recall",
            &json,
            &[
                "query",
                "k",
                "direct_matches",
                "graph_matches",
                "results",
                "elapsed_ms",
            ],
        );
        assert!(json["results"].is_array());
    }
}

// ---------------------------------------------------------------------------
// 14 — hybrid-search
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_14_hybrid_search() {
    let env = Env::new();
    env.init();
    env.remember("mem-hybrid-contrato", "texto para hybrid search contrato");

    let out = env
        .cmd()
        .args(["hybrid-search", "contrato"])
        .output()
        .unwrap();
    let code = out.status.code().unwrap_or(1);
    assert!(
        code == 0 || code == 4,
        "hybrid-search exit code inesperado: {code}"
    );

    if code == 0 {
        let json = Env::parse_stdout(&out);
        assert_has_keys(
            "hybrid-search",
            &json,
            &[
                "query",
                "k",
                "rrf_k",
                "weights",
                "results",
                "graph_matches",
                "elapsed_ms",
            ],
        );
        assert!(json["results"].is_array());
    }
}

// ---------------------------------------------------------------------------
// 15 — link
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_15_link() {
    let env = Env::new();
    env.init();
    let (ent_a, ent_b) = env.remember_with_entities("mem-link-contrato", "corpo link entidades");

    let out = env
        .cmd()
        .args([
            "link",
            "--from",
            &ent_a,
            "--to",
            &ent_b,
            "--relation",
            "related",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "link falhou: {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "link",
        &json,
        &[
            "action",
            "from",
            "source",
            "to",
            "target",
            "relation",
            "weight",
            "namespace",
        ],
    );
    assert_eq!(json["action"], "created");
}

// ---------------------------------------------------------------------------
// 16 — unlink
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_16_unlink() {
    let env = Env::new();
    env.init();
    let (ent_a, ent_b) =
        env.remember_with_entities("mem-unlink-contrato", "corpo unlink entidades");
    // Cria relação primeiro
    env.cmd()
        .args([
            "link",
            "--from",
            &ent_a,
            "--to",
            &ent_b,
            "--relation",
            "related",
        ])
        .assert()
        .success();

    let out = env
        .cmd()
        .args([
            "unlink",
            "--from",
            &ent_a,
            "--to",
            &ent_b,
            "--relation",
            "related",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "unlink falhou: {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "unlink",
        &json,
        &[
            "action",
            "relationship_id",
            "from_name",
            "to_name",
            "relation",
            "namespace",
        ],
    );
    assert_eq!(json["action"], "deleted");
}

// ---------------------------------------------------------------------------
// 17 — related
// O contrato publico atual exige objeto com {elapsed_ms, results:[...]}.
// Aceitar array root aqui enfraquece a deteccao de regressao documental.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_17_related() {
    let env = Env::new();
    env.init();
    let (ent_a, _ent_b) =
        env.remember_with_entities("mem-related-a", "corpo entidade A para grafo");
    let (ent_c, _ent_d) =
        env.remember_with_entities("mem-related-b", "corpo entidade B para grafo");
    // Liga as entidades para garantir que related retorna algo
    env.cmd()
        .args([
            "link",
            "--from",
            &ent_a,
            "--to",
            &ent_c,
            "--relation",
            "related",
        ])
        .assert()
        .success();

    let out = env
        .cmd()
        .args(["related", "--name", "mem-related-a"])
        .output()
        .unwrap();
    let code = out.status.code().unwrap_or(1);
    assert!(
        code == 0 || code == 4,
        "related exit code inesperado: {code}"
    );

    if code == 0 {
        let json = Env::parse_stdout(&out);
        let results = json.get("results").unwrap_or_else(|| {
            panic!("related: esperado objeto com {{results:[...]}}, recebido: {json}")
        });
        assert!(
            results.is_array(),
            "related: 'results' nao e array: {results}"
        );
        let arr = results.as_array().unwrap();
        if !arr.is_empty() {
            assert_array_items_have_keys(
                "related",
                results,
                &[
                    "memory_id",
                    "name",
                    "namespace",
                    "type",
                    "description",
                    "hop_distance",
                    "source_entity",
                    "target_entity",
                    "relation",
                    "weight",
                ],
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 18 — graph
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_18_graph() {
    let env = Env::new();
    env.init();

    let out = env
        .cmd()
        .args(["graph", "--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys("graph", &json, &["nodes", "edges"]);
    assert!(json["nodes"].is_array());
    assert!(json["edges"].is_array());
}

// ---------------------------------------------------------------------------
// 19 — namespace-detect
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_19_namespace_detect() {
    let env = Env::new();
    env.init();

    let out = env.cmd().arg("namespace-detect").output().unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "namespace-detect",
        &json,
        &["namespace", "source", "cwd", "elapsed_ms"],
    );
    assert!(json["namespace"].is_string());
}

// ---------------------------------------------------------------------------
// 20 — migrate
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_20_migrate() {
    let env = Env::new();
    env.init();

    let out = env.cmd().arg("migrate").output().unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys("migrate", &json, &["db_path", "schema_version", "status"]);
    // schema_version pode ser emitido como string ou número dependendo da implementação
    let sv = &json["schema_version"];
    assert!(
        sv.is_number() || sv.is_string(),
        "migrate schema_version deve ser número ou string, recebido: {sv}"
    );
}

// ---------------------------------------------------------------------------
// 21 — optimize
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_21_optimize() {
    let env = Env::new();
    env.init();

    let out = env.cmd().arg("optimize").output().unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys("optimize", &json, &["db_path", "status"]);
    assert_eq!(json["status"], "ok");
}

// ---------------------------------------------------------------------------
// 22 — vacuum
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_22_vacuum() {
    let env = Env::new();
    env.init();

    let out = env.cmd().arg("vacuum").output().unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "vacuum",
        &json,
        &["db_path", "size_before_bytes", "size_after_bytes", "status"],
    );
    assert_eq!(json["status"], "ok");
}

// ---------------------------------------------------------------------------
// 23 — sync-safe-copy
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_23_sync_safe_copy() {
    let env = Env::new();
    env.init();
    let dest = env.tmp.path().join("backup.sqlite");

    let out = env
        .cmd()
        .args(["sync-safe-copy", "--dest", dest.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "sync-safe-copy",
        &json,
        &["source_db_path", "dest_path", "bytes_copied", "status"],
    );
    assert_eq!(json["status"], "ok");
    assert!(dest.exists(), "arquivo de destino deve existir");
}

// ---------------------------------------------------------------------------
// 24 — cleanup-orphans
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_24_cleanup_orphans() {
    let env = Env::new();
    env.init();

    let out = env.cmd().arg("cleanup-orphans").output().unwrap();
    assert!(out.status.success());
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "cleanup-orphans",
        &json,
        &["orphan_count", "deleted", "dry_run", "namespace"],
    );
    assert!(json["orphan_count"].is_number());
}

// ---------------------------------------------------------------------------
// 25 — __debug_schema (oculto, adicionado na v2.0.5)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contract_25_debug_schema() {
    let env = Env::new();
    env.init();

    let out = env.cmd().arg("__debug_schema").output().unwrap();
    assert!(
        out.status.success(),
        "__debug_schema falhou: {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let json = Env::parse_stdout(&out);
    assert_has_keys(
        "__debug_schema",
        &json,
        &[
            "schema_version",
            "user_version",
            "objects",
            "migrations",
            "elapsed_ms",
        ],
    );
    assert!(json["objects"].is_array());
    assert!(json["migrations"].is_array());
}
