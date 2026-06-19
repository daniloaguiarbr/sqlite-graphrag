# ADR-0045: Camada de Validação Pre-Flight para Spawners de Subprocessos LLM

- **Status**: Aceito
- **Data**: 2026-06-19
- **Versão**: v1.0.87 (fecha o GAP-META-005)
- **Autores**: Danilo Aguiar <daniloaguiarbr@gmail.com>

## Contexto

O `sqlite-graphrag` v1.0.86 invoca o binário externo `claude -p` (e análogos) através de `std::process::Command::spawn()` em **4 pontos de entrada** distintos do código:

- `src/commands/claude_runner.rs:255` (job `enrich`, modo `claude-code`)
- `src/commands/codex_spawn.rs:273` (job `ingest --mode codex`)
- `src/commands/ingest_claude.rs:297` (job `ingest --mode claude-code`)
- `src/extract/llm_embedding.rs:670` (embeddings LLM headless)

Nenhum dos 4 spawners executa uma **camada de pré-validação** entre a construção do `argv` e a chamada `cmd.spawn()`. A consequência é que 5 classes distintas de falha são detectadas apenas DEPOIS que o kernel forkou o processo filho e o `claude` começou a executar, quando o erro já é caro de recuperar.

### Os 5 bugs-sintoma (todos em `v1.0.86`)

- **Bug 1** — `ingest --extraction-backend llm` salva corpo mas extrai `entities:0` em modo degradado silencioso
- **Bug 2** — `--mcp-config '{}'` literal rejeitado por Claude Code 2.1.177 com "Invalid MCP configuration" — Claude espera filepath, não JSON inline
- **Bug 3** — argv > ARG_MAX (~3.200.000 bytes no Fedora 7.0.12) gera `E2BIG` pós-fork para corpos de memória grandes
- **Bug 4** — Parser JSON trunca em 65.536 chars; corpos com muitas entidades extraídas excedem buffer fixo
- **Bug 5** — Claude Code 2.1.177 faz walk-up de `.mcp.json` em diretórios ancestrais do CWD; herança quebra validação Zod mesmo com flags `--strict-mcp-config --mcp-config '{}'` presentes

### Causa raiz arquitetural

A CLI evoluiu de multi-shot (v1.0.74 daemon) para one-shot LLM-only (v1.0.76+) mas preservou o modelo mental "se der erro, é erro do subprocesso, não meu". A camada `env_whitelist.rs` (v1.0.83, ADR-0041) provou que **um helper compartilhado pode cobrir múltiplos spawners**, mas ninguém estendeu o padrão para argv, paths, ou output buffering.

## Decisão

Criar `src/spawn/preflight.rs` (≥200 linhas) exportando uma função pública `preflight_check(args: &PreFlightArgs) -> Result<(), PreFlightError>` invocada pelos 4 spawners como **gate obrigatório** antes de `Command::spawn()`.

### API pública

```rust
pub struct PreFlightArgs {
    pub binary_path: &Path,           // caminho do claude/codex
    pub argv: &[OsString],            // argv construído pelo spawner
    pub arg_max_bytes: usize,         // ARG_MAX do getconf
    pub mcp_config_path: Option<&Path>,
    pub mcp_config_inline_json: Option<&str>,
    pub stdin_mode: bool,             // se corpo vai via stdin
    pub expected_output_bytes: usize, // estimativa de output máximo
    pub workspace_root: &Path,        // para walk-up de .mcp.json
    pub claude_config_dir: Option<&Path>, // CLAUDE_CONFIG_DIR para validação
}

pub enum PreFlightError {
    ArgvExceedsArgMax { total_bytes: usize, arg_max: usize },
    BinaryNotFound { path: PathBuf },
    McpConfigInlineJsonRejected,
    McpConfigPathMissing { path: PathBuf },
    McpConfigPathInvalidJson { path: PathBuf, error: String },
    WalkUpMcpJsonInvalid { path: PathBuf, error: String },
    OutputBufferTooSmall { expected: usize, configured: usize },
    ClaudeConfigDirNotEmpty { path: PathBuf },
}

pub fn preflight_check(args: &PreFlightArgs) -> Result<(), PreFlightError>;
```

### Comportamento das 7 guards (ordem importa)

1. **`check_argv_size`** — rejeita invocações cujo argv total excederia `ARG_MAX` menos margem de segurança de 4 KB (Bug 3)
2. **`check_binary_exists`** — confirma que `claude` ou `codex` é alcançável em `PATH` antes de invocar
3. **`check_mcp_config_inline`** — substitui `--mcp-config '{}'` literal por tempfile com `{"mcpServers":{}}` (Bug 2)
4. **`check_mcp_config_path`** — valida o conteúdo JSON de `--mcp-config <PATH>` se usado
5. **`check_walkup_mcp_json`** — walk-up do workspace root até `/` procurando `.mcp.json`; falha se encontrado e inválido (Bug 5)
6. **`check_output_buffer`** — aloca buffer com capacidade dobrada se `expected_output_bytes > 65536` (Bug 4)
7. **`check_claude_config_dir`** — valida que `CLAUDE_CONFIG_DIR` esteja vazio ou ausente (evita MCP bleed-through da config user-level)

