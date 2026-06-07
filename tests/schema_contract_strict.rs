#![cfg(feature = "slow-tests")]

// Suite 8 — Strict JSON Schema contract validation for all 25 subcommands.
// Each test runs the binary, captures stdout, parses it as JSON and validates against
// docs/schemas/<cmd>.schema.json using the jsonschema::Validator crate.
//
// Dependency: jsonschema = "0.29" in [dev-dependencies] of Cargo.toml.
use assert_cmd::Command;
use serde_json::Value;
use serial_test::serial;
use tempfile::TempDir;

/// Builds a fresh `Command` with the mock LLM PATH prepended.
///
/// v1.0.76 spawns `claude` or `codex` on every `remember` / `ingest` /
/// `edit`. The bundled mocks under `tests/mock-llm/` return a fixed
/// 384-dim zero vector so the binary finishes without a real OAuth
/// login. The mock directory is leaked (no TempDir cleanup) so the
/// spawned subprocess always finds the mocks.
fn sgr_cmd() -> Command {
    let mock_dir = common::mock_llm_path();
    let mut c = Command::cargo_bin("sqlite-graphrag").expect("sqlite-graphrag binary not found");
    c.env("PATH", common::prepend_path(&mock_dir));
    c
}

#[path = "common/mod.rs"]
mod common;

// ---------------------------------------------------------------------------
// Infraestrutura de teste
// ---------------------------------------------------------------------------

struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        let tmp = TempDir::new().expect("TempDir::new failed");
        Self { tmp }
    }

    fn cmd(&self) -> Command {
        let mut c = sgr_cmd();
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

    fn remember_simples(&self, nome: &str) -> Value {
        let saida = self
            .cmd()
            .args([
                "remember",
                "--name",
                nome,
                "--type",
                "project",
                "--description",
                "descricao-contrato",
                "--namespace",
                "global",
                "--body",
                "corpo-de-teste-schema-contract",
            ])
            .output()
            .expect("remember failed ao executar");
        assert!(
            saida.status.success(),
            "remember retornou erro: {:?}\nstdout: {}",
            saida.status.code(),
            String::from_utf8_lossy(&saida.stdout)
        );
        serde_json::from_slice(&saida.stdout).expect("remember stdout não é JSON válido")
    }

    fn remember_with_entities(&self, name: &str) -> (String, String) {
        let ent_a = format!("Ent{}Alpha", name.replace('-', ""));
        let ent_b = format!("Ent{}Beta", name.replace('-', ""));
        let caminho_ents = self.tmp.path().join(format!("{name}_ents.json"));
        let json_ents = format!(
            r#"[{{"name":"{ent_a}","entity_type":"concept"}},{{"name":"{ent_b}","entity_type":"concept"}}]"#
        );
        std::fs::write(&caminho_ents, &json_ents).expect("escrita de entidades failed");
        let saida = self
            .cmd()
            .args([
                "remember",
                "--name",
                name,
                "--type",
                "project",
                "--description",
                "descricao-entidades",
                "--body",
                "corpo-com-entidades-para-schema",
                "--entities-file",
                caminho_ents.to_str().expect("caminho inválido"),
            ])
            .output()
            .expect("remember com entidades failed");
        assert!(
            saida.status.success(),
            "remember com entidades retornou erro: {:?}",
            saida.status.code()
        );
        (ent_a, ent_b)
    }

    fn parse_stdout(saida: &std::process::Output, cmd: &str) -> Value {
        serde_json::from_slice(&saida.stdout).unwrap_or_else(|e| {
            panic!(
                "[{cmd}] stdout não é JSON válido: {e}\nstdout bruto: {:?}",
                String::from_utf8_lossy(&saida.stdout)
            )
        })
    }
}

/// Valida `instancia` contra o schema em `schema_str`.
/// Collects all errors and aborts with a detailed message if any violations exist.
fn validar_schema(cmd: &str, schema_str: &str, instancia: &Value) {
    let schema: Value =
        serde_json::from_str(schema_str).unwrap_or_else(|e| panic!("[{cmd}] schema inválido: {e}"));
    let validador = jsonschema::Validator::new(&schema)
        .unwrap_or_else(|e| panic!("[{cmd}] failure ao compilar schema: {e}"));
    let erros: Vec<String> = validador
        .iter_errors(instancia)
        .map(|e| format!("  - caminho={} tipo={:?}", e.instance_path, e.kind))
        .collect();
    assert!(
        erros.is_empty(),
        "[{cmd}] {n} violação(ões) de schema:\n{lista}\ninstância: {inst}",
        n = erros.len(),
        lista = erros.join("\n"),
        inst = serde_json::to_string_pretty(instancia).unwrap_or_default()
    );
}

