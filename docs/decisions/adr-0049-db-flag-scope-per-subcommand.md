# ADR-0049 — `--db` Flag Scope Per Subcommand (NOT Global)

**Status**: Accepted
**Date**: 2026-06-19
**Context**: sqlite-graphrag v1.0.89 — GAP-E2E-008

## Contexto

`sqlite-graphrag` tem 49 subcommands. O caminho do banco SQLite pode ser
sobrescrito via flag `--db <PATH>` ou env var `SQLITE_GRAPHRAG_DB_PATH`.
Antes da v1.0.89, 5 subcommands-folha esqueceram de portar esse padrão:

- `embedding status`
- `embedding list`
- `embedding abandon`
- `pending list`
- `pending show`

Resultado: invocar `sqlite-graphrag embedding status --db /tmp/x.sqlite`
falhava com `unexpected argument` (clap reject).

## Decisão

Adicionar `pub db: Option<String>` em cada `Args` struct faltante, com
`#[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]`. NÃO usar
`clap::Arg::global = true`.

## Alternativas Consideradas

### A. `clap::Arg::global = true` para `--db`

Propagaria `--db` automaticamente para todos os subcommands. Rejeitada:

- Invasiva: muda comportamento de TODOS os 49 subcommands.
- Quebra o help text atual: cada subcommand mostraria `--db` em sua help,
  poluindo saída.
- Foge do padrão atual do projeto: o struct `Cli` em `src/cli.rs`
  tem poucas flags globais (`-v`, `--lang`, `--tz`, `--max-concurrency`,
  `--wait-lock`). O resto é local.
- Dificulta debugging: operador não sabe de qual flag o `--db` veio.

### B. Macro para evitar copy-paste

Poderia definir `db_field!()` macro para injetar o campo. Rejeitada:

- Acelera copy-paste mas não elimina o problema de longo prazo.
- Dificulta customização (cada subcommand pode querer help text
  diferente para `--db`).
- Aumenta superfície de bugs de macro.

### C. Wrapper no `AppPaths::resolve`

Centralizar a resolução do path. Considerada mas já existe (função
`AppPaths::resolve(args.db.as_deref())`). Não cobre a aceitação do
flag em si, apenas o uso.

## Consequências

### Positivas

- Padrão consistente: cada subcommand tem 100% de cobertura de
  `--db` via copy-paste de 5 linhas.
- Help text permanece limpo: `--db` aparece apenas nos subcommands
  que precisam.
- Fácil auditoria: `rg 'pub db: Option<String>' src/commands/`
  lista todos os subcommands com suporte.

### Negativas

- Copy-paste de 5 lugares (mitigado por ser apenas um campo).
- Esquecimentos futuros (mitigado pelo regression test
  `tests/cli_db_flag_parity_regression.rs` que valida os 5 subcommands).

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
- `rules_rust_json_e_ndjson.md:33` — Must-Ignore policy (consistente
  com ADR-0048).
- `src/cli.rs` — struct `Cli` com flags globais mínimas.