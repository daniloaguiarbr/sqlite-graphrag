/// Suite 10 — Smoke tests contra ~/.cargo/bin/neurographrag (v2.0.4 instalado)
///
/// Testa o happy path de cada um dos 25 subcomandos contra o binário já
/// instalado. Skippa gracefully se:
/// - Binário ausente em `~/.cargo/bin/neurographrag`
/// - Variável `NEUROGRAPHRAG_SKIP_INSTALLED_BINARY_SMOKE=1` definida
///
/// Cada teste usa TempDir + NEUROGRAPHRAG_DB_PATH isolado.
/// Todos os testes devem retornar exit code 0 e JSON válido no stdout.
///
/// Contratos de API verificados contra v2.0.4:
/// - `init`     → {status: "ok", db_path, schema_version, ...}
/// - `remember` → {memory_id, name, action: "created", ...}   (sem `status`)
/// - `forget`   → {forgotten: true, name, namespace}          (sem `status`)
/// - `rename`   → {memory_id, name, version}                  (sem `status`)
/// - `edit`     → {memory_id, name, action: "updated", ...}   (sem `status`)
/// - `list`     → array JSON direto na raiz                   (não {memories:[]})
/// - `link`     → {action: "created", from, to, relation, ...}
/// - `unlink`   → {action: "deleted", relationship_id, ...}
/// - `__debug_schema` não existe em v2.0.4 — smoke_25 skippa gracefully
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn installed_bin() -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let p = PathBuf::from(home).join(".cargo/bin/neurographrag");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn skip_if_not_installed() -> PathBuf {
    if std::env::var("NEUROGRAPHRAG_SKIP_INSTALLED_BINARY_SMOKE").as_deref() == Ok("1") {
        eprintln!("Suite 10: skipped via NEUROGRAPHRAG_SKIP_INSTALLED_BINARY_SMOKE=1");
        std::process::exit(0);
    }
    match installed_bin() {
        Some(p) => p,
        None => {
            eprintln!("Suite 10: neurographrag não encontrado em ~/.cargo/bin — skipping");
            std::process::exit(0);
        }
    }
}

/// Retorna a versão do binário instalado como string, ex: "2.0.4"
fn installed_version(bin: &PathBuf) -> String {
    let out = Command::new(bin)
        .arg("--version")
        .output()
        .expect("--version falhou");
    let s = String::from_utf8_lossy(&out.stdout);
    // formato: "neurographrag 2.0.4\n"
    s.split_whitespace().nth(1).unwrap_or("0.0.0").to_string()
}

