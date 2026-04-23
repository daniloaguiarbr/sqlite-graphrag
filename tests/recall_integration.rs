use assert_cmd::Command;
use serial_test::serial;
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
// recall — DB não-inicializada
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_recall_falha_sem_init() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp)
        .args(["recall", "qualquer-query"])
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// recall — banco vazio retorna listas vazias
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_recall_banco_vazio_retorna_listas_vazias() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .args(["recall", "busca-em-banco-vazio"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["query"], "busca-em-banco-vazio");
    assert_eq!(json["direct_matches"].as_array().unwrap().len(), 0);
    assert_eq!(json["graph_matches"].as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// recall — query simples encontra memória existente
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_recall_query_simples_encontra_memoria() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    remember(
        &tmp,
        "rust-ownership",
        "skill",
        "Conceito de ownership em Rust",
        "Ownership é o sistema de gerenciamento de memória do Rust sem garbage collector",
    );

    let output = cmd(&tmp)
        .args(["recall", "ownership Rust memória"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["query"], "ownership Rust memória");
    let direct = json["direct_matches"].as_array().unwrap();
    assert!(
        !direct.is_empty(),
        "deve retornar ao menos uma correspondência direta"
    );
    assert_eq!(direct[0]["name"], "rust-ownership");
    assert_eq!(direct[0]["source"], "direct");
    assert!(direct[0]["distance"].as_f64().unwrap() >= 0.0);
}

// ---------------------------------------------------------------------------
// recall — campo snippet limitado a 300 chars
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_recall_snippet_limitado_a_300_chars() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let corpo_longo = "x".repeat(600);
    remember(
        &tmp,
        "memoria-longa",
        "project",
        "Memória com corpo muito longo",
        &corpo_longo,
    );

    let output = cmd(&tmp)
        .args(["recall", "memoria longa corpo"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let direct = json["direct_matches"].as_array().unwrap();
    if !direct.is_empty() {
        let snippet = direct[0]["snippet"].as_str().unwrap();
        assert!(
            snippet.len() <= 300,
            "snippet deve ter no máximo 300 chars, tem {}",
            snippet.len()
        );
    }
}

// ---------------------------------------------------------------------------
// recall — -k limita número de resultados
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_recall_k_limita_resultados() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    for i in 0..5 {
        remember(
            &tmp,
            &format!("memoria-k-{i}"),
            "user",
            &format!("Memória número {i} para teste de k"),
            &format!("Corpo da memória {i} com conteúdo sobre aprendizado e conhecimento"),
        );
    }

    let output = cmd(&tmp)
        .args(["recall", "memória aprendizado", "-k", "2"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let direct = json["direct_matches"].as_array().unwrap();
    assert!(
        direct.len() <= 2,
        "com -k 2 deve retornar no máximo 2 diretos, retornou {}",
        direct.len()
    );
}

// ---------------------------------------------------------------------------
// recall — --no-graph desativa expansão por grafo
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_recall_no_graph_desativa_expansao() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    remember(
        &tmp,
        "memoria-no-graph",
        "feedback",
        "Memória para teste de no-graph",
        "Conteúdo sobre configuração e preferências do usuário",
    );

    let output = cmd(&tmp)
        .args(["recall", "configuração preferências", "--no-graph"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        json["graph_matches"].as_array().unwrap().len(),
        0,
        "--no-graph deve resultar em lista graph_matches vazia"
    );
}

// ---------------------------------------------------------------------------
// recall — --namespace filtra por namespace
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_recall_namespace_filtra_resultados() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "mem-ns-alpha",
            "--type",
            "project",
            "--description",
            "Memória no namespace alpha",
            "--body",
            "Conteúdo do namespace alpha sobre projeto",
            "--namespace",
            "alpha",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "mem-ns-beta",
            "--type",
            "project",
            "--description",
            "Memória no namespace beta",
            "--body",
            "Conteúdo do namespace beta sobre projeto",
            "--namespace",
            "beta",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args([
            "recall",
            "projeto conteúdo",
            "--namespace",
            "alpha",
            "--no-graph",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let direct = json["direct_matches"].as_array().unwrap();
    for item in direct {
        assert_eq!(
            item["namespace"].as_str().unwrap(),
            "alpha",
            "todos resultados devem pertencer ao namespace alpha"
        );
    }
}

// ---------------------------------------------------------------------------
// recall — --type filtra por tipo de memória
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_recall_type_filtra_por_tipo() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    remember(
        &tmp,
        "mem-tipo-skill",
        "skill",
        "Habilidade de programação",
        "Saber programar em Rust é essencial para sistemas de alto desempenho",
    );
    remember(
        &tmp,
        "mem-tipo-user",
        "user",
        "Informação do usuário",
        "Usuário prefere Rust para desenvolvimento de sistemas",
    );

    let output = cmd(&tmp)
        .args([
            "recall",
            "Rust programação",
            "--type",
            "skill",
            "--no-graph",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let direct = json["direct_matches"].as_array().unwrap();
    for item in direct {
        assert_eq!(
            item["type"].as_str().unwrap(),
            "skill",
            "com --type skill todos resultados devem ser do tipo skill"
        );
    }
}

// ---------------------------------------------------------------------------
// recall — estrutura JSON de resposta válida
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_recall_estrutura_json_valida() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    remember(
        &tmp,
        "mem-json-check",
        "reference",
        "Referência para validação JSON",
        "Conteúdo de referência técnica sobre estrutura de dados",
    );

    let output = cmd(&tmp)
        .args(["recall", "referência técnica", "--no-graph"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();

    assert!(json.get("query").is_some(), "resposta deve ter campo query");
    assert!(
        json.get("k").is_some(),
        "resposta deve ter campo k (v2.0.0+)"
    );
    assert!(
        json["k"].as_u64().unwrap() > 0,
        "campo k deve ser inteiro positivo"
    );
    assert!(
        json.get("direct_matches").is_some(),
        "resposta deve ter campo direct_matches"
    );
    assert!(
        json.get("graph_matches").is_some(),
        "resposta deve ter campo graph_matches"
    );

    let direct = json["direct_matches"].as_array().unwrap();
    if !direct.is_empty() {
        let item = &direct[0];
        assert!(item.get("memory_id").is_some());
        assert!(item.get("name").is_some());
        assert!(item.get("namespace").is_some());
        assert!(item.get("type").is_some(), "campo 'type' deve existir");
        assert!(item.get("snippet").is_some());
        assert!(item.get("distance").is_some());
        assert!(item.get("source").is_some());
        assert_eq!(item["source"].as_str().unwrap(), "direct");
    }
}

// ---------------------------------------------------------------------------
// recall — múltiplas memórias, query reflete todas
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_recall_multiplas_memorias() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    remember(
        &tmp,
        "mem-multi-1",
        "feedback",
        "Feedback sobre testes automatizados",
        "Testes automatizados garantem qualidade do software e evitam regressões",
    );
    remember(
        &tmp,
        "mem-multi-2",
        "feedback",
        "Feedback sobre CI/CD",
        "Integração contínua melhora a entrega de software com testes automatizados",
    );
    remember(
        &tmp,
        "mem-multi-3",
        "feedback",
        "Feedback sobre code review",
        "Revisão de código com testes ajuda a manter padrão de qualidade",
    );

    let output = cmd(&tmp)
        .args(["recall", "testes qualidade software", "--no-graph"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let direct = json["direct_matches"].as_array().unwrap();
    assert!(
        direct.len() >= 2,
        "deve retornar ao menos 2 das 3 memórias relacionadas, retornou {}",
        direct.len()
    );
}