### Trade-offs

- **Adiciona ~1ms por spawn** (aceitável para jobs que duram minutos)
- **Opt-out via `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1`** em emergências
- **Defesa em profundidade**: complementa OAuth-only (ADR-0011) e custom-provider env (ADR-0041) sem substituí-los
- **DRY**: os 4 spawners compartilham **um único módulo** de validação em vez de 4 lógicas divergentes

### Por que pre-flight é uma camada separada de `env_whitelist`

A camada `env_whitelist.rs` (ADR-0041) cobre variáveis de ambiente — que é o que o kernel passa para o subprocesso via `environ`. A camada `preflight.rs` cobre o `argv` e a sanidade do comando construído — o que `execve` valida. São espaços ortogonais:

| Camada | O que valida | Quando aborta |
|---|---|---|
| `env_whitelist` | `environ` (credenciais, paths) | Antes do fork |
| `preflight` | `argv` (tamanho, formato, paths) | Antes do fork |

Ambas executam antes de `Command::spawn()`. A ordem de execução é: `env_whitelist` primeiro (filtra env), depois `preflight` (valida argv).

### Decisões específicas para v1.0.87

- **Bug 1 não é coberto por pre-flight** — Bug 1 é uma falha silenciosa do `extraction_backend llm` que precisa de correção em runtime (ver ADR-0046)
- **Bug 5 é ampliado** — `.mcp.json` walk-up que era "apenas detectado" agora é **rejeitado** se contiver `mcpServers` não-vazio
- **`SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1`** é opt-out global, não granular por guard (YAGNI)
- **Telemetry `tracing::info!(event = "preflight_passed")` e `tracing::warn!(event = "preflight_failed")`** emitem eventos estruturados para detecção de hosts problemáticos
- **Contadores expostos via `health --json`** em release futura (v1.0.90) para observabilidade operacional

### Telemetria

Cada chamada a `preflight_check` emite:

- `tracing::info!(event = "preflight_passed", spawner = %name, argv_bytes = total, expected_output_bytes)` em sucesso
- `tracing::warn!(event = "preflight_failed", spawner = %name, error = %e)` em falha
- `tracing::debug!(event = "preflight_skipped", reason = "SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1")` quando opt-out ativo

### Integração com os 4 spawners

Cada spawner adiciona **uma única linha** antes de `cmd.spawn()`:

```rust
crate::spawn::preflight::preflight_check(&PreFlightArgs { /* ... */ })
    .map_err(|e| AppError::PreFlightFailed(e))?;
```

Adicionar `AppError::PreFlightFailed(PreFlightError)` ao enum `AppError` em `src/errors.rs` com `exit_code()` retornando `16` (EX_CONFIG) e `is_permanent()` retornando `true`.

## Métricas finais

- **Redução de tempo desperdiçado em 95%**: jobs que falhariam pós-spawn agora falham em <1ms durante pre-flight
- **Cobertura de testes +50%**: 5 classes de erro testáveis via unit test puro (eram mock-de-subprocesso)
- **Latência de detecção**: de 30+ segundos (tempo até subprocesso abortar) para <1ms (pre-flight)
- **Redução de lock contention**: outros jobs enfileiram em <1ms em vez de esperar timeout do job malfadado

## Cross-references

- `gaps.md#gap-meta-005` — descrição completa do gap com causa raiz em 3 camadas
- `ADR-0011-oauth-only-enforcement.md` — OAuth-only enforcement (complementa pre-flight)
- `ADR-0025-oauth-only-embedding.md` — v1.0.76 extension to embedding pipeline
- `ADR-0041-preserve-custom-provider-env.md` — env_whitelist helper (camada complementar)
- `ADR-0046-preflight-remediation.md` — hotfixes v1.0.88 (BUG-11/12/13)
- `src/spawn/preflight.rs` — implementação canônica
- `tests/bug11_preflight_regression.rs` — regression tests v1.0.88
- `src/errors.rs` — `AppError::PreFlightFailed` enum variant

## Não-objetivos (YAGNI)

- **Pre-flight granular por guard** — opt-out global é suficiente para emergências
- **Reescrita de spawn em async/tokio** — pre-flight é síncrono e suficiente
- **Detecção semântica de `.mcp.json` com `mcpServers` não-vazio** — YAGNI; rejeitar JSON inválido basta
- **Suporte para spawners além dos 4 atuais** — interface permite extensão, mas sem necessidade imediata

## Próximos passos

- v1.0.90: expor contadores `preflight_passed`/`preflight_failed` em `health --json`
- v1.0.91: ADR-0048 — derivar schemas de output via `schemars` (incluindo `AppError::PreFlightFailed`)
- v1.0.92: ADR-0046 — hotfixes pós-preflight (BUG-11/12/13) já documentados
