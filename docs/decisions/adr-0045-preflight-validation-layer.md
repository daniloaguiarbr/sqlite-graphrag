# ADR-0045: Pre-Flight Validation Layer for LLM Subprocess Spawners

- **Status**: Accepted
- **Data**: 2026-06-19
- **Versão**: v1.0.87 (closes GAP-META-005)
- **Autores**: Danilo Aguiar <daniloaguiarbr@gmail.com>

## Context

`sqlite-graphrag` v1.0.86 invoca o binário externo `claude -p` (e análogos) através de `std::process::Command::spawn()` em **4 pontos de entrada** distintos do código:

- `src/commands/claude_runner.rs:255` (job `enrich`, modo `claude-code`)
- `src/commands/codex_spawn.rs:273` (job `ingest --mode codex`)
- `src/commands/ingest_claude.rs:297` (job `ingest --mode claude-code`)
- `src/extract/llm_embedding.rs:670` (embeddings LLM headless)

Nenhum dos 4 spawners executa uma **camada de pré-validação** entre a
construção do `argv` e a chamada `cmd.spawn()`. A consequência é que 5
classes distintas de falha são detectadas apenas DEPOIS que o kernel
forkou o processo filho e o claude começou a executar, quando o erro
já é caro de recuperar.

### Os 5 bugs-sintoma (todos em `v1.0.86`)

- **Bug 1** — `ingest --extraction-backend llm` salva corpo mas extrai
  `entities:0` em modo degradado silencioso
- **Bug 2** — `--mcp-config '{}'` literal rejeitado por Claude Code 2.1.177
  com "Invalid MCP configuration" — Claude espera filepath, não JSON inline
- **Bug 3** — argv > ARG_MAX (~3.200.000 bytes no Fedora 7.0.12) gera
  `E2BIG` pós-fork para corpos de memória grandes
- **Bug 4** — Parser JSON trunca em 65.536 chars; corpos com muitas
  entidades extraídas excedem buffer fixo
- **Bug 5** — Claude Code 2.1.177 faz walk-up de `.mcp.json` em
  diretórios ancestrais do CWD; herança quebra validação Zod mesmo
  com flags `--strict-mcp-config --mcp-config '{}'` presentes

### Causa raiz arquitetural

A CLI evoluiu de multi-shot (v1.0.74 daemon) para one-shot LLM-only
(v1.0.76+) mas preservou o modelo mental "se der erro, é erro do
subprocesso, não meu". A camada `env_whitelist.rs` (v1.0.83, ADR-0041)
provou que **um helper compartilhado pode cobrir múltiplos spawners**,
mas ninguém estendeu o padrão para argv, paths, ou output buffering.

## Decision

Criar `src/spawn/preflight.rs` (≥200 linhas) exportando uma função
pública `preflight_check(args: &PreFlightArgs) -> Result<(), PreFlightError>`
invocada pelos 4 spawners como **gate obrigatório** antes de
`Command::spawn()`.

### API pública

```rust
pub struct PreFlightArgs<'a> {
    pub binary_path: &'a Path,
    pub argv: &'a [OsString],
    pub workspace_root: &'a Path,
    pub mcp_config_inline_json: Option<&'a str>,
    pub expected_output_bytes: usize,
    pub spawner_name: &'static str,
}

pub enum PreFlightError {
    BinaryNotFound { path: PathBuf },
    ArgvExceedsArgMax { total_bytes: usize, arg_max: usize },
    McpConfigInlineJsonRejected(String),
    McpConfigPathMissing { path: PathBuf },
    McpConfigPathInvalidJson { path: PathBuf, error: String },
    WalkUpMcpJsonInvalid { path: PathBuf, error: String },
    OutputBufferTooSmall { expected: usize, configured: usize },
    ClaudeConfigDirNotEmpty { path: PathBuf },
}

pub fn preflight_check(args: &PreFlightArgs) -> Result<(), PreFlightError>;
pub fn write_empty_mcp_config_tempfile() -> Result<PathBuf, std::io::Error>;
```

### Comportamento das 7 guards (ordem importa)

1. **`check_argv_size`** — soma bytes do argv + 1 byte NUL separator por
   elemento; compara com `libc::sysconf(_SC_ARG_MAX) - 4096` safety
   margin. Falha com `ArgvExceedsArgMax`.
2. **`check_binary_exists`** — `binary_path.exists()`. Falha com
   `BinaryNotFound`.
3. **`check_output_buffer`** — se `expected_output_bytes > 65_536`,
   falha com `OutputBufferTooSmall`.
4. **`check_mcp_config_inline`** — se `mcp_config_inline_json == Some("{...}")`,
   falha com `McpConfigInlineJsonRejected` (caller usa
   `write_empty_mcp_config_tempfile()` para substituir).
5. **`check_mcp_config_path`** — se argv contém `--mcp-config <PATH>`,
   valida que path existe e tem JSON válido.
6. **`check_walkup_mcp_json`** — sobe de `workspace_root` até 16 níveis,
   procura `.mcp.json`; se inválido, falha com `WalkUpMcpJsonInvalid`.
