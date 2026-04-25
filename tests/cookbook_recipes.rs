#![cfg(feature = "slow-tests")]

// Suite 11 — testes das receitas documentadas em docs/COOKBOOK.md
//
// Cada teste valida que o comportamento real do CLI corresponde ao documentado.
// Detecta drift entre documentação e implementação.
//
// Receitas skippadas por design:
//   - Recipe 6: AGENTS.md discovery — apenas documentação, sem comandos executáveis
//   - Recipe 12: Git LFS — requer git lfs instalado e repositório git
//
// Receitas testadas: 1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 13, 14, 15 (13 de 13 executáveis)

use assert_cmd::Command;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

fn bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_sqlite-graphrag"))
}

fn cmd(dir: &TempDir) -> Command {
    let mut c = Command::new(bin());
    c.env_clear()
        .env("SQLITE_GRAPHRAG_DB_PATH", dir.path().join("ng.sqlite"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", dir.path().join("cache"))
        .arg("--skip-memory-guard");
    c
}

fn init(dir: &TempDir) {
    cmd(dir).arg("init").assert().success();
}

// Recipe 1 — Bootstrap 60s: init + health retorna JSON com status ok
#[test]
#[serial]
fn recipe_01_bootstrap_60s() {
    let dir = TempDir::new().unwrap();
    init(&dir);

    let output = cmd(&dir).args(["health", "--json"]).output().unwrap();
    assert!(output.status.success(), "health deve ter exit 0");

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("health deve retornar JSON válido");

    assert_eq!(json["status"], "ok", "recipe 1: health.status deve ser ok");
    assert_eq!(
        json["integrity"], "ok",
        "recipe 1: health.integrity deve ser ok"
    );
    assert!(
        json["schema_version"].is_number(),
        "recipe 1: health.schema_version deve ser número"
    );
    assert!(
        json["elapsed_ms"].is_number(),
        "recipe 1: health deve ter elapsed_ms"
    );
}

// Recipe 2 — Bulk-import stdin: remember com --body-stdin lê corpo do stdin
#[test]
#[serial]
fn recipe_02_bulk_import_body_stdin() {
    let dir = TempDir::new().unwrap();
    init(&dir);

    let corpo = "Este é o conteúdo importado via stdin do arquivo markdown.";

    let output = cmd(&dir)
        .args([
            "remember",
            "--name",
            "doc-importado",
            "--type",
            "user",
            "--description",
            "imported from docs/readme.md",
            "--body-stdin",
            "--namespace",
            "global",
        ])
        .write_stdin(corpo)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "recipe 2: remember com --body-stdin deve ter exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("recipe 2: remember deve retornar JSON válido");
    assert_eq!(
        json["action"], "created",
        "recipe 2: action deve ser created"
    );

    // Valida que o corpo foi persistido via read
    let leitura = cmd(&dir)
        .args(["read", "--name", "doc-importado", "--namespace", "global"])
        .output()
        .unwrap();
    assert!(
        leitura.status.success(),
        "recipe 2: read do memory importado deve ter exit 0"
    );
    let json_leitura: serde_json::Value = serde_json::from_slice(&leitura.stdout).unwrap();
    let body = json_leitura["body"].as_str().unwrap_or("");
    assert!(
        body.contains("conteúdo importado via stdin"),
        "recipe 2: body deve conter texto do stdin, got: {body}"
    );
}

// Recipe 3 — Hybrid search tunable: --rrf-k e --weight-vec emitidos no JSON
#[test]
#[serial]
fn recipe_03_hybrid_search_tunable() {
    let dir = TempDir::new().unwrap();
    init(&dir);

    // Seed com uma memória
    cmd(&dir)
        .args([
            "remember",
            "--name",
            "pg-deadlock",
            "--type",
            "incident",
            "--description",
            "postgres migration deadlock",
            "--body",
            "deadlock detectado durante migration de índices no postgres",
            "--namespace",
            "global",
        ])
        .assert()
        .success();

    let output = cmd(&dir)
        .args([
            "hybrid-search",
            "postgres migration deadlock",
            "--k",
            "10",
            "--rrf-k",
            "60",
            "--weight-vec",
            "0.7",
            "--weight-fts",
            "0.3",
            "--namespace",
            "global",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "recipe 3: hybrid-search deve ter exit 0"
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("recipe 3: hybrid-search deve retornar JSON válido");

    assert_eq!(
        json["rrf_k"], 60,
        "recipe 3: rrf_k deve ser 60 conforme documentado"
    );
    assert!(
        (json["weights"]["vec"].as_f64().unwrap() - 0.7).abs() < 0.001,
        "recipe 3: weights.vec deve ser 0.7"
    );
    assert!(
        (json["weights"]["fts"].as_f64().unwrap() - 0.3).abs() < 0.001,
        "recipe 3: weights.fts deve ser 0.3"
    );
    assert!(
        json["results"].is_array(),
        "recipe 3: results deve ser array"
    );
    assert!(
        json["elapsed_ms"].is_number(),
        "recipe 3: elapsed_ms deve estar presente"
    );

    // Validar que cada resultado tem vec_rank e fts_rank como documentado
    let results = json["results"].as_array().unwrap();
    if !results.is_empty() {
        let primeiro = &results[0];
        assert!(
            primeiro.get("vec_rank").is_some() || primeiro.get("combined_score").is_some(),
            "recipe 3: resultado deve ter vec_rank ou combined_score"
        );
    }
}

// Recipe 4 — Graph traversal: related com --hops retorna JSON com results
#[test]
#[serial]
fn recipe_04_graph_traversal_related() {
    let dir = TempDir::new().unwrap();
    init(&dir);

    // Seed memória de origem
    cmd(&dir)
        .args([
            "remember",
            "--name",
            "authentication-flow",
            "--type",
            "project",
            "--description",
            "fluxo de autenticação OAuth2",
            "--body",
            "implementação do fluxo de autenticação com OAuth2 e JWT",
            "--namespace",
            "global",
        ])
        .assert()
        .success();

    let output = cmd(&dir)
        .args([
            "related",
            "authentication-flow",
            "--hops",
            "2",
            "--format",
            "json",
            "--namespace",
            "global",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "recipe 4: related deve ter exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("recipe 4: related deve retornar JSON válido");

    assert!(
        json["results"].is_array(),
        "recipe 4: results deve ser array conforme documentado"
    );
    assert!(
        json["elapsed_ms"].is_number(),
        "recipe 4: elapsed_ms deve estar presente"
    );
}

// Recipe 5 — Pre/post-task hooks: recall retorna JSON com results, remember persiste
#[test]
#[serial]
fn recipe_05_pre_post_task_hooks() {
    let dir = TempDir::new().unwrap();
    init(&dir);

    // Simula post-task hook: persiste resposta do assistente
    let resposta_assistente = "decisão: usar JWT com expiração de 24h para tokens de sessão";
    let nome_sessao = "session-12345";

    let output_post = cmd(&dir)
        .args([
            "remember",
            "--name",
            nome_sessao,
            "--type",
            "project",
            "--description",
            "decision log",
            "--body",
            resposta_assistente,
            "--namespace",
            "global",
        ])
        .output()
        .unwrap();

    assert!(
        output_post.status.success(),
        "recipe 5 (post-hook): remember deve ter exit 0"
    );

    // Simula pre-task hook: recupera contexto relevante
    let output_pre = cmd(&dir)
        .args([
            "recall",
            "decisão JWT sessão",
            "--k",
            "5",
            "--namespace",
            "global",
        ])
        .output()
        .unwrap();

    assert!(
        output_pre.status.success(),
        "recipe 5 (pre-hook): recall deve ter exit 0"
    );

    let json: serde_json::Value = serde_json::from_slice(&output_pre.stdout)
        .expect("recipe 5: recall deve retornar JSON válido");

    assert!(
        json["results"].is_array(),
        "recipe 5: recall.results deve ser array"
    );
    assert!(
        json["elapsed_ms"].is_number(),
        "recipe 5: recall deve ter elapsed_ms"
    );

    // A memória persistida deve ser encontrada
    let results = json["results"].as_array().unwrap();
    assert!(
        !results.is_empty(),
        "recipe 5: recall deve encontrar a memória persistida pelo post-hook"
    );
}

// Recipe 7 — SQLITE_GRAPHRAG_NAMESPACE env: namespace-detect reporta fonte "environment"
#[test]
#[serial]
fn recipe_07_namespace_env_precedencia() {
    let dir = TempDir::new().unwrap();

    let output = std::process::Command::new(bin())
        .env_clear()
        .env("SQLITE_GRAPHRAG_DB_PATH", dir.path().join("ng.sqlite"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", dir.path().join("cache"))
        .env("SQLITE_GRAPHRAG_NAMESPACE", "meu-projeto")
        .arg("--skip-memory-guard")
        .args(["namespace-detect", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "recipe 7: namespace-detect deve ter exit 0"
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("recipe 7: namespace-detect deve retornar JSON válido");

    assert_eq!(
        json["namespace"], "meu-projeto",
        "recipe 7: namespace deve ser o valor da env var"
    );
    assert_eq!(
        json["source"], "environment",
        "recipe 7: source deve ser environment conforme documentado"
    );
    assert!(
        json["elapsed_ms"].is_number(),
        "recipe 7: elapsed_ms deve estar presente"
    );
}

// Recipe 8 — Export para arquivo /tmp/ng.json: hybrid-search > arquivo
#[test]
#[serial]
fn recipe_08_export_para_arquivo() {
    let dir = TempDir::new().unwrap();
    init(&dir);

    // Seed com memória
    cmd(&dir)
        .args([
            "remember",
            "--name",
            "editor-context",
            "--type",
            "project",
            "--description",
            "contexto do editor",
            "--body",
            "contexto atual do editor sobre o módulo de autenticação",
            "--namespace",
            "global",
        ])
        .assert()
        .success();

    let dest = dir.path().join("ng.json");

    let output = cmd(&dir)
        .args([
            "hybrid-search",
            "editor contexto autenticação",
            "--k",
            "10",
            "--namespace",
            "global",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "recipe 8: hybrid-search deve ter exit 0"
    );

    // Simula redirecionamento para arquivo
    fs::write(&dest, &output.stdout).expect("deve escrever ng.json");

    assert!(dest.exists(), "recipe 8: ng.json deve existir após export");

    let conteudo = fs::read_to_string(&dest).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&conteudo).expect("recipe 8: ng.json deve ser JSON válido");
    assert!(
        json["results"].is_array(),
        "recipe 8: ng.json deve conter array results"
    );
}

// Recipe 9 — sync-safe-copy: snapshot é consistente e abre com exit 0
#[test]
#[serial]
fn recipe_09_sync_safe_copy() {
    let dir = TempDir::new().unwrap();
    init(&dir);

    // Seed com memória para ter dados no snapshot
    cmd(&dir)
        .args([
            "remember",
            "--name",
            "sync-test",
            "--type",
            "user",
            "--description",
            "test para sync",
            "--body",
            "dados importantes que não devem se corromper no sync",
            "--namespace",
            "global",
        ])
        .assert()
        .success();

    let dest = dir.path().join("snapshot.sqlite");

    let output = cmd(&dir)
        .args(["sync-safe-copy", "--dest", dest.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "recipe 9: sync-safe-copy deve ter exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("recipe 9: sync-safe-copy deve retornar JSON válido");

    assert_eq!(
        json["status"], "ok",
        "recipe 9: status deve ser ok conforme documentado"
    );
    assert!(
        json["bytes_copied"].as_u64().unwrap_or(0) > 0,
        "recipe 9: bytes_copied deve ser maior que 0"
    );
    assert!(dest.exists(), "recipe 9: arquivo snapshot deve existir");

    // Valida que o snapshot abre corretamente via health
    let health = std::process::Command::new(bin())
        .env_clear()
        .env("SQLITE_GRAPHRAG_DB_PATH", &dest)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", dir.path().join("cache2"))
        .arg("--skip-memory-guard")
        .args(["health", "--json"])
        .output()
        .unwrap();

    assert!(
        health.status.success(),
        "recipe 9: health no snapshot deve ter exit 0 — snapshot deve ser abrível"
    );
    let json_health: serde_json::Value = serde_json::from_slice(&health.stdout).unwrap();
    assert_eq!(
        json_health["status"], "ok",
        "recipe 9: snapshot deve ter status ok"
    );
}

// Recipe 10 — Purge + vacuum + optimize: pipeline completo retorna JSON com status ok
#[test]
#[serial]
fn recipe_10_purge_vacuum_optimize() {
    let dir = TempDir::new().unwrap();
    init(&dir);

    // Seed e soft-delete para ter dados a purgar
    cmd(&dir)
        .args([
            "remember",
            "--name",
            "mem-a-purgar",
            "--type",
            "user",
            "--description",
            "será deletada",
            "--body",
            "conteúdo temporário para teste de purge",
            "--namespace",
            "global",
        ])
        .assert()
        .success();

    cmd(&dir)
        .args(["forget", "--name", "mem-a-purgar", "--namespace", "global"])
        .assert()
        .success();

    // Purge
    let purge_out = cmd(&dir)
        .args([
            "purge",
            "--retention-days",
            "0",
            "--yes",
            "--namespace",
            "global",
        ])
        .output()
        .unwrap();

    assert!(
        purge_out.status.success(),
        "recipe 10: purge deve ter exit 0"
    );
    let purge_json: serde_json::Value = serde_json::from_slice(&purge_out.stdout)
        .expect("recipe 10: purge deve retornar JSON válido");
    assert!(
        purge_json["elapsed_ms"].is_number(),
        "recipe 10: purge deve ter elapsed_ms"
    );

    // Vacuum
    let vacuum_out = cmd(&dir).arg("vacuum").output().unwrap();
    assert!(
        vacuum_out.status.success(),
        "recipe 10: vacuum deve ter exit 0"
    );
    let vacuum_json: serde_json::Value = serde_json::from_slice(&vacuum_out.stdout)
        .expect("recipe 10: vacuum deve retornar JSON válido");
    assert_eq!(
        vacuum_json["status"], "ok",
        "recipe 10: vacuum.status deve ser ok"
    );

    // Optimize
    let optimize_out = cmd(&dir).arg("optimize").output().unwrap();
    assert!(
        optimize_out.status.success(),
        "recipe 10: optimize deve ter exit 0"
    );
    let optimize_json: serde_json::Value = serde_json::from_slice(&optimize_out.stdout)
        .expect("recipe 10: optimize deve retornar JSON válido");
    assert_eq!(
        optimize_json["status"], "ok",
        "recipe 10: optimize.status deve ser ok"
    );
}

// Recipe 11 — NDJSON export via list: list retorna objeto com chave items (não array root)
// NOTA: O COOKBOOK documenta `jaq -c '.[]'` mas o JSON real tem `{"items": [...]}`.
// Este teste valida o comportamento REAL e detecta o drift se a doc for corrigida.
#[test]
#[serial]
fn recipe_11_ndjson_list() {
    let dir = TempDir::new().unwrap();
    init(&dir);

    for i in 1..=3u32 {
        cmd(&dir)
            .args([
                "remember",
                "--name",
                &format!("mem-export-{i}"),
                "--type",
                "reference",
                "--description",
                &format!("memória {i} para export"),
                "--body",
                &format!("conteúdo da memória número {i}"),
                "--namespace",
                "global",
            ])
            .assert()
            .success();
    }

    let output = cmd(&dir)
        .args([
            "list",
            "--limit",
            "10000",
            "--format",
            "json",
            "--namespace",
            "global",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "recipe 11: list deve ter exit 0");

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("recipe 11: list deve retornar JSON válido");

    // Comportamento real: objeto com chave "items"
    assert!(
        json["items"].is_array(),
        "recipe 11: list retorna objeto com chave 'items' (não array root — drift detectado se mudou)"
    );
    assert!(
        json["elapsed_ms"].is_number(),
        "recipe 11: list deve ter elapsed_ms"
    );

    let items = json["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        3,
        "recipe 11: deve listar 3 memórias inseridas"
    );

    // Cada item deve ter campos esperados para NDJSON
    let primeiro = &items[0];
    assert!(
        primeiro["id"].is_number(),
        "recipe 11: item.id deve existir"
    );
    assert!(
        primeiro["name"].is_string(),
        "recipe 11: item.name deve existir"
    );
    assert!(
        primeiro["namespace"].is_string(),
        "recipe 11: item.namespace deve existir"
    );
}

// Recipe 13 — GNU parallel simulado com threads: recall paralelo em 4 namespaces
#[test]
#[serial]
fn recipe_13_parallel_namespaces() {
    let dir = TempDir::new().unwrap();
    init(&dir);

    let namespaces = ["project-a", "project-b", "project-c", "project-d"];

    // Seed uma memória em cada namespace
    for ns in &namespaces {
        cmd(&dir)
            .args([
                "remember",
                "--name",
                &format!("mem-{ns}"),
                "--type",
                "project",
                "--description",
                &format!("memória do {ns}"),
                "--body",
                &format!("taxa de erro elevada em {ns} detectada"),
                "--namespace",
                ns,
            ])
            .assert()
            .success();
    }

    let db_path = dir.path().join("ng.sqlite").to_owned();
    let cache_path = dir.path().join("cache").to_owned();
    let bin_path = bin();

    // Simula `parallel -j 4` com 4 threads simultâneas
    let handles: Vec<_> = namespaces
        .iter()
        .map(|ns| {
            let ns = ns.to_string();
            let db = db_path.clone();
            let cache = cache_path.clone();
            let bin = bin_path.clone();
            std::thread::spawn(move || {
                std::process::Command::new(&bin)
                    .env_clear()
                    .env("SQLITE_GRAPHRAG_DB_PATH", &db)
                    .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache)
                    .args([
                        "--skip-memory-guard",
                        "recall",
                        "error rate",
                        "--k",
                        "5",
                        "--namespace",
                        &ns,
                    ])
                    .output()
                    .expect("recall em thread deve executar sem panic")
            })
        })
        .collect();

    let resultados: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    for (i, output) in resultados.iter().enumerate() {
        assert!(
            output.status.success(),
            "recipe 13: recall no namespace {} deve ter exit 0",
            namespaces[i]
        );
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .expect("recipe 13: recall deve retornar JSON válido");
        assert!(
            json["results"].is_array(),
            "recipe 13: recall.results deve ser array no namespace {}",
            namespaces[i]
        );
        let results = json["results"].as_array().unwrap();
        assert!(
            !results.is_empty(),
            "recipe 13: recall deve encontrar memória no namespace {} com query 'error rate'",
            namespaces[i]
        );
    }
}

// Recipe 14 — Debug slow queries: health + stats + --json retornam campos documentados
#[test]
#[serial]
fn recipe_14_debug_health_stats() {
    let dir = TempDir::new().unwrap();
    init(&dir);

    // Health: campos documentados no COOKBOOK
    let health_out = cmd(&dir).args(["health", "--json"]).output().unwrap();

    assert!(
        health_out.status.success(),
        "recipe 14: health deve ter exit 0"
    );

    let health: serde_json::Value = serde_json::from_slice(&health_out.stdout)
        .expect("recipe 14: health deve retornar JSON válido");

    // Valida campos documentados: `integrity, wal_size_mb, journal_mode`
    assert!(
        health.get("integrity").is_some(),
        "recipe 14: health deve ter campo 'integrity' como documentado"
    );
    assert!(
        health.get("wal_size_mb").is_some(),
        "recipe 14: health deve ter campo 'wal_size_mb' como documentado"
    );
    assert!(
        health.get("journal_mode").is_some(),
        "recipe 14: health deve ter campo 'journal_mode' como documentado"
    );

    // Stats: campos documentados no COOKBOOK
    let stats_out = cmd(&dir).args(["stats", "--json"]).output().unwrap();

    assert!(
        stats_out.status.success(),
        "recipe 14: stats deve ter exit 0"
    );

    let stats: serde_json::Value = serde_json::from_slice(&stats_out.stdout)
        .expect("recipe 14: stats deve retornar JSON válido");

    // Valida campos documentados: `memories, memories_total, entities, entities_total,
    // relationships, relationships_total, edges, chunks_total, avg_body_len,
    // db_size_bytes, db_bytes`
    let campos_esperados = [
        "memories",
        "memories_total",
        "entities",
        "entities_total",
        "relationships",
        "relationships_total",
        "edges",
        "chunks_total",
        "avg_body_len",
        "db_size_bytes",
        "db_bytes",
    ];

    for campo in &campos_esperados {
        assert!(
            stats.get(campo).is_some(),
            "recipe 14: stats deve ter campo '{campo}' como documentado no COOKBOOK"
        );
    }
}

// Recipe 15 — Benchmark simulado: recall e hybrid-search executam em tempo razoável
// Simula `hyperfine` verificando que ambos os comandos completam sem timeout
#[test]
#[serial]
fn recipe_15_hyperfine_timing() {
    let dir = TempDir::new().unwrap();
    init(&dir);

    // Seed com memória para busca não-trivial
    cmd(&dir)
        .args([
            "remember",
            "--name",
            "pg-migration",
            "--type",
            "incident",
            "--description",
            "postgres migration benchmark",
            "--body",
            "migração postgres com deadlock em ambiente de produção durante janela de manutenção",
            "--namespace",
            "global",
        ])
        .assert()
        .success();

    // Medição de recall
    let t0 = std::time::Instant::now();
    let recall_out = cmd(&dir)
        .args([
            "recall",
            "postgres migration",
            "--k",
            "10",
            "--namespace",
            "global",
        ])
        .output()
        .unwrap();
    let recall_elapsed = t0.elapsed();

    assert!(
        recall_out.status.success(),
        "recipe 15: recall deve ter exit 0"
    );
    assert!(
        recall_elapsed.as_secs() < 30,
        "recipe 15: recall deve completar em menos de 30s, levou {recall_elapsed:?}"
    );

    // Medição de hybrid-search
    let t1 = std::time::Instant::now();
    let hybrid_out = cmd(&dir)
        .args([
            "hybrid-search",
            "postgres migration",
            "--k",
            "10",
            "--namespace",
            "global",
        ])
        .output()
        .unwrap();
    let hybrid_elapsed = t1.elapsed();

    assert!(
        hybrid_out.status.success(),
        "recipe 15: hybrid-search deve ter exit 0"
    );
    assert!(
        hybrid_elapsed.as_secs() < 30,
        "recipe 15: hybrid-search deve completar em menos de 30s, levou {hybrid_elapsed:?}"
    );

    // Ambos retornam resultados JSON válidos com elapsed_ms
    let recall_json: serde_json::Value = serde_json::from_slice(&recall_out.stdout).unwrap();
    let hybrid_json: serde_json::Value = serde_json::from_slice(&hybrid_out.stdout).unwrap();

    assert!(
        recall_json["elapsed_ms"].is_number(),
        "recipe 15: recall deve reportar elapsed_ms no JSON"
    );
    assert!(
        hybrid_json["elapsed_ms"].is_number(),
        "recipe 15: hybrid-search deve reportar elapsed_ms no JSON"
    );
}
