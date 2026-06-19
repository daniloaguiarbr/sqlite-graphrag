# ADR-0046: Remediação do Preflight — Correções da Auditoria (v1.0.88)

- **Status**: Aceito
- **Data**: 2026-06-19
- **Versão**: v1.0.88 (fecha followup do GAP-META-005)
- **Autores**: Danilo Aguiar <daniloaguiarbr@gmail.com>

## Contexto

A ADR-0045 (v1.0.87) introduziu `src/spawn/preflight.rs` expondo `preflight_check` mais 7 guards (`check_argv_size`, `check_binary_exists`, `check_output_buffer`, `check_mcp_config_inline`, `check_mcp_config_path`, `check_walkup_mcp_json`, `check_claude_config_dir`) consumidos pelos 4 spawners LLM (`claude_runner.rs`, `codex_spawn.rs`, `ingest_claude.rs`, `llm_embedding.rs`).

A auditoria end-to-end pós-release (`audit-a2-graph-2026-06-18`) revelou **10 bugs latentes** na camada de preflight e nas tubulações adjacentes, variando de CRÍTICO (o sandbox dev foi quebrado em 100% por uma guarda de config-dir agressiva demais) a BAIXO (informação de variant perdida via `std::process::exit(16)` pulando o envelope de erro estruturado).

### Os 10 achados da auditoria

- **BUG-1 CRÍTICO** — `check_claude_config_dir` rejeitava QUALQUER diretório não-vazio, quebrando 100% das chamadas em dev. A correção: caminhar pelo diretório e inspecionar `settings.json` semanticamente em vez de verificar que o diretório está vazio. Um `~/.claude/` populado com `settings.json` declarando zero servidores MCP deve passar na guarda.

- **BUG-2 / BUG-3** — Spawners passavam a string literal `--mcp-config '{}'` para `Command::arg()`. Claude Code 2.1.177+ rejeitou o JSON inline com "Invalid MCP configuration". A correção: introduzir `write_empty_mcp_config_tempfile()` e substituir o literal em 3 sites de spawner.

- **BUG-4 BAIXO** — `check_mcp_config_inline` inspecionava apenas `--mcp-config <PATH>` mas não a forma com `=` `--mcp-config=PATH`. Usuários rodando com a forma `=` burlavam a validação de path.

- **BUG-5 MÉDIO** — `check_claude_config_dir` curto-circuitava na primeira entrada não-vazia sem inspecionar o conteúdo. A correção: carregar `settings.json` (se presente) e verificar que não há declarações de servidor MCP.

- **BUG-6 CRÍTICO** — `build_claude_command` retornava `Command` (assinatura infalível). Falha de preflight chamava `std::process::exit(16)`, matando a CLI sem emitir um envelope `AppError` estruturado. A correção: introduzir `From<PreFlightError> for AppError` e mudar `build_claude_command` para retornar `Result<Command, AppError>`.

- **BUG-7 ALTO** — `preflight_check` propagava `PreFlightError` diretamente aos chamadores, que não tinham como renderizar como JSON para consumidores `--json`. A correção: `preflight_check` propaga `AppError::PreFlightFailed` diretamente via a nova impl `From`.

- **BUG-9 BAIXO** — `check_walkup_mcp_json` aceitava qualquer arquivo JSON nomeado `.mcp.json`, mesmo se seu conteúdo fosse malformado. A correção: validação semântica de que o JSON parseia como o shape `{ "mcpServers": ... }`.

- **BUG-10 MÉDIO** — `AppError::PreFlightFailed` era previamente tipado como `String`, perdendo a variant estruturada `PreFlightError`. A correção: shape mudou para `Box<PreFlightError>` de modo que a variant original é preservada através da cadeia de erro.

- **BUG-11 CRÍTICO** — `src/embedder.rs` chamava `claude -p` sem invocar `preflight_check`, burlando todas as 7 guards. A correção: envolver o call site com `preflight_check` antes de `Command::spawn()`.

- **BUG-12 MÉDIO** — `src/output.rs:141` (`output::emit_error`) chamava TANTO `tracing::error!` QUANTO `eprintln!` para a mesma violação, produzindo 2 linhas stderr por trip de OAuth-only enforcement.

## Decisão

Remediação consolidada na v1.0.88:

1. **Correção BUG-1** — `check_claude_config_dir` agora inspeciona `settings.json` semanticamente. Um diretório contendo apenas `settings.json` (com zero declarações de servidor MCP) é aceito. A heurística de diretório-deve-ser-vazio é removida.

2. **Correção BUG-2/3** — `write_empty_mcp_config_tempfile()` escreve `{"mcpServers":{}}` em um tempfile via `tempfile::persist()` e retorna o path. Todos os 3 sites de spawner (`claude_runner.rs`, `ingest_claude.rs`, `llm_embedding.rs`) substituem o literal `'{}'` pelo path do tempfile.

3. **Correção BUG-4** — `check_mcp_config_inline` agora parseia `--mcp-config=...` (token único, forma `=`) e `--mcp-config <PATH>` (dois tokens) simetricamente. Ambas as formas roteiam para `check_mcp_config_path` quando o valor é um path não-vazio.

4. **Correção BUG-5** — `check_claude_config_dir` caminha pelo diretório; para cada arquivo `*.json` presente, tenta `serde_json::from_str::<Settings>()` e verifica que `mcp_servers` é vazio ou ausente.