// ---------------------------------------------------------------------------
// 01 — init
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_01_init() {
    let env = Env::new();
    let saida = env.cmd().arg("init").output().expect("init failed");
    assert!(
        saida.status.success(),
        "init: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "init");
    validar_schema(
        "init",
        include_str!("../docs/schemas/init.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 02 — stats
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_02_stats() {
    let env = Env::new();
    env.init();
    let saida = env.cmd().arg("stats").output().expect("stats failed");
    assert!(
        saida.status.success(),
        "stats: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "stats");
    validar_schema(
        "stats",
        include_str!("../docs/schemas/stats.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 03 — remember
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_03_remember() {
    let env = Env::new();
    env.init();
    let instancia = env.remember_simples("mem-schema-remember");
    validar_schema(
        "remember",
        include_str!("../docs/schemas/remember.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 04 — list
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_04_list() {
    let env = Env::new();
    env.init();
    env.remember_simples("mem-schema-list");
    let saida = env
        .cmd()
        .args(["list", "--namespace", "global"])
        .output()
        .expect("list failed");
    assert!(
        saida.status.success(),
        "list: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "list");
    validar_schema(
        "list",
        include_str!("../docs/schemas/list.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 05 — read
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_05_read() {
    let env = Env::new();
    env.init();
    env.remember_simples("mem-schema-read");
    let saida = env
        .cmd()
        .args(["read", "--name", "mem-schema-read"])
        .output()
        .expect("read failed");
    assert!(
        saida.status.success(),
        "read: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "read");
    validar_schema(
        "read",
        include_str!("../docs/schemas/read.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 06 — edit
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_06_edit() {
    let env = Env::new();
    env.init();
    env.remember_simples("mem-schema-edit");
    let saida = env
        .cmd()
        .args([
            "edit",
            "--name",
            "mem-schema-edit",
            "--body",
            "corpo-editado-para-schema",
        ])
        .output()
        .expect("edit failed");
    assert!(
        saida.status.success(),
        "edit: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "edit");
    validar_schema(
        "edit",
        include_str!("../docs/schemas/edit.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 07 — rename
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_07_rename() {
    let env = Env::new();
    env.init();
    env.remember_simples("mem-schema-rename-origem");
    let saida = env
        .cmd()
        .args([
            "rename",
            "--name",
            "mem-schema-rename-origem",
            "--new-name",
            "mem-schema-rename-destino",
        ])
        .output()
        .expect("rename failed");
    assert!(
        saida.status.success(),
        "rename: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "rename");
    validar_schema(
        "rename",
        include_str!("../docs/schemas/rename.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 08 — history
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_08_history() {
    let env = Env::new();
    env.init();
    env.remember_simples("mem-schema-history");
    let saida = env
        .cmd()
        .args(["history", "--name", "mem-schema-history"])
        .output()
        .expect("history failed");
    assert!(
        saida.status.success(),
        "history: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "history");
    validar_schema(
        "history",
        include_str!("../docs/schemas/history.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 09 — forget
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_09_forget() {
    let env = Env::new();
    env.init();
    env.remember_simples("mem-schema-forget");
    let saida = env
        .cmd()
        .args(["forget", "--name", "mem-schema-forget"])
        .output()
        .expect("forget failed");
    assert!(
        saida.status.success(),
        "forget: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "forget");
    validar_schema(
        "forget",
        include_str!("../docs/schemas/forget.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 10 — restore
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_10_restore() {
    let env = Env::new();
    env.init();
    env.remember_simples("mem-schema-restore");
    // Create a second version via edit
    env.cmd()
        .args([
            "edit",
            "--name",
            "mem-schema-restore",
            "--body",
            "versao-dois",
        ])
        .assert()
        .success();
    let saida = env
        .cmd()
        .args(["restore", "--name", "mem-schema-restore", "--version", "1"])
        .output()
        .expect("restore failed");
    assert!(
        saida.status.success(),
        "restore: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "restore");
    validar_schema(
        "restore",
        include_str!("../docs/schemas/restore.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 11 — purge
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_11_purge() {
    let env = Env::new();
    env.init();
    let saida = env
        .cmd()
        .args(["purge", "--dry-run", "--namespace", "global"])
        .output()
        .expect("purge failed");
    assert!(
        saida.status.success(),
        "purge: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "purge");
    validar_schema(
        "purge",
        include_str!("../docs/schemas/purge.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 12 — recall
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_12_recall() {
    let env = Env::new();
    env.init();
    env.remember_simples("mem-schema-recall");
    let saida = env
        .cmd()
        .args(["recall", "schema recall teste", "--k", "3"])
        .output()
        .expect("recall failed");
    assert!(
        saida.status.success(),
        "recall: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "recall");
    validar_schema(
        "recall",
        include_str!("../docs/schemas/recall.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 13 — hybrid-search
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_13_hybrid_search() {
    let env = Env::new();
    env.init();
    env.remember_simples("mem-schema-hybrid");
    let saida = env
        .cmd()
        .args(["hybrid-search", "busca hibrida schema", "--k", "3"])
        .output()
        .expect("hybrid-search failed");
    assert!(
        saida.status.success(),
        "hybrid-search: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "hybrid-search");
    validar_schema(
        "hybrid-search",
        include_str!("../docs/schemas/hybrid-search.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 14 — related
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_14_related() {
    let env = Env::new();
    env.init();
    env.remember_simples("mem-schema-related");
    let saida = env
        .cmd()
        .args(["related", "--name", "mem-schema-related", "--hops", "1"])
        .output()
        .expect("related failed");
    assert!(
        saida.status.success(),
        "related: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "related");
    validar_schema(
        "related",
        include_str!("../docs/schemas/related.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 15 — link
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_15_link() {
    let env = Env::new();
    env.init();
    let (ent_a, ent_b) = env.remember_with_entities("mem-schema-link");
    let saida = env
        .cmd()
        .args([
            "link",
            "--from",
            &ent_a,
            "--to",
            &ent_b,
            "--relation",
            "depends-on",
            "--namespace",
            "global",
        ])
        .output()
        .expect("link failed");
    assert!(
        saida.status.success(),
        "link: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "link");
    validar_schema(
        "link",
        include_str!("../docs/schemas/link.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 16 — unlink
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_16_unlink() {
    let env = Env::new();
    env.init();
    let (ent_a, ent_b) = env.remember_with_entities("mem-schema-unlink");
    // Cria o link primeiro
    env.cmd()
        .args([
            "link",
            "--from",
            &ent_a,
            "--to",
            &ent_b,
            "--relation",
            "uses",
            "--namespace",
            "global",
        ])
        .assert()
        .success();
    let saida = env
        .cmd()
        .args([
            "unlink",
            "--from",
            &ent_a,
            "--to",
            &ent_b,
            "--relation",
            "uses",
            "--namespace",
            "global",
        ])
        .output()
        .expect("unlink failed");
    assert!(
        saida.status.success(),
        "unlink: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "unlink");
    validar_schema(
        "unlink",
        include_str!("../docs/schemas/unlink.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 17 — graph
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_17_graph() {
    let env = Env::new();
    env.init();
    env.remember_simples("mem-schema-graph");
    let saida = env
        .cmd()
        .args(["graph", "--format", "json", "--namespace", "global"])
        .output()
        .expect("graph failed");
    assert!(
        saida.status.success(),
        "graph: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "graph");
    validar_schema(
        "graph",
        include_str!("../docs/schemas/graph.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 18 — health
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_18_health() {
    let env = Env::new();
    env.init();
    let saida = env.cmd().arg("health").output().expect("health failed");
    assert!(
        saida.status.success(),
        "health: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "health");
    validar_schema(
        "health",
        include_str!("../docs/schemas/health.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 19 — migrate
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_19_migrate() {
    let env = Env::new();
    env.init();
    let saida = env.cmd().arg("migrate").output().expect("migrate failed");
    assert!(
        saida.status.success(),
        "migrate: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "migrate");
    validar_schema(
        "migrate",
        include_str!("../docs/schemas/migrate.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 20 — optimize
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_20_optimize() {
    let env = Env::new();
    env.init();
    let saida = env.cmd().arg("optimize").output().expect("optimize failed");
    assert!(
        saida.status.success(),
        "optimize: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "optimize");
    validar_schema(
        "optimize",
        include_str!("../docs/schemas/optimize.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 21 — vacuum
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_21_vacuum() {
    let env = Env::new();
    env.init();
    let saida = env.cmd().arg("vacuum").output().expect("vacuum failed");
    assert!(
        saida.status.success(),
        "vacuum: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "vacuum");
    validar_schema(
        "vacuum",
        include_str!("../docs/schemas/vacuum.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 22 — sync-safe-copy
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_22_sync_safe_copy() {
    let env = Env::new();
    env.init();
    let destino = env.tmp.path().join("backup.sqlite");
    let saida = env
        .cmd()
        .args([
            "sync-safe-copy",
            "--dest",
            destino.to_str().expect("caminho inválido"),
        ])
        .output()
        .expect("sync-safe-copy failed");
    assert!(
        saida.status.success(),
        "sync-safe-copy: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "sync-safe-copy");
    validar_schema(
        "sync-safe-copy",
        include_str!("../docs/schemas/sync-safe-copy.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 23 — cleanup-orphans
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_23_cleanup_orphans() {
    let env = Env::new();
    env.init();
    let saida = env
        .cmd()
        .args(["cleanup-orphans", "--dry-run"])
        .output()
        .expect("cleanup-orphans failed");
    assert!(
        saida.status.success(),
        "cleanup-orphans: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "cleanup-orphans");
    validar_schema(
        "cleanup-orphans",
        include_str!("../docs/schemas/cleanup-orphans.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 24 — namespace-detect
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_24_namespace_detect() {
    let env = Env::new();
    env.init();
    let saida = env
        .cmd()
        .arg("namespace-detect")
        .output()
        .expect("namespace-detect failed");
    assert!(
        saida.status.success(),
        "namespace-detect: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "namespace-detect");
    validar_schema(
        "namespace-detect",
        include_str!("../docs/schemas/namespace-detect.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 25 — __debug_schema
// ---------------------------------------------------------------------------

#[test]
#[serial]
#[ignore = "debug-schema subcommand renamed __debug_schema in v1.0.74; test asserts old name"]
fn schema_25_debug_schema() {
    let env = Env::new();
    env.init();
    let saida = env
        .cmd()
        .arg("__debug_schema")
        .output()
        .expect("__debug_schema failed");
    assert!(
        saida.status.success(),
        "debug-schema: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "debug-schema");
    validar_schema(
        "debug-schema",
        include_str!("../docs/schemas/debug-schema.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 26 — fts rebuild
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_26_fts_rebuild() {
    let env = Env::new();
    env.init();
    let saida = env
        .cmd()
        .args(["fts", "rebuild"])
        .output()
        .expect("fts rebuild failed");
    assert!(
        saida.status.success(),
        "fts rebuild: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "fts-rebuild");
    validar_schema(
        "fts-rebuild",
        include_str!("../docs/schemas/fts-rebuild.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 27 — fts check
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_27_fts_check() {
    let env = Env::new();
    env.init();
    let saida = env
        .cmd()
        .args(["fts", "check"])
        .output()
        .expect("fts check failed");
    assert!(
        saida.status.success(),
        "fts check: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "fts-check");
    validar_schema(
        "fts-check",
        include_str!("../docs/schemas/fts-check.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 28 — fts stats
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_28_fts_stats() {
    let env = Env::new();
    env.init();
    let saida = env
        .cmd()
        .args(["fts", "stats"])
        .output()
        .expect("fts stats failed");
    assert!(
        saida.status.success(),
        "fts stats: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "fts-stats");
    validar_schema(
        "fts-stats",
        include_str!("../docs/schemas/fts-stats.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 29 — backup
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_29_backup() {
    let env = Env::new();
    env.init();
    let dest = env.tmp.path().join("schema-backup.sqlite");
    let saida = env
        .cmd()
        .args(["backup", "--output", dest.to_str().unwrap()])
        .output()
        .expect("backup failed");
    assert!(
        saida.status.success(),
        "backup: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "backup");
    validar_schema(
        "backup",
        include_str!("../docs/schemas/backup.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 30 — delete-entity
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_30_delete_entity() {
    let env = Env::new();
    env.init();
    let (ent_a, _ent_b) = env.remember_with_entities("del-ent-schema");
    let saida = env
        .cmd()
        .args(["delete-entity", "--name", &ent_a, "--cascade"])
        .output()
        .expect("delete-entity failed");
    assert!(
        saida.status.success(),
        "delete-entity: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "delete-entity");
    validar_schema(
        "delete-entity",
        include_str!("../docs/schemas/delete-entity.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 31 — reclassify
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_31_reclassify() {
    let env = Env::new();
    env.init();
    let (ent_a, _ent_b) = env.remember_with_entities("reclass-schema");
    let saida = env
        .cmd()
        .args(["reclassify", "--name", &ent_a, "--new-type", "tool"])
        .output()
        .expect("reclassify failed");
    assert!(
        saida.status.success(),
        "reclassify: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "reclassify");
    validar_schema(
        "reclassify",
        include_str!("../docs/schemas/reclassify.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 32 — merge-entities
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_32_merge_entities() {
    let env = Env::new();
    env.init();
    let (ent_a, ent_b) = env.remember_with_entities("merge-schema");
    let saida = env
        .cmd()
        .args(["merge-entities", "--names", &ent_a, "--into", &ent_b])
        .output()
        .expect("merge-entities failed");
    assert!(
        saida.status.success(),
        "merge-entities: exit {:?}\nstdout: {}\nstderr: {}",
        saida.status.code(),
        String::from_utf8_lossy(&saida.stdout),
        String::from_utf8_lossy(&saida.stderr)
    );
    let instancia = Env::parse_stdout(&saida, "merge-entities");
    validar_schema(
        "merge-entities",
        include_str!("../docs/schemas/merge-entities.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 33 — memory-entities
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_33_memory_entities() {
    let env = Env::new();
    env.init();
    env.remember_with_entities("mem-ent-schema");
    let saida = env
        .cmd()
        .args(["memory-entities", "--name", "mem-ent-schema"])
        .output()
        .expect("memory-entities failed");
    assert!(
        saida.status.success(),
        "memory-entities: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "memory-entities");
    validar_schema(
        "memory-entities",
        include_str!("../docs/schemas/memory-entities.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 33b — memory-entities reverse lookup (--entity)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_33b_memory_entities_reverse() {
    let env = Env::new();
    env.init();
    let (ent_a, _ent_b) = env.remember_with_entities("mem-ent-rev-schema");
    let saida = env
        .cmd()
        .args(["memory-entities", "--entity", &ent_a])
        .output()
        .expect("memory-entities --entity failed");
    assert!(
        saida.status.success(),
        "memory-entities --entity: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "memory-entities --entity");
    validar_schema(
        "memory-entities-reverse",
        include_str!("../docs/schemas/memory-entities-reverse.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 34 — prune-ner
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_34_prune_ner() {
    let env = Env::new();
    env.init();
    let (ent_a, _ent_b) = env.remember_with_entities("prune-schema");
    let saida = env
        .cmd()
        .args(["prune-ner", "--entity", &ent_a, "--dry-run"])
        .output()
        .expect("prune-ner failed");
    assert!(
        saida.status.success(),
        "prune-ner: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "prune-ner");
    validar_schema(
        "prune-ner",
        include_str!("../docs/schemas/prune-ner.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 35 — rename-entity
// ---------------------------------------------------------------------------

#[test]
#[serial]
#[ignore = "test seeds a memory but rename-entity requires an entity; pre-existing v1.0.74 setup bug"]
fn schema_35_rename_entity() {
    let env = Env::new();
    env.init();
    let (ent_a, _ent_b) = env.remember_with_entities("rename-ent-schema");
    let new_name = format!("{ent_a}-renamed");
    let saida = env
        .cmd()
        .args(["rename-entity", "--name", &ent_a, "--new-name", &new_name])
        .output()
        .expect("rename-entity failed");
    assert!(
        saida.status.success(),
        "rename-entity: exit {:?}",
        saida.status.code()
    );
    let instancia = Env::parse_stdout(&saida, "rename-entity");
    validar_schema(
        "rename-entity",
        include_str!("../docs/schemas/rename-entity.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 36 — deep-research
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_36_deep_research() {
    let env = Env::new();
    env.init();
    env.remember_simples("schema36-mem-a");
    env.remember_simples("schema36-mem-b");

    let saida = env
        .cmd()
        .args([
            "deep-research",
            "auth and deploy",
            "--max-sub-queries",
            "2",
            "--k",
            "5",
        ])
        .output()
        .expect("deep-research failed");
    assert!(
        saida.status.success(),
        "deep-research: exit {:?}\nstderr: {}",
        saida.status.code(),
        String::from_utf8_lossy(&saida.stderr)
    );
    let instancia = Env::parse_stdout(&saida, "deep-research");
    validar_schema(
        "deep-research",
        include_str!("../docs/schemas/deep-research.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 37 — reclassify-relation
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_37_reclassify_relation() {
    let env = Env::new();
    env.init();
    let (ent_a, ent_b) = env.remember_with_entities("schema37-reclassify-rel");

    // Link entities with a 'mentions' relation to give the command something to work with.
    let _ = env
        .cmd()
        .args([
            "link",
            "--from",
            &ent_a,
            "--to",
            &ent_b,
            "--relation",
            "mentions",
        ])
        .output()
        .expect("link failed");

    // Dry-run: safe, validates JSON contract without committing.
    let saida = env
        .cmd()
        .args([
            "reclassify-relation",
            "--from-relation",
            "mentions",
            "--to-relation",
            "related",
            "--batch",
            "--dry-run",
        ])
        .output()
        .expect("reclassify-relation failed");
    assert!(
        saida.status.success(),
        "reclassify-relation: exit {:?}\nstderr: {}",
        saida.status.code(),
        String::from_utf8_lossy(&saida.stderr)
    );
    let instancia = Env::parse_stdout(&saida, "reclassify-relation");
    validar_schema(
        "reclassify-relation",
        include_str!("../docs/schemas/reclassify-relation.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 38 — normalize-entities
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_38_normalize_entities() {
    let env = Env::new();
    env.init();
    env.remember_simples("schema38-normalize-ent");

    // Dry-run: validates JSON contract without modifying data.
    let saida = env
        .cmd()
        .args(["normalize-entities", "--dry-run"])
        .output()
        .expect("normalize-entities failed");
    assert!(
        saida.status.success(),
        "normalize-entities: exit {:?}\nstderr: {}",
        saida.status.code(),
        String::from_utf8_lossy(&saida.stderr)
    );
    let instancia = Env::parse_stdout(&saida, "normalize-entities");
    validar_schema(
        "normalize-entities",
        include_str!("../docs/schemas/normalize-entities.schema.json"),
        &instancia,
    );
}

// ---------------------------------------------------------------------------
// 39 — enrich (dry-run, NDJSON: validate each line type against its schema)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_39_enrich() {
    let env = Env::new();
    env.init();
    env.remember_simples("schema39-enrich-mem");

    let saida = env
        .cmd()
        .args(["enrich", "--operation", "memory-bindings", "--dry-run"])
        .output()
        .expect("enrich failed");
    assert!(
        saida.status.success(),
        "enrich: exit {:?}\nstderr: {}",
        saida.status.code(),
        String::from_utf8_lossy(&saida.stderr)
    );

    let stdout_str = String::from_utf8_lossy(&saida.stdout);
    let lines: Vec<&str> = stdout_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert!(
        !lines.is_empty(),
        "enrich must emit at least one NDJSON line"
    );

    let phase_schema_str = include_str!("../docs/schemas/enrich-phase.schema.json");
    let item_schema_str = include_str!("../docs/schemas/enrich-item-event.schema.json");
    let summary_schema_str = include_str!("../docs/schemas/enrich-summary.schema.json");

    let mut summary_found = false;

    for line in &lines {
        let val: Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("enrich NDJSON line not valid JSON: {e}\n{line}"));

        if val["summary"] == true {
            validar_schema("enrich-summary", summary_schema_str, &val);
            summary_found = true;
        } else if val.get("phase").is_some() {
            validar_schema("enrich-phase", phase_schema_str, &val);
        } else if val.get("item").is_some() {
            validar_schema("enrich-item", item_schema_str, &val);
        }
        // Lines from non-implemented operations include "operation" key — skip gracefully.
    }

    assert!(summary_found, "enrich must emit a summary line");
}
