#![cfg(feature = "slow-tests")]

// Suite 8 — Validação estrita de contrato JSON Schema para todos os 25 subcomandos.
// Cada teste executa o binário, captura stdout, parseia como JSON e valida contra
// docs/schemas/<cmd>.schema.json usando o crate jsonschema::Validator.
//
// Dependência: jsonschema = "0.29" em [dev-dependencies] do Cargo.toml.
use assert_cmd::Command;
use serde_json::Value;
use serial_test::serial;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Infraestrutura de teste
// ---------------------------------------------------------------------------

struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        let tmp = TempDir::new().expect("TempDir::new falhou");
        Self { tmp }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("sqlite-graphrag").expect("binário não encontrado");
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
            .expect("remember falhou ao executar");
        assert!(
            saida.status.success(),
            "remember retornou erro: {:?}\nstdout: {}",
            saida.status.code(),
            String::from_utf8_lossy(&saida.stdout)
        );
        serde_json::from_slice(&saida.stdout).expect("remember stdout não é JSON válido")
    }

    fn remember_com_entidades(&self, nome: &str) -> (String, String) {
        let ent_a = format!("Ent{}Alpha", nome.replace('-', ""));
        let ent_b = format!("Ent{}Beta", nome.replace('-', ""));
        let caminho_ents = self.tmp.path().join(format!("{nome}_ents.json"));
        let json_ents = format!(
            r#"[{{"name":"{ent_a}","entity_type":"concept"}},{{"name":"{ent_b}","entity_type":"concept"}}]"#
        );
        std::fs::write(&caminho_ents, &json_ents).expect("escrita de entidades falhou");
        let saida = self
            .cmd()
            .args([
                "remember",
                "--name",
                nome,
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
            .expect("remember com entidades falhou");
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
/// Coleta todos os erros e aborta com mensagem detalhada se houver violações.
fn validar_schema(cmd: &str, schema_str: &str, instancia: &Value) {
    let schema: Value =
        serde_json::from_str(schema_str).unwrap_or_else(|e| panic!("[{cmd}] schema inválido: {e}"));
    let validador = jsonschema::Validator::new(&schema)
        .unwrap_or_else(|e| panic!("[{cmd}] falha ao compilar schema: {e}"));
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
    let saida = env.cmd().arg("init").output().expect("init falhou");
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
    let saida = env.cmd().arg("stats").output().expect("stats falhou");
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
        .expect("list falhou");
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
        .expect("read falhou");
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
        .expect("edit falhou");
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
        .expect("rename falhou");
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
        .expect("history falhou");
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
        .expect("forget falhou");
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
    // Cria uma segunda versão via edit
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
        .expect("restore falhou");
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
        .expect("purge falhou");
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
        .expect("recall falhou");
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
        .expect("hybrid-search falhou");
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
        .expect("related falhou");
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
    let (ent_a, ent_b) = env.remember_com_entidades("mem-schema-link");
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
        .expect("link falhou");
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
    let (ent_a, ent_b) = env.remember_com_entidades("mem-schema-unlink");
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
        .expect("unlink falhou");
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
        .expect("graph falhou");
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
    let saida = env.cmd().arg("health").output().expect("health falhou");
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
    let saida = env.cmd().arg("migrate").output().expect("migrate falhou");
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
    let saida = env.cmd().arg("optimize").output().expect("optimize falhou");
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
    let saida = env.cmd().arg("vacuum").output().expect("vacuum falhou");
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
        .expect("sync-safe-copy falhou");
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
        .expect("cleanup-orphans falhou");
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
        .expect("namespace-detect falhou");
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
fn schema_25_debug_schema() {
    let env = Env::new();
    env.init();
    let saida = env
        .cmd()
        .arg("__debug_schema")
        .output()
        .expect("__debug_schema falhou");
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