7. **`check_claude_config_dir`** — se `CLAUDE_CONFIG_DIR` aponta para
   diretório não-vazio, falha com `ClaudeConfigDirNotEmpty`.

### Trade-offs

- **Latência**: ~1ms por spawn (aceitável para jobs de minutos)
- **Compatibilidade**: exit code 16 é novo — scripts que tratam exit 1
  continuam funcionando
- **Opt-out**: `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` documentado para
  emergências (emit warning estruturado)
- **API breakage**: `build_claude_command` e `build_codex_command`
  continuam retornando `Command` (não `Result`) para preservar
  assinatura; preflight failure chama `std::process::exit(16)`

### Por que pre-flight é uma camada separada de `env_whitelist`

- `env_whitelist.rs` (ADR-0041) lida com **env vars** apenas
- preflight lida com **argv, paths, output buffer, config dirs** — não
  são env vars; mistura quebraria SRP
- Pattern compartilhado: helper consumido pelos 4 spawners, sem
  reimplementação local

### Decisões específicas para v1.0.87

1. **Bug 2 fix**: spawners claude usam `write_empty_mcp_config_tempfile()`
   que escreve `{"mcpServers":{}}` em `tempdir().persist()` e retornam
   o path; o path é passado como `--mcp-config <PATH>` em vez do literal
   `'{}'`.

2. **Bug 5 fix**: walk-up limitado a 16 níveis para evitar lentidão;
   detecta `.mcp.json` herdado de diretório pai antes do fork.

3. **Bug 3 fix**: `libc::sysconf(_SC_ARG_MAX)` com fallback `32_768`
   para Windows (CreateProcess cap).

4. **Bug 4 fix**: `expected_output_bytes` é estimativa do caller;
   default `65_536` reflete o cap histórico do parser.

5. **Bug 1 fix**: o tracing event `preflight_passed`/`preflight_failed`
   confirma a invocação; jobs sem preflight_passed indicam modo degradado.

### Telemetria

- `tracing::info!(target: "preflight", event = "preflight_passed", spawner, argv_bytes, workspace_root)`
- `tracing::warn!(target: "preflight", event = "preflight_failed", spawner, error)`
- `tracing::warn!(target: "preflight", event = "preflight_skipped", spawner)` quando
  `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1`

### Integração com os 4 spawners

```rust
// Após .arg() chain completa, antes de retornar Command:
let argv_refs: Vec<OsString> = cmd.get_args().map(|s| s.to_os_string()).collect();
let preflight_args = PreFlightArgs {
    binary_path, argv: &argv_refs,
    workspace_root: Path::new("."),
    mcp_config_inline_json: None,
    expected_output_bytes: 65_536,
    spawner_name: "claude_runner",
};
if let Err(e) = preflight_check(&preflight_args) {
    tracing::error!(target: "claude_runner", error = %e, "preflight validation failed; aborting spawn (exit 16)");
    std::process::exit(16);
}
```

Para tokio::process::Command (caso `extract/llm_embedding.rs`), passamos
`argv: &[]` (skip argv-size) porque tokio não expõe `get_args()`.
Embedding prompts são bounded pelo schema validator então argv overflow
não é risco real.

## Métricas finais

- 200+ linhas em `src/spawn/preflight.rs`
- 15 testes unitários em `src/spawn/preflight.rs::tests`
- 8 variantes de erro estruturadas
- 4 spawners instrumentados
- 5 bugs-sintoma resolvidos
- 0 dependências novas (todas já em Cargo.toml)
- exit code 16 adicionado ao contrato
- ADR-0045 (este arquivo) + versão PT-BR pendente

## Cross-references

- `audit-a2-graph-2026-06-14` (score 92/100) auditou stdout/stderr/errors
  mas não cobriu argv — abriu GAP-META-005
- `v1-0-83-helper-env-whitelist-design` provou o pattern de helper
  compartilhado entre 3 spawners (estendido para 4 aqui)
- `claude-empty-config-dir-embedding-speedup` documentou que
  `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` acelera embedding 4x —
  pre-flight agora rejeita config dir não-vazio por padrão
- `feedback-signal-combined-severe` definiu estado 3/3 — ADR registra
  a justificativa arquitetural para o gate obrigatório

## Não-objetivos (YAGNI)

- NÃO alterar `VersionAdapter` (DEAD CODE desde v1.0.75, fora de escopo)
- NÃO refatorar 4 spawners para abstração única além do hook preflight
- NÃO adicionar suporte a outros backends LLM
- NÃO mudar `apply_env_whitelist`
- NÃO introduzir dependências novas
- NÃO adicionar counters em `health --json` (futuro, tracked como
  oportunidade de melhoria)

## Próximos passos

- v1.0.88: contadores preflight em `health --json` (G10 do plano)
- v1.0.89: decisão sobre `VersionAdapter` DEAD CODE (delete ou revive)
- v1.0.90: preflight strict mode (`SQLITE_GRAPHRAG_STRICT_PREFLIGHT=1`
  que adiciona guarda de comprimento mínimo de argv)
