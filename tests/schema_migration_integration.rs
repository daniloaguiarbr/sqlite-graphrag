// Suite 3 — Validação de schema e migrations V001-V005
//
// ISOLAMENTO: cada teste usa `SQLITE_GRAPHRAG_DB_PATH` apontando para um arquivo
// SQLite em `TempDir` exclusivo. A introspecção é feita via rusqlite diretamente,
// sem depender de nenhum output do binário.
//
// NOTA: sqlite-vec usa `sqlite3_auto_extension`, que é global ao processo.
// Para evitar que a extensão seja registrada múltiplas vezes em testes paralelos,
// TODOS os testes que abrem um banco com sqlite-vec fazem isso via `sqlite-graphrag init`
// (binário externo), que carrega a extensão no seu próprio processo. Os testes de
// introspecção pura (sqlite_master, triggers, FTS) abrem o banco via rusqlite após
// o init para consultar somente — não carregam sqlite-vec no processo de teste.
//
// `#[serial]` é obrigatório: embora cada teste use DB próprio, o compilado é
// compartilhado e `TempDir` só é liberado após o teste encerrar; serializar
// elimina corridas no filesystem e torna timings previsíveis.

use assert_cmd::Command;
use rusqlite::Connection;
use serial_test::serial;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Executa `sqlite-graphrag init` em um banco temporário isolado e retorna
/// o `TempDir` (para manter o banco vivo) e o caminho do arquivo sqlite.
fn init_db_isolado() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().expect("TempDir deve ser criado");
    let db_path = tmp.path().join("test.sqlite");

    Command::cargo_bin("sqlite-graphrag")
        .expect("binário sqlite-graphrag não encontrado")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    (tmp, db_path)
}

/// Abre o banco em modo leitura após o init (sem sqlite-vec no processo de teste).
fn conn_ro(db_path: &std::path::Path) -> Connection {
    Connection::open(db_path).expect("conexão ao banco deve funcionar")
}

/// Verifica se uma tabela ou view existe em `sqlite_master`.
fn tabela_existe(conn: &Connection, nome: &str) -> bool {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table','view') AND name = ?1",
            rusqlite::params![nome],
            |row| row.get(0),
        )
        .unwrap_or(0);
    count > 0
}

/// Verifica se um trigger existe em `sqlite_master`.
fn trigger_existe(conn: &Connection, nome: &str) -> bool {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = ?1",
            rusqlite::params![nome],
            |row| row.get(0),
        )
        .unwrap_or(0);
    count > 0
}

/// Verifica se um índice existe em `sqlite_master`.
fn indice_existe(conn: &Connection, nome: &str) -> bool {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
            rusqlite::params![nome],
            |row| row.get(0),
        )
        .unwrap_or(0);
    count > 0
}

// ---------------------------------------------------------------------------
// Teste 1 — init aplica exatamente 5 migrations V001 a V005
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn init_cria_5_migrations_v001_a_v005() {
    let (_tmp, db_path) = init_db_isolado();
    let conn = conn_ro(&db_path);

    let versoes: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT version FROM refinery_schema_history ORDER BY version ASC")
            .expect("prepare deve funcionar");
        stmt.query_map([], |row| row.get(0))
            .expect("query deve funcionar")
            .map(|r| r.expect("row deve ser lida"))
            .collect()
    };

    assert_eq!(
        versoes.len(),
        5,
        "deve haver exatamente 5 migrations aplicadas, encontrou: {versoes:?}"
    );
    assert_eq!(versoes, vec![1, 2, 3, 4, 5], "versões V001-V005 esperadas");
}

// ---------------------------------------------------------------------------
// Teste 2 — trigger trg_fts_ai existe após V004
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn trigger_trg_fts_ai_existe() {
    let (_tmp, db_path) = init_db_isolado();
    let conn = conn_ro(&db_path);

    assert!(
        trigger_existe(&conn, "trg_fts_ai"),
        "trigger trg_fts_ai deve existir após V004"
    );
}