5. **Correção BUG-6** — `From<PreFlightError> for AppError` adicionado. `build_claude_command` retorna `Result<Command, AppError>`. Chamadores recebem `AppError::PreFlightFailed(_)` e renderizam o envelope estruturado via `output::emit_error`.

6. **Correção BUG-7** — `preflight_check` mapeia internamente `PreFlightError` para `AppError::PreFlightFailed` via a nova impl `From`. Chamadores veem apenas variants `AppError`.

7. **Correção BUG-9** — `check_walkup_mcp_json` agora parseia `.mcp.json` e verifica que o schema é `{ "mcpServers": { ... } }`. Arquivos com shape malformado são rejeitados com `WalkUpMcpJsonInvalid`.

8. **Correção BUG-10** — `AppError::PreFlightFailed` agora é `Box<PreFlightError>` (o próprio shape da variant). Payload lossy `String` removido.

9. **Correção BUG-11** — `src/embedder.rs` agora chama `preflight_check` antes de `Command::spawn()` no pipeline de embedding LLM. A invocação da guarda é idêntica aos outros 3 spawners.

10. **Correção BUG-12** — `src/output.rs:141` (`output::emit_error`) descarta o `eprintln!` redundante e mantém apenas `tracing::error!`. Stderr agora emite exatamente 1 linha por erro.

### Migração de chamadores (3 sites de spawner)

| Site | Antes | Depois |
|------|-------|--------|
| `claude_runner.rs:255` | `let cmd = build_claude_command(...); preflight_check(...).unwrap_or_else(\|e\| std::process::exit(16)); cmd.spawn()` | `let cmd = build_claude_command(...)?; preflight_check(...)?; cmd.spawn()` |
| `ingest_claude.rs:297` | `cmd.arg("--mcp-config").arg("{}")` | `let path = write_empty_mcp_config_tempfile()?; cmd.arg("--mcp-config").arg(path)` |
| `llm_embedding.rs:670` | bypassava preflight inteiramente | `preflight_check(...)?; cmd.spawn()` |

## Consequências

### Positivas

- 9 testes de integração restaurados (previamente mascarados pela quebra do sandbox dev): `entity_validation`, `graph_traverse`, `recall_distance`, e 6 outros
- 0 regressões em `cargo test --lib` (833+ passou)
- 5 sites usando `std::process::exit(16)` removidos — todos substituídos por propagação `?`
- 3 sites de spawner usam `write_empty_mcp_config_tempfile()` em vez de JSON inline
- `AppError::PreFlightFailed(Box<PreFlightError>)` é totalmente estruturado end-to-end
- stderr emite 1 linha por violação OAuth-only enforcement (era 2)

### Negativas

- Assinatura de `build_claude_command` mudou de `-> Command` para `-> Result<Command, AppError>`. Todos os 4 call sites atualizados.
- Mudança de shape de `AppError::PreFlightFailed` é breaking change para consumidores downstream que parseiam a variant. Mitigação: a variant retém a mesma impl `Display`, apenas o tipo de payload interno mudou.
- 1 tempfile novo por spawn no caso de MCP-config-inline. Aceitável para jobs que já levam segundos.

## Débito Técnico Conhecido (v1.0.89+)

- `src/commands/enrich.rs` tem 4116 linhas. Modularização em `enrich/queue.rs`, `enrich/extraction.rs`, `enrich/postprocess.rs` está planejada para v1.0.89.
- `AppError::Embedding(String)` é stringly-typed. Subtipagem em `EmbeddingBackendUnavailable`, `EmbeddingTimeout`, `EmbeddingResponseMalformed` está planejada via ADR-0048 (proposta em v1.0.89).
- `tests/integration.rs` tem 2367 linhas. Divisão em suites específicas para embeddings, graph traversal, recall e CRUD está planejada para v1.0.89.
- `preflight_check` retorna unit; trabalho futuro pode retornar um relatório estruturado (timings por guarda) para contadores em `health --json`.

## Cross-references

- ADR-0045 (camada de validação preflight original, v1.0.87)
- ADR-0011 (OAuth-only enforcement — correção BUG-12 visa deduplicação de stderr)
- ADR-0040 (cadeia de fallback de captura de stderr — BUG-12 é ortogonal mas adjacente)
- ADR-0041 (preservar env de provider customizado — preflight não deve limpar `CLAUDE_CONFIG_DIR` para providers legítimos)
- ADR-0042 (split do backend claude — 3 dos sites de spawner vivem em módulos divididos por esta ADR)
- `audit-a2-graph-2026-06-18` (a auditoria que surfacejou BUG-1..12)
- `gaps.md` (GAP-META-005 fechado via esta remediação)
- `src/spawn/preflight.rs:1` (o helper consumido pelos 4 spawners)
- `src/error.rs` (`AppError::PreFlightFailed(Box<PreFlightError>)`)

## Não-objetivos (YAGNI)

- NÃO refatorar os 4 spawners em uma abstração única além do hook de preflight
- NÃO introduzir preflight assíncrono (custo síncrono de 1ms é aceitável)
- NÃO mudar semântica de `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1`
- NÃO adicionar exit codes novos (16 permanece o único exit de preflight, mas agora atingido via renderização de `AppError`)

## Próximos passos

- v1.0.89: modularizar `src/commands/enrich.rs` (4116 linhas)
- v1.0.89: ADR-0048 (subtipagem de EmbeddingErrorKind)
- v1.0.89: dividir `tests/integration.rs` (2367 linhas) em suites focadas
- v1.0.90: contadores de preflight em `health --json`
