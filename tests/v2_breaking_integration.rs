#![cfg(feature = "slow-tests")]

use assert_cmd::Command;
use tempfile::TempDir;

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

fn remember(tmp: &TempDir, name: &str, memory_type: &str, description: &str, body: &str) {
    cmd(tmp)
        .args([
            "remember",
            "--name",
            name,
            "--type",
            memory_type,
            "--description",
            description,
            "--body",
            body,
        ])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// purge --dry-run — não deleta nada
// ---------------------------------------------------------------------------

#[test]
fn purge_dry_run_nao_deleta_nada() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    remember(
        &tmp,
        "mem-dry-run",
        "user",
        "Memória para dry-run",
        "Corpo de memória que não deve ser deletada em dry-run",
    );

    cmd(&tmp)
        .args(["forget", "--name", "mem-dry-run"])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args([
            "purge",
            "--name",
            "mem-dry-run",
            "--dry-run",
            "--retention-days",
            "0",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["dry_run"], true, "dry_run deve ser true na resposta");

    cmd(&tmp)
        .args(["purge", "--name", "mem-dry-run", "--retention-days", "0"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// purge sem --retention-days — constante default é 90
// ---------------------------------------------------------------------------

#[test]
fn purge_retention_days_padrao_90() {
    // Verifica diretamente a constante — o comportamento de CLI (retenção de 90 dias)
    // significa que memórias recém-deletadas NÃO são incluídas no cutoff padrão.
    // Testamos que a constante está correta sem precisar manipular timestamps.
    assert_eq!(
        sqlite_graphrag::constants::PURGE_RETENTION_DAYS_DEFAULT,
        90,
        "PURGE_RETENTION_DAYS_DEFAULT deve ser 90"
    );

    // Verifica também que o campo retention_days_used aparece na resposta quando
    // há memórias elegíveis (usando retention_days=0 para forçar inclusão imediata).
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    remember(
        &tmp,
        "mem-retention-check",
        "user",
        "Memória para checar campo retention_days_used na resposta",
        "Corpo da memória para validação de retention days no response shape",
    );

    cmd(&tmp)
        .args(["forget", "--name", "mem-retention-check"])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args([
            "purge",
            "--name",
            "mem-retention-check",
            "--dry-run",
            "--retention-days",
            "0",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        json["retention_days_used"].as_u64().unwrap(),
        0u64,
        "retention_days_used deve refletir o valor passado via --retention-days"
    );
    assert!(
        json.get("dry_run").is_some(),
        "resposta deve ter campo dry_run"
    );
    assert!(
        json.get("bytes_freed").is_some(),
        "resposta deve ter campo bytes_freed"
    );
    assert!(
        json.get("oldest_deleted_at").is_some(),
        "resposta deve ter campo oldest_deleted_at"
    );
}

// ---------------------------------------------------------------------------
// hybrid-search response shape tem campo results
// ---------------------------------------------------------------------------

#[test]
fn hybrid_search_response_shape_tem_results() {
    use sqlite_graphrag::commands::hybrid_search::{
        HybridSearchItem, HybridSearchResponse, Weights,
    };
    use sqlite_graphrag::output::RecallItem;
    let resp = HybridSearchResponse {
        query: "consulta de teste".to_string(),
        k: 5,
        rrf_k: 60,
        weights: Weights { vec: 1.0, fts: 1.0 },
        elapsed_ms: 0,
        results: vec![HybridSearchItem {
            memory_id: 1,
            name: "mem-1".to_string(),
            namespace: "global".to_string(),
            memory_type: "user".to_string(),
            description: "descrição".to_string(),
            body: "corpo".to_string(),
            combined_score: 0.95,
            score: 0.95,
            source: "hybrid".to_string(),
            vec_rank: Some(1),
            fts_rank: Some(2),
        }],
        graph_matches: vec![RecallItem {
            memory_id: 2,
            name: "mem-2".to_string(),
            namespace: "global".to_string(),
            memory_type: "project".to_string(),
            description: "desc2".to_string(),
            snippet: "trecho".to_string(),
            distance: 0.3,
            source: "graph".to_string(),
        }],
    };

    let json_str = serde_json::to_string(&resp).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert!(
        json.get("results").is_some(),
        "resposta deve ter campo results"
    );
    assert!(json.get("k").is_some(), "resposta deve ter campo k");
    assert!(
        json.get("graph_matches").is_some(),
        "resposta deve ter campo graph_matches"
    );
    assert!(json.get("query").is_some(), "resposta deve ter campo query");

    let results = json["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert!(
        results[0].get("combined_score").is_some(),
        "item deve ter combined_score"
    );
    assert!(
        results[0].get("vec_rank").is_some(),
        "item deve ter vec_rank"
    );
    assert!(
        results[0].get("fts_rank").is_some(),
        "item deve ter fts_rank"
    );
    assert!(results[0].get("body").is_some(), "item deve ter body");

    assert!(
        json.get("combined_rank").is_none(),
        "campo combined_rank removido em v2.0.0 — shape old não deve existir"
    );
    assert!(
        json.get("vec_rank").is_none(),
        "campo vec_rank raiz removido em v2.0.0 — shape old não deve existir"
    );
    assert!(
        json.get("fts_rank").is_none(),
        "campo fts_rank raiz removido em v2.0.0 — shape old não deve existir"
    );
}

// ---------------------------------------------------------------------------
// DbBusy mapeia para exit code 15 em v2.0.0
// ---------------------------------------------------------------------------

#[test]
fn db_busy_exit_code_15() {
    use sqlite_graphrag::constants::DB_BUSY_EXIT_CODE;
    use sqlite_graphrag::errors::AppError;

    let err = AppError::DbBusy("esgotou retries após 5 tentativas".into());
    assert_eq!(
        err.exit_code(),
        15,
        "DbBusy deve mapear para exit 15 em v2.0.0"
    );
    assert_eq!(
        err.exit_code(),
        DB_BUSY_EXIT_CODE,
        "DbBusy exit code deve bater com constante DB_BUSY_EXIT_CODE"
    );
}

// ---------------------------------------------------------------------------
// BatchPartialFailure mapeia para exit code 13
// ---------------------------------------------------------------------------

#[test]
fn batch_partial_failure_exit_code_13() {
    use sqlite_graphrag::constants::BATCH_PARTIAL_FAILURE_EXIT_CODE;
    use sqlite_graphrag::errors::AppError;

    let err = AppError::BatchPartialFailure {
        total: 100,
        failed: 7,
    };
    assert_eq!(
        err.exit_code(),
        13,
        "BatchPartialFailure deve mapear para exit 13"
    );
    assert_eq!(
        err.exit_code(),
        BATCH_PARTIAL_FAILURE_EXIT_CODE,
        "BatchPartialFailure exit code deve bater com constante BATCH_PARTIAL_FAILURE_EXIT_CODE"
    );
}

// ---------------------------------------------------------------------------
// NAME_SLUG_REGEX permite single char [a-z0-9]
// ---------------------------------------------------------------------------

#[test]
fn name_slug_regex_permite_single_digit() {
    use regex::Regex;
    use sqlite_graphrag::constants::NAME_SLUG_REGEX;

    let re = Regex::new(NAME_SLUG_REGEX).unwrap();

    assert!(re.is_match("9"), "single digit '9' deve ser válido");
    assert!(re.is_match("a"), "single letter 'a' deve ser válido");
    assert!(re.is_match("z"), "single letter 'z' deve ser válido");
    assert!(re.is_match("0"), "single digit '0' deve ser válido");
}

// ---------------------------------------------------------------------------
// NAME_SLUG_REGEX rejeita multichar com prefixo dígito
// ---------------------------------------------------------------------------

#[test]
fn name_slug_regex_rejeita_prefixo_digito_multichar() {
    use regex::Regex;
    use sqlite_graphrag::constants::NAME_SLUG_REGEX;

    let re = Regex::new(NAME_SLUG_REGEX).unwrap();

    assert!(
        !re.is_match("1abc"),
        "multichar '1abc' começando com dígito deve ser rejeitado"
    );
    assert!(
        !re.is_match("9memoria"),
        "multichar '9memoria' começando com dígito deve ser rejeitado"
    );
    assert!(
        !re.is_match("42test"),
        "multichar '42test' começando com dígito deve ser rejeitado"
    );

    assert!(
        re.is_match("abc"),
        "multichar 'abc' começando com letra deve ser aceito"
    );
    assert!(
        re.is_match("memoria-teste"),
        "'memoria-teste' deve ser válido"
    );
}