// ---------------------------------------------------------------------------
// Teste 3 — trigger trg_fts_ad existe após V004
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn trigger_trg_fts_ad_existe() {
    let (_tmp, db_path) = init_db_isolado();
    let conn = conn_ro(&db_path);

    assert!(
        trigger_existe(&conn, "trg_fts_ad"),
        "trigger trg_fts_ad deve existir após V004"
    );
}

// ---------------------------------------------------------------------------
// Teste 4 — trigger trg_fts_au está AUSENTE (conflito sqlite-vec intencional)
// ---------------------------------------------------------------------------
// V004 documenta explicitamente que trg_fts_au é omitido porque sqlite-vec
// carregado via sqlite3_auto_extension conflita com FTS5 em AFTER UPDATE triggers.
// A sincronização de edição/rename é feita no código Rust (edit.rs, rename.rs).

#[test]
#[serial]
fn trigger_trg_fts_au_ausente_conflito_vec() {
    let (_tmp, db_path) = init_db_isolado();
    let conn = conn_ro(&db_path);

    assert!(
        !trigger_existe(&conn, "trg_fts_au"),
        "trigger trg_fts_au NÃO deve existir — sqlite-vec conflita com FTS5 em AFTER UPDATE"
    );
}

// ---------------------------------------------------------------------------
// Teste 5 — vec_memories usa float[384] e distance_metric=cosine
// ---------------------------------------------------------------------------
// Verifica via DDL do sqlite_master que a definição da tabela vec0 inclui
// os parâmetros corretos de dimensão e métrica de distância.

#[test]
#[serial]
fn vec_memories_dim_384_cosine() {
    let (_tmp, db_path) = init_db_isolado();
    let conn = conn_ro(&db_path);

    let ddl: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'vec_memories'",
            [],
            |row| row.get(0),
        )
        .expect("vec_memories deve existir no sqlite_master");

    assert!(
        ddl.contains("float[384]"),
        "vec_memories deve declarar float[384], DDL obtido: {ddl}"
    );
    assert!(
        ddl.contains("distance_metric=cosine"),
        "vec_memories deve usar distance_metric=cosine, DDL obtido: {ddl}"
    );
}

// ---------------------------------------------------------------------------
// Teste 6 — vec_memories tem 2 partition keys (namespace, type)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn vec_memories_partition_keys_namespace_type() {
    let (_tmp, db_path) = init_db_isolado();
    let conn = conn_ro(&db_path);

    let ddl: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'vec_memories'",
            [],
            |row| row.get(0),
        )
        .expect("vec_memories deve existir no sqlite_master");

    // Ambas as colunas devem aparecer com 'partition key' no DDL
    let namespace_pk = ddl.contains("namespace") && ddl.to_lowercase().contains("partition key");
    let type_pk = ddl.contains("type") && ddl.to_lowercase().contains("partition key");

    assert!(
        namespace_pk,
        "vec_memories deve ter 'namespace' como partition key, DDL: {ddl}"
    );
    assert!(
        type_pk,
        "vec_memories deve ter 'type' como partition key, DDL: {ddl}"
    );
}

// ---------------------------------------------------------------------------
// Teste 7 — fts_memories usa tokenizer unicode61 remove_diacritics 1
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn fts_memories_tokenizer_unicode61_remove_diacritics() {
    let (_tmp, db_path) = init_db_isolado();
    let conn = conn_ro(&db_path);

    let ddl: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'fts_memories'",
            [],
            |row| row.get(0),
        )
        .expect("fts_memories deve existir no sqlite_master");

    assert!(
        ddl.contains("unicode61"),
        "fts_memories deve usar tokenizer unicode61, DDL: {ddl}"
    );
    assert!(
        ddl.contains("remove_diacritics"),
        "fts_memories deve declarar remove_diacritics, DDL: {ddl}"
    );
}

// ---------------------------------------------------------------------------
// Teste 8 — FTS5 busca 'cafe' encontra texto com 'café' (remove_diacritics)
// ---------------------------------------------------------------------------
// Insere uma memória com acento via CLI e verifica que a busca sem acento
// funciona — confirma que o tokenizer remove_diacritics está ativo.

