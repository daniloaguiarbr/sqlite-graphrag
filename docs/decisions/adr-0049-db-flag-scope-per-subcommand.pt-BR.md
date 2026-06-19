# ADR-0049 — Escopo da Flag `--db` Por Subcomando (NÃO Global)

**Status**: Aceito
**Data**: 2026-06-19
**Contexto**: sqlite-graphrag v1.0.89 — GAP-E2E-008

## Contexto

`sqlite-graphrag` possui 49 subcomandos. O caminho do banco SQLite pode ser
sobrescrito via flag `--db <PATH>` ou variável de ambiente
`SQLITE_GRAPHRAG_DB_PATH`. Antes da v1.0.89, 5 subcomandos-folha esqueceram
de portar esse padrão:

- `embedding status`
- `embedding list`
- `embedding abandon`
- `pending list`
- `pending show`

Resultado: invocar `sqlite-graphrag embedding status --db /tmp/x.sqlite`
falhava com `unexpected argument` (clap rejeita).

## Decisão

Adicionar `pub db: Option<String>` em cada struct `Args` faltante, com
`#[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]`. NÃO usar
`clap::Arg::global = true`.

## Alternativas Consideradas

### A. `clap::Arg::global = true` para `--db`

Propagaria `--db` automaticamente para todos os subcomandos. Rejeitada:

- Invasiva: muda comportamento de TODOS os 49 subcomandos.
- Quebra o texto de help atual: cada subcomando mostraria `--db` em
  seu help, poluindo a saída.
- Foge do padrão atual do projeto: a struct `Cli` em `src/cli.rs`
  tem poucas flags globais (`-v`, `--lang`, `--tz`, `--max-concurrency`,
  `--wait-lock`). O resto é local.
- Dificulta debug: operador não sabe de qual flag o `--db` veio.

### B. Macro para evitar copy-paste

Poderia definir `db_field!()` macro para injetar o campo. Rejeitada:

- Acelera copy-paste mas não elimina o problema de longo prazo.
- Dificulta customização (cada subcomando pode querer texto de help
  diferente para `--db`).
- Aumenta superfície de bugs de macro.

### C. Wrapper no `AppPaths::resolve`

Centralizar a resolução do caminho. Considerada mas já existe (função
`AppPaths::resolve(args.db.as_deref())`). Não cobre a aceitação da
flag em si, apenas o uso.

## Consequências

### Positivas

- Padrão consistente: cada subcomando tem 100% de cobertura de
  `--db` via copy-paste de 5 linhas.
- Texto de help permanece limpo: `--db` aparece apenas nos subcomandos
  que precisam.
- Auditoria fácil: `rg 'pub db: Option<String>' src/commands/`
  lista todos os subcomandos com suporte.

### Negativas

- Copy-paste de 5 lugares (mitigado por ser apenas um campo).
- Esquecimentos futuros (mitigado pelo teste de regressão
  `tests/cli_db_flag_parity_regression.rs` que valida os 5 subcomandos).

## Implementação

```rust
#[derive(Debug, Args)]
pub struct EmbeddingStatusArgs {
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    // ... outros campos
}
```

## Validação

- `tests/cli_db_flag_parity_regression.rs::assert_db_flag_on_all_namespace_subcommands`
  garante que `embedding status/list/abandon`, `pending list/show` aceitam `--db`.
- Auditoria manual via `sqlite-graphrag embedding status --db /tmp/x --json`
  retorna JSON estruturado.

## Referências

- GAP-E2E-008 no plano v1.0.89.
- `rules_rust_json_e_ndjson.md:33` — política Must-Ignore (consistente
  com ADR-0048).
- `src/cli.rs` — struct `Cli` com flags globais mínimas.