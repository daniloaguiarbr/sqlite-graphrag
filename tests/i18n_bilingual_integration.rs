use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cmd_lang(tmp: &TempDir, lang: &str) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.env_remove("SQLITE_GRAPHRAG_LANG");
    c.env_remove("LC_ALL");
    c.env_remove("LANG");
    c.arg("--lang").arg(lang);
    c
}

fn cmd_env_lang(tmp: &TempDir, lang_val: &str) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.env("SQLITE_GRAPHRAG_LANG", lang_val);
    c.env_remove("LC_ALL");
    c.env_remove("LANG");
    c
}

fn cmd_no_lang(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.env_remove("SQLITE_GRAPHRAG_LANG");
    c.env_remove("LC_ALL");
    c.env_remove("LANG");
    c
}

fn init_db(tmp: &TempDir) {
    Command::cargo_bin("sqlite-graphrag")
        .unwrap()
        .env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"))
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .arg("init")
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Paridade EN/PT — variantes AppError via localized_message_for
// ---------------------------------------------------------------------------

#[test]
fn paridade_localized_message_todas_variantes_apperror() {
    use sqlite_graphrag::errors::AppError;
    use sqlite_graphrag::i18n::Language;
    use std::io;

    let variantes: Vec<AppError> = vec![
        AppError::Validation("campo x".into()),
        AppError::Duplicate("ns/mem".into()),
        AppError::Conflict("ts mudou".into()),
        AppError::NotFound("mem-x".into()),
        AppError::NamespaceError("sem marcador".into()),
        AppError::LimitExceeded("corpo enorme".into()),
        AppError::Embedding("dim errada".into()),
        AppError::VecExtension("extensao falhou".into()),
        AppError::DbBusy("retries esgotados".into()),
        AppError::BatchPartialFailure {
            total: 10,
            failed: 3,
        },
        AppError::Io(io::Error::new(io::ErrorKind::NotFound, "arquivo ausente")),
        AppError::LockBusy("outra instancia ativa".into()),
        AppError::AllSlotsFull {
            max: 4,
            waited_secs: 60,
        },
        AppError::LowMemory {
            available_mb: 100,
            required_mb: 500,
        },
    ];

    for variante in &variantes {
        let msg_en = variante.localized_message_for(Language::English);
        let msg_pt = variante.localized_message_for(Language::Portugues);

        assert!(
            !msg_en.is_empty(),
            "mensagem EN vazia para variante: {variante:?}"
        );
        assert!(
            !msg_pt.is_empty(),
            "mensagem PT vazia para variante: {variante:?}"
        );
        assert_ne!(
            msg_en, msg_pt,
            "mensagem EN e PT identicas para variante {variante:?}: '{msg_en}'"
        );
    }
}

#[test]
fn localized_message_en_cada_variante_contem_termo_ingles() {
    use sqlite_graphrag::errors::AppError;
    use sqlite_graphrag::i18n::Language;

    let casos: Vec<(AppError, &str)> = vec![
        (AppError::Validation("campo".into()), "validation error"),
        (AppError::Duplicate("ns/m".into()), "duplicate detected"),
        (AppError::Conflict("ts".into()), "conflict"),
        (AppError::NotFound("m".into()), "not found"),
        (
            AppError::NamespaceError("ns".into()),
            "namespace not resolved",
        ),
        (AppError::LimitExceeded("l".into()), "limit exceeded"),
        (AppError::Embedding("e".into()), "embedding error"),
        (
            AppError::VecExtension("v".into()),
            "sqlite-vec extension failed",
        ),
        (AppError::DbBusy("d".into()), "database busy"),
        (AppError::LockBusy("l".into()), "lock busy"),
    ];

    for (variante, esperado) in &casos {
        let msg = variante.localized_message_for(Language::English);
        assert!(
            msg.contains(esperado),
            "EN: esperado '{esperado}' em '{msg}' (variante: {variante:?})"
        );
    }
}

#[test]
fn localized_message_pt_cada_variante_contem_termo_portugues() {
    use sqlite_graphrag::errors::AppError;
    use sqlite_graphrag::i18n::Language;

    let casos: Vec<(AppError, &str)> = vec![
        (AppError::Validation("campo".into()), "erro de validação"),
        (AppError::Duplicate("ns/m".into()), "duplicata detectada"),
        (AppError::Conflict("ts".into()), "conflito"),
        (AppError::NotFound("m".into()), "não encontrado"),
        (
            AppError::NamespaceError("ns".into()),
            "namespace não resolvido",
        ),
        (AppError::LimitExceeded("l".into()), "limite excedido"),
        (AppError::Embedding("e".into()), "erro de embedding"),
        (AppError::VecExtension("v".into()), "sqlite-vec falhou"),
        (AppError::DbBusy("d".into()), "banco ocupado"),
        (AppError::LockBusy("l".into()), "lock ocupado"),
    ];

    for (variante, esperado) in &casos {
        let msg = variante.localized_message_for(Language::Portugues);
        assert!(
            msg.contains(esperado),
            "PT: esperado '{esperado}' em '{msg}' (variante: {variante:?})"
        );
    }
}

// ---------------------------------------------------------------------------
// Testes E2E via --lang flag
// ---------------------------------------------------------------------------

#[test]
fn lang_pt_remember_nome_invalido_stderr_portugues() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_lang(&tmp, "pt")
        .args([
            "remember",
            "--name",
            "NOME_INVALIDO_MAIUSCULA",
            "--type",
            "user",
            "--description",
            "descricao de teste",
            "--body",
            "conteudo",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("erro de validação")
                .or(predicate::str::contains("kebab-case")),
        );
}

#[test]
fn lang_en_mesmo_cenario_stderr_ingles() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_lang(&tmp, "en")
        .args([
            "remember",
            "--name",
            "NOME_INVALIDO_MAIUSCULA",
            "--type",
            "user",
            "--description",
            "test description",
            "--body",
            "conteudo",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("validation error").or(predicate::str::contains("kebab-case")),
        );
}

#[test]
fn lang_pt_not_found_stderr_portugues() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_lang(&tmp, "pt")
        .args(["read", "--name", "memoria-que-nao-existe"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("não encontrado"));
}

#[test]
fn lang_en_not_found_stderr_ingles() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_lang(&tmp, "en")
        .args(["read", "--name", "memoria-que-nao-existe"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn lang_pt_body_excede_limite_stderr_portugues() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let corpo_enorme = "x".repeat(20_001);
    cmd_lang(&tmp, "pt")
        .args([
            "remember",
            "--name",
            "mem-grande",
            "--type",
            "user",
            "--description",
            "descricao de teste",
            "--body",
            &corpo_enorme,
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("corpo excede")
                .or(predicate::str::contains("limite excedido")),
        );
}

#[test]
fn lang_en_body_excede_limite_stderr_ingles() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let corpo_enorme = "x".repeat(20_001);
    cmd_lang(&tmp, "en")
        .args([
            "remember",
            "--name",
            "mem-grande",
            "--type",
            "user",
            "--description",
            "test description",
            "--body",
            &corpo_enorme,
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("body exceeds").or(predicate::str::contains("limit exceeded")),
        );
}

// ---------------------------------------------------------------------------
// Testes E2E via env var SQLITE_GRAPHRAG_LANG
// ---------------------------------------------------------------------------

#[test]
fn env_var_sqlite_graphrag_lang_pt_aplica_portugues() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_env_lang(&tmp, "pt")
        .args(["read", "--name", "inexistente"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("não encontrado"));
}

#[test]
fn env_var_sqlite_graphrag_lang_en_aplica_ingles() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_env_lang(&tmp, "en")
        .args(["read", "--name", "inexistente"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn env_var_sqlite_graphrag_lang_pt_br_aplica_portugues() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_env_lang(&tmp, "pt-BR")
        .args(["read", "--name", "inexistente"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("não encontrado"));
}

// ---------------------------------------------------------------------------
// Flag --lang vence env var SQLITE_GRAPHRAG_LANG
// ---------------------------------------------------------------------------

#[test]
fn flag_lang_en_vence_env_lang_pt() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.env("SQLITE_GRAPHRAG_LANG", "pt");
    c.env_remove("LC_ALL");
    c.env_remove("LANG");
    c.arg("--lang").arg("en");
    c.args(["read", "--name", "inexistente"]);

    c.assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn flag_lang_pt_vence_env_lang_en() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.env("SQLITE_GRAPHRAG_LANG", "en");
    c.env_remove("LC_ALL");
    c.env_remove("LANG");
    c.arg("--lang").arg("pt");
    c.args(["read", "--name", "inexistente"]);

    c.assert()
        .failure()
        .stderr(predicate::str::contains("não encontrado"));
}

// ---------------------------------------------------------------------------
// Default sem flag e sem env var — fallback English
// ---------------------------------------------------------------------------

#[test]
fn default_sem_lang_e_sem_env_retorna_ingles() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_no_lang(&tmp)
        .args(["read", "--name", "inexistente"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ---------------------------------------------------------------------------
// Locale LC_ALL=pt_BR.UTF-8 sem flag e sem SQLITE_GRAPHRAG_LANG → Português
// ---------------------------------------------------------------------------

#[test]
fn locale_ptbr_sem_flag_sem_env_sqlite_graphrag_aplica_portugues() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.env_remove("SQLITE_GRAPHRAG_LANG");
    c.env("LC_ALL", "pt_BR.UTF-8");
    c.args(["read", "--name", "inexistente"]);

    c.assert()
        .failure()
        .stderr(predicate::str::contains("não encontrado"));
}

// ---------------------------------------------------------------------------
// Mensagens stdout JSON são idênticas em EN e PT (JSON é determinístico)
// ---------------------------------------------------------------------------

#[test]
fn json_stdout_identico_em_en_e_pt() {
    let tmp_en = TempDir::new().unwrap();
    let tmp_pt = TempDir::new().unwrap();
    init_db(&tmp_en);
    init_db(&tmp_pt);

    let saida_en = cmd_lang(&tmp_en, "en")
        .arg("health")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let saida_pt = cmd_lang(&tmp_pt, "pt")
        .arg("health")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json_en: serde_json::Value = serde_json::from_slice(&saida_en).unwrap();
    let json_pt: serde_json::Value = serde_json::from_slice(&saida_pt).unwrap();

    assert_eq!(
        json_en["status"], json_pt["status"],
        "campo status difere entre EN e PT"
    );
    assert_eq!(
        json_en["integrity"], json_pt["integrity"],
        "campo integrity difere entre EN e PT"
    );
}

// ---------------------------------------------------------------------------
// Alias do idioma — aliases aceitos: english, portugues, pt-BR, pt-br
// ---------------------------------------------------------------------------

#[test]
fn alias_english_aceito_pela_cli() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let saida = Command::cargo_bin("sqlite-graphrag")
        .unwrap()
        .env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"))
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .env_remove("SQLITE_GRAPHRAG_LANG")
        .env_remove("LC_ALL")
        .env_remove("LANG")
        .arg("--lang")
        .arg("en")
        .arg("health")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&saida).unwrap();
    assert_eq!(json["status"], "ok");
}

#[test]
fn alias_pt_br_aceito_pela_cli() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let saida = Command::cargo_bin("sqlite-graphrag")
        .unwrap()
        .env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"))
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"))
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .env_remove("SQLITE_GRAPHRAG_LANG")
        .env_remove("LC_ALL")
        .env_remove("LANG")
        .arg("--lang")
        .arg("pt")
        .arg("health")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&saida).unwrap();
    assert_eq!(json["status"], "ok");
}