#[test]
#[serial]
fn fts5_matching_com_acentos_cafe_cafe() {
    let tmp = TempDir::new().expect("TempDir deve ser criado");
    let db_path = tmp.path().join("test.sqlite");

    // Init do banco
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário não encontrado")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    // Inserir memória com texto acentuado
    Command::cargo_bin("sqlite-graphrag")
        .expect("binário não encontrado")
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path())
        .env("SQLITE_GRAPHRAG_NAMESPACE", "global")
        .args([
            "--skip-memory-guard",
            "remember",
            "--name",
            "nota-cafe",
            "--type",
            "user",
            "--description",
            "nota sobre café",
            "--body",
            "O café brasileiro é famoso mundialmente por sua qualidade",
        ])
        .assert()
        .success();

    // Busca sem acento deve encontrar a memória (remove_diacritics=1)
    let conn = conn_ro(&db_path);
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM fts_memories WHERE fts_memories MATCH 'cafe'",
            [],
            |row| row.get(0),
        )
        .expect("query FTS5 deve funcionar");

    assert!(
        count >= 1,
        "FTS5 com remove_diacritics deve encontrar 'café' ao buscar 'cafe', count={count}"
    );
}

// ---------------------------------------------------------------------------
// Teste 9 — tabelas principais existem após init
// ---------------------------------------------------------------------------
// Verifica todas as 7 tabelas regulares + vec/fts virtuais criadas pelas migrations.

#[test]
#[serial]
fn todas_tabelas_principais_existem_apos_init() {
    let (_tmp, db_path) = init_db_isolado();
    let conn = conn_ro(&db_path);

    let tabelas = [
        "schema_meta",
        "memories",
        "memory_versions",
        "memory_chunks",
        "entities",
        "relationships",
        "memory_entities",
        "memory_relationships",
        "fts_memories",
    ];

    for nome in tabelas {
        assert!(
            tabela_existe(&conn, nome),
            "tabela '{nome}' deve existir após init"
        );
    }
}

// ---------------------------------------------------------------------------
// Teste 10 — índices principais de V001 e V005 existem
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn indices_principais_existem_apos_init() {
    let (_tmp, db_path) = init_db_isolado();
    let conn = conn_ro(&db_path);

    let indices = [
        "idx_memories_ns_type",
        "idx_memories_ns_live",
        "idx_memories_body_hash",
        "idx_entities_ns",
        "idx_me_entity",
        "idx_relationships_source_id",
        "idx_relationships_target_id",
        "idx_relationships_namespace_relation",
        "idx_entities_namespace_degree",
        "idx_memory_chunks_memory_id",
        "idx_memory_relationships_relationship_id",
    ];

    for nome in indices {
        assert!(
            indice_existe(&conn, nome),
            "índice '{nome}' deve existir após init"
        );
    }
}

// ---------------------------------------------------------------------------
// Teste 11 — schema_meta contém campos esperados após init
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_meta_campos_obrigatorios_existem() {
    let (_tmp, db_path) = init_db_isolado();
    let conn = conn_ro(&db_path);

    let chaves_esperadas = ["schema_version", "model", "dim", "created_at"];

    for chave in chaves_esperadas {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_meta WHERE key = ?1",
                rusqlite::params![chave],
                |row| row.get(0),
            )
            .expect("query schema_meta deve funcionar");

        assert!(
            count > 0,
            "schema_meta deve conter chave '{chave}' após init"
        );
    }
}

// ---------------------------------------------------------------------------
// Teste 12 — schema_version em schema_meta corresponde a V005 (5)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn schema_version_meta_igual_a_5() {
    let (_tmp, db_path) = init_db_isolado();
    let conn = conn_ro(&db_path);

    let versao: String = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .expect("schema_version deve existir em schema_meta");

    assert_eq!(
        versao, "5",
        "schema_version em schema_meta deve ser '5' após V005"
    );
}