struct Env {
    bin: PathBuf,
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        let bin = skip_if_not_installed();
        let tmp = TempDir::new().expect("TempDir falhou");
        Self { bin, tmp }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(&self.bin);
        c.env(
            "NEUROGRAPHRAG_DB_PATH",
            self.tmp.path().join("smoke.sqlite"),
        );
        c.env("NEUROGRAPHRAG_CACHE_DIR", self.tmp.path().join("cache"));
        c.env("NEUROGRAPHRAG_LOG_LEVEL", "error");
        c.arg("--skip-memory-guard");
        c
    }

    fn init(&self) {
        let out = self.cmd().arg("init").output().expect("init falhou");
        assert!(out.status.success(), "init falhou: {}", stderr(&out));
    }

    fn remember(&self, name: &str, body: &str) {
        let out = self
            .cmd()
            .args([
                "remember",
                "--name",
                name,
                "--type",
                "project",
                "--description",
                "smoke test",
                "--body",
                body,
            ])
            .output()
            .expect("remember falhou");
        assert!(
            out.status.success(),
            "remember {name} falhou: {}",
            stderr(&out)
        );
    }

    /// Cria uma memória com duas entidades no grafo e retorna os nomes das entidades.
    /// entities-file requer campo `entity_type` (não `kind`).
    fn remember_with_entities(&self, name: &str, body: &str) -> (String, String) {
        let ent_a = format!("Ent{name}A");
        let ent_b = format!("Ent{name}B");
        let ents_path = self.tmp.path().join(format!("{name}_ents.json"));
        let ents_json = format!(
            r#"[{{"name":"{ent_a}","entity_type":"concept"}},{{"name":"{ent_b}","entity_type":"concept"}}]"#
        );
        std::fs::write(&ents_path, ents_json).expect("escrita entities-file falhou");

        let out = self
            .cmd()
            .args([
                "remember",
                "--name",
                name,
                "--type",
                "project",
                "--description",
                "smoke test com entidades",
                "--body",
                body,
                "--entities-file",
                ents_path.to_str().unwrap(),
            ])
            .output()
            .expect("remember com entities falhou");
        assert!(
            out.status.success(),
            "remember {name} com entities falhou: {}",
            stderr(&out)
        );
        (ent_a, ent_b)
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn assert_json_stdout(out: &Output) {
    assert!(
        out.status.success(),
        "exit code {:?}: {}",
        out.status.code(),
        stderr(out)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(stdout.trim());
    assert!(parsed.is_ok(), "stdout não é JSON válido: {stdout}");
}

/// Aceitável para comandos que podem retornar 0 ou 4 (not found)
fn assert_json_or_not_found(out: &Output) {
    let code = out.status.code().unwrap_or(1);
    assert!(
        code == 0 || code == 4,
        "exit code inesperado {code}: {}",
        stderr(out)
    );
    if code == 0 {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(stdout.trim());
        assert!(parsed.is_ok(), "stdout não é JSON válido: {stdout}");
    }
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #01: init
// ---------------------------------------------------------------------------

#[test]
fn smoke_01_init() {
    let env = Env::new();
    let out = env.cmd().arg("init").output().expect("init falhou");
    assert_json_stdout(&out);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["status"], "ok", "init deve retornar status=ok: {json}");
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #02: health
// ---------------------------------------------------------------------------

#[test]
fn smoke_02_health() {
    let env = Env::new();
    env.init();
    let out = env.cmd().arg("health").output().expect("health falhou");
    assert_json_stdout(&out);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["status"], "ok",
        "health deve retornar status=ok: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #03: remember
// ---------------------------------------------------------------------------

#[test]
fn smoke_03_remember() {
    let env = Env::new();
    env.init();
    let out = env
        .cmd()
        .args([
            "remember",
            "--name",
            "smoke-memoria-01",
            "--type",
            "user",
            "--description",
            "Memória de smoke test",
            "--body",
            "Conteúdo da memória de smoke test para validar o subcomando remember.",
        ])
        .output()
        .expect("remember falhou");
    assert_json_stdout(&out);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // v2.0.4: remember retorna action "created", não campo status
    assert_eq!(
        json["action"], "created",
        "remember deve retornar action=created: {json}"
    );
    assert!(
        json["memory_id"].as_i64().is_some(),
        "memory_id deve ser inteiro: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #04: recall
// ---------------------------------------------------------------------------

#[test]
fn smoke_04_recall() {
    let env = Env::new();
    env.init();
    env.remember("smoke-recall-01", "memória para busca semântica de recall");
    let out = env
        .cmd()
        .args(["recall", "busca semântica", "-k", "5"])
        .output()
        .expect("recall falhou");
    assert_json_or_not_found(&out);
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #05: read
// ---------------------------------------------------------------------------

#[test]
fn smoke_05_read() {
    let env = Env::new();
    env.init();
    env.remember("smoke-read-01", "conteúdo para read");
    let out = env
        .cmd()
        .args(["read", "--name", "smoke-read-01"])
        .output()
        .expect("read falhou");
    assert_json_stdout(&out);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["name"], "smoke-read-01",
        "read deve retornar a memória pelo nome: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #06: list
// ---------------------------------------------------------------------------

#[test]
fn smoke_06_list() {
    let env = Env::new();
    env.init();
    env.remember("smoke-list-01", "memória para listar");
    let out = env
        .cmd()
        .args(["list", "--limit", "10"])
        .output()
        .expect("list falhou");
    assert_json_stdout(&out);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = json["items"]
        .as_array()
        .expect("list deve retornar objeto com campo 'items'");
    assert!(
        !arr.is_empty(),
        "list deve retornar pelo menos uma memória: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #07: forget
// ---------------------------------------------------------------------------

#[test]
fn smoke_07_forget() {
    let env = Env::new();
    env.init();
    env.remember("smoke-forget-01", "memória para deletar");
    let out = env
        .cmd()
        .args(["forget", "--name", "smoke-forget-01"])
        .output()
        .expect("forget falhou");
    assert_json_stdout(&out);
    // v2.0.4: forget retorna {forgotten: true, name, namespace} — sem campo status
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["forgotten"], true,
        "forget deve retornar forgotten=true: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #08: purge
// ---------------------------------------------------------------------------

#[test]
fn smoke_08_purge() {
    let env = Env::new();
    env.init();
    env.remember("smoke-purge-01", "memória para purgar");
    // Soft-delete primeiro
    env.cmd()
        .args(["forget", "--name", "smoke-purge-01"])
        .output()
        .unwrap();
    let out = env
        .cmd()
        .args(["purge", "--yes"])
        .output()
        .expect("purge falhou");
    assert_json_stdout(&out);
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #09: rename
// ---------------------------------------------------------------------------

#[test]
fn smoke_09_rename() {
    let env = Env::new();
    env.init();
    env.remember("smoke-rename-src", "memória para renomear");
    // v2.0.4: rename usa --name e --new-name (não --from/--to)
    let out = env
        .cmd()
        .args([
            "rename",
            "--name",
            "smoke-rename-src",
            "--new-name",
            "smoke-rename-dst",
        ])
        .output()
        .expect("rename falhou");
    assert_json_stdout(&out);
    // v2.0.4: rename retorna {memory_id, name, version} — sem campo status
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["name"], "smoke-rename-dst",
        "rename deve retornar o novo nome: {json}"
    );
    assert!(
        json["memory_id"].as_i64().is_some(),
        "rename deve retornar memory_id: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #10: edit
// ---------------------------------------------------------------------------

#[test]
fn smoke_10_edit() {
    let env = Env::new();
    env.init();
    env.remember("smoke-edit-01", "conteúdo original");
    let out = env
        .cmd()
        .args([
            "edit",
            "--name",
            "smoke-edit-01",
            "--body",
            "conteúdo editado pelo smoke test",
        ])
        .output()
        .expect("edit falhou");
    assert_json_stdout(&out);
    // v2.0.4: edit retorna {memory_id, name, action: "updated", version} — sem campo status
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["action"], "updated",
        "edit deve retornar action=updated: {json}"
    );
    assert!(
        json["memory_id"].as_i64().is_some(),
        "edit deve retornar memory_id: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #11: history
// ---------------------------------------------------------------------------

#[test]
fn smoke_11_history() {
    let env = Env::new();
    env.init();
    env.remember("smoke-history-01", "versão 1 do conteúdo");
    // Gera uma segunda versão
    env.cmd()
        .args([
            "edit",
            "--name",
            "smoke-history-01",
            "--body",
            "versão 2 do conteúdo",
        ])
        .output()
        .unwrap();
    let out = env
        .cmd()
        .args(["history", "--name", "smoke-history-01"])
        .output()
        .expect("history falhou");
    assert_json_stdout(&out);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["versions"].is_array(),
        "history deve retornar array versions: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #12: restore
// ---------------------------------------------------------------------------

#[test]
fn smoke_12_restore() {
    let env = Env::new();
    env.init();
    env.remember("smoke-restore-01", "versão 1");
    env.cmd()
        .args(["edit", "--name", "smoke-restore-01", "--body", "versão 2"])
        .output()
        .unwrap();
    // Obtém versões pelo history
    let hist_out = env
        .cmd()
        .args(["history", "--name", "smoke-restore-01"])
        .output()
        .unwrap();
    let hist_json: serde_json::Value = serde_json::from_slice(&hist_out.stdout).unwrap();
    let versions = hist_json["versions"].as_array().unwrap();
    // Restaura para a versão mais antiga disponível
    // v2.0.4: campo é "version" (não "version_id")
    if versions.len() >= 2 {
        let version_id = versions
            .iter()
            .map(|v| v["version"].as_i64().unwrap_or(0))
            .min()
            .unwrap_or(1);
        let out = env
            .cmd()
            .args([
                "restore",
                "--name",
                "smoke-restore-01",
                "--version",
                &version_id.to_string(),
            ])
            .output()
            .expect("restore falhou");
        assert_json_stdout(&out);
    }
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #13: hybrid-search
// ---------------------------------------------------------------------------

#[test]
fn smoke_13_hybrid_search() {
    let env = Env::new();
    env.init();
    env.remember(
        "smoke-hybrid-01",
        "conteúdo para busca híbrida com FTS e vetorial",
    );
    let out = env
        .cmd()
        .args(["hybrid-search", "busca híbrida", "-k", "5"])
        .output()
        .expect("hybrid-search falhou");
    assert_json_or_not_found(&out);
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #14: stats
// ---------------------------------------------------------------------------

#[test]
fn smoke_14_stats() {
    let env = Env::new();
    env.init();
    let out = env.cmd().arg("stats").output().expect("stats falhou");
    assert_json_stdout(&out);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["memories"].as_i64().is_some(),
        "stats deve ter campo memories como inteiro: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #15: migrate
// ---------------------------------------------------------------------------

#[test]
fn smoke_15_migrate() {
    let env = Env::new();
    env.init();
    let out = env.cmd().arg("migrate").output().expect("migrate falhou");
    assert_json_stdout(&out);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["status"], "ok",
        "migrate deve retornar status=ok: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #16: namespace-detect
// ---------------------------------------------------------------------------

#[test]
fn smoke_16_namespace_detect() {
    let env = Env::new();
    env.init();
    let out = env
        .cmd()
        .arg("namespace-detect")
        .output()
        .expect("namespace-detect falhou");
    assert_json_stdout(&out);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["namespace"].is_string(),
        "namespace-detect deve retornar campo namespace: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #17: optimize
// ---------------------------------------------------------------------------

#[test]
fn smoke_17_optimize() {
    let env = Env::new();
    env.init();
    let out = env.cmd().arg("optimize").output().expect("optimize falhou");
    assert_json_stdout(&out);
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #18: sync-safe-copy
// ---------------------------------------------------------------------------

#[test]
fn smoke_18_sync_safe_copy() {
    let env = Env::new();
    env.init();
    let dest = env.tmp.path().join("snapshot.sqlite");
    let out = env
        .cmd()
        .args(["sync-safe-copy", "--dest", dest.to_str().unwrap()])
        .output()
        .expect("sync-safe-copy falhou");
    assert_json_stdout(&out);
    assert!(dest.exists(), "snapshot deve ter sido criado em {dest:?}");
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #19: vacuum
// ---------------------------------------------------------------------------

#[test]
fn smoke_19_vacuum() {
    let env = Env::new();
    env.init();
    let out = env.cmd().arg("vacuum").output().expect("vacuum falhou");
    assert_json_stdout(&out);
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #20: link
// ---------------------------------------------------------------------------

#[test]
fn smoke_20_link() {
    let env = Env::new();
    env.init();
    // Link opera sobre entidades do grafo, não sobre nomes de memórias.
    // Cria memória com entidades via --entities-file (campo entity_type obrigatório).
    let (ent_a, ent_b) = env.remember_with_entities(
        "smoke-link",
        "memória com entidades para smoke test de link",
    );
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
        .expect("link falhou");
    assert_json_stdout(&out);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["action"], "created",
        "link deve retornar action=created: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #21: unlink
// ---------------------------------------------------------------------------

#[test]
fn smoke_21_unlink() {
    let env = Env::new();
    env.init();
    // Cria entidades, linka, depois desfaz
    let (ent_a, ent_b) = env.remember_with_entities(
        "smoke-unlink",
        "memória com entidades para smoke test de unlink",
    );
    // Linka primeiro
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
        .output()
        .unwrap();
    // Desfaz o link
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
        .expect("unlink falhou");
    assert_json_stdout(&out);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["action"], "deleted",
        "unlink deve retornar action=deleted: {json}"
    );
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #22: related
// ---------------------------------------------------------------------------

#[test]
fn smoke_22_related() {
    let env = Env::new();
    env.init();
    env.remember("smoke-related-01", "conteúdo para busca de relacionados");
    let out = env
        .cmd()
        .args(["related", "smoke-related-01"])
        .output()
        .expect("related falhou");
    // Aceita 0 (encontrou relacionados) ou 4 (sem relacionados)
    assert_json_or_not_found(&out);
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #23: graph
// ---------------------------------------------------------------------------

#[test]
fn smoke_23_graph() {
    let env = Env::new();
    env.init();
    let out = env
        .cmd()
        .args(["graph", "--format", "json"])
        .output()
        .expect("graph falhou");
    assert_json_stdout(&out);
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #24: cleanup-orphans
// ---------------------------------------------------------------------------

#[test]
fn smoke_24_cleanup_orphans() {
    let env = Env::new();
    env.init();
    let out = env
        .cmd()
        .arg("cleanup-orphans")
        .output()
        .expect("cleanup-orphans falhou");
    assert_json_stdout(&out);
}

// ---------------------------------------------------------------------------
// Suite 10 — Smoke #25: __debug_schema
//
// __debug_schema foi adicionado em v2.0.5 (task #6).
// Se o binário instalado for v2.0.4 ou anterior, este teste skippa
// gracefully sem falhar.
// ---------------------------------------------------------------------------

#[test]
fn smoke_25_debug_schema() {
    let env = Env::new();
    env.init();

    let version = installed_version(&env.bin);
    // Parseia major.minor.patch para comparação semântica
    let parts: Vec<u64> = version.split('.').map(|s| s.parse().unwrap_or(0)).collect();
    let semver = (
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    );

    // __debug_schema só existe a partir de v2.0.5
    if semver < (2, 0, 5) {
        eprintln!(
            "Suite 10 smoke_25: binário instalado é v{version} (< 2.0.5) — __debug_schema inexistente, skip graceful"
        );
        return;
    }

    let out = env
        .cmd()
        .arg("__debug_schema")
        .output()
        .expect("__debug_schema falhou");
    assert_json_stdout(&out);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["objects"].is_array() || json["migrations"].is_array(),
        "__debug_schema deve retornar informações de schema: {json}"
    );
}
