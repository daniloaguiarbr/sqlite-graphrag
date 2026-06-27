# sqlite-graphrag

[![Crates.io](https://img.shields.io/crates/v/sqlite-graphrag.svg)](https://crates.io/crates/sqlite-graphrag)
[![Docs.rs](https://docs.rs/sqlite-graphrag/badge.svg)](https://docs.rs/sqlite-graphrag)
[![CI](https://github.com/daniloaguiarbr/sqlite-graphrag/actions/workflows/ci.yml/badge.svg)](https://github.com/daniloaguiarbr/sqlite-graphrag/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](LICENSE)
[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg)](CODE_OF_CONDUCT.md)

> Memória persistente para agentes de IA em um único binário Rust com GraphRAG embutido.
> **Release atual: v1.0.95 — `enrich --mode openrouter` roteia o JUDGE de extração pela API REST de chat do OpenRouter, então a extração estruturada não exige mais uma CLI local claude/codex/opencode.** Todo build embute via `claude -p`, `codex exec`, `opencode run` (OAuth) ou API REST OpenRouter (`--embedding-backend openrouter`). Sem daemon, sem runtime ONNX, binário de ~14.6 MiB. A v1.0.94 adiciona `--embedding-backend auto|openrouter|llm` com `--embedding-model` para embeddings via API REST (~200ms vs 15s subprocess LLM), propaga `EmbeddingBackendChoice` para TODOS os 13 caminhos de embedding (GAP-OR-PROPAGATION), corrige exit code 78 para erros de configuração OpenRouter (BUG-OR-EXIT-CODE) e valida 10 modelos de embedding E2E. O backend de embedding OpenRouter anterior (`--embedding-backend openrouter`) permanece inalterado. Consumidores da biblioteca devem fixar em `=1.0.95`; veja a `Política de Estabilidade` abaixo.

- Leia este documento em [inglês (EN)](README.md).

- Versão em inglês disponível em [README.md](README.md)
- O pacote público e o repositório já estão disponíveis no GitHub e no crates.io
- Instale a última release publicada com `cargo install sqlite-graphrag --locked`
- Atualize uma instalação existente com `cargo install sqlite-graphrag --locked --force`
- Verifique o binário ativo com `sqlite-graphrag --version`
- Veja o histórico completo de releases em [CHANGELOG.pt-BR.md](CHANGELOG.pt-BR.md)
- A validação de release inclui as suítes de contrato `slow-tests` documentadas em `docs/TESTING.pt-BR.md`
- Faça o build direto do checkout local com `cargo install --path .`
- **Atualizando de v1.0.74 / v1.0.75?** Veja [docs/MIGRATION.pt-BR.md](docs/MIGRATION.pt-BR.md) para o procedimento de migração da v1.0.76
- **Atualizando de v1.0.79 para v1.0.80?** Nenhuma migração de banco necessária; basta `cargo install sqlite-graphrag --locked --force`. A v1.0.80 adiciona o job de CI `semver-checks` (informativo), os steps de pre-warm do Windows (ADR-0033) e a saída sem panic no terceiro sinal (ADR-0034). Consumidores da biblioteca devem fixar em `=1.0.80`; veja a `Política de Estabilidade` abaixo. / v1.0.77 / v1.0.78 / v1.0.79
- **Atualizando de v1.0.80 / v1.0.81 para v1.0.82?** Duas novas migrations rodam automaticamente no primeiro `init`/`migrate`: `V014__pending_memories` (fila de checkpoint do `remember`) e `V015__pending_embeddings` (fila de retry de embedding). Após atualizar, rode `codex login` uma vez para refrescar o refresh token OAuth — o incidente de 2026-06-14 mostrou que `codex exec` retornando HTTP 401 `refresh_token_reused` agora é capturado pela nova cadeia de fallback (ADR-0040) e roteado para o próximo backend em `--llm-backend codex,claude`. Veja [docs/MIGRATION.pt-BR.md](docs/MIGRATION.pt-BR.md) para o procedimento completo em 6 passos incluindo rollback.
- **Atualizando de v1.0.91 / v1.0.92 para v1.0.94?** Nenhuma migração de banco necessária; basta `cargo install sqlite-graphrag --locked --force`. A v1.0.94 adiciona o backend de embedding OpenRouter (`--embedding-backend openrouter`), propaga `EmbeddingBackendChoice` para todos os 13 caminhos de embedding (GAP-OR-PROPAGATION), corrige exit code 78 para erros de configuração OpenRouter (BUG-OR-EXIT-CODE) e valida 10 modelos de embedding E2E. Consumidores da biblioteca devem fixar em `=1.0.94`.
- **Atualizando de v1.0.94 para v1.0.95?** Nenhuma migração de banco necessária; o schema permanece em v15 — basta `cargo install sqlite-graphrag --locked --force`. A v1.0.95 adiciona `enrich --mode openrouter`, roteando o JUDGE de extração pelo endpoint REST `/chat/completions` do OpenRouter para que a extração estruturada (memory-bindings, entity-descriptions, body-enrich, etc.) não exija mais uma CLI local claude/codex/opencode. Novas flags: `--openrouter-model` (obrigatória com `--mode openrouter`; sem default — sua ausência sai com exit 1 antes de qualquer chamada de rede), `--openrouter-api-key` (env `OPENROUTER_API_KEY`), `--openrouter-timeout` (padrão 300s) e `--openrouter-base-url`. O pipeline SCAN→JUDGE→PERSIST permanece inalterado; só o transporte do JUDGE muda (ADR-0054). Consumidores da biblioteca devem fixar em `=1.0.95`.
- **Atualizando de v1.0.85 / v1.0.86 / v1.0.87 / v1.0.88 / v1.0.89 / v1.0.90 para v1.0.91?** Nenhuma migração de banco necessária; basta `cargo install sqlite-graphrag --locked --force`. A v1.0.91 corrige GAP-SPAWN-001 (subprocessos LLM não herdam mais `.mcp.json` — embedding funciona zero-config em qualquer projeto), BUG-17 (inflação de `entities.degree` substituída por `recalculate_degree`), BUG-15 (7 enums de schema), BUG-16 (schema `deep-research`), GAP-SPAWN-002 (cleanup de diretórios órfãos) e BUG-14 (correção de teste). Consumidores da biblioteca devem fixar em `=1.0.91`.
- **Atualizando de v1.0.82 / v1.0.83 para v1.0.85?** Nenhuma migração de banco necessária; basta `cargo install sqlite-graphrag --locked --force`. A v1.0.84 (ADR-0042, GAP-002) adicionou o split real do backend Claude via `LlmEmbeddingBuilder` para que `--llm-backend claude` invoque `claude` e nunca `codex`, o campo `backend_invoked` em 7 envelopes JSON, o campo `vec_degraded_reason` em `hybrid-search` e `recall`, a flag global `--dry-run-backend` para auditoria pré-voo em CI, e `apply_env_whitelist_for_claude` para providers hardened. A v1.0.85 (ADR-0043) estendeu `FallbackReason` de 3 para 7 variantes com discriminador `reason_code` (captura exaustão de quota, exaustão de slot, mismatch de backend, dim zero, cancelamento, timeout), `try_embed_query_with_deterministic_fallback` re-tenta o backend alternativo em `OAuthQuota` e dorme 750ms em `SlotExhausted`, e `LlmEmbedding::invoke_claude` agora captura 12-14 headers `anthropic-ratelimit-*-remaining` ANTES de checar o exit do subprocesso (G45-CR5). Consumidores da biblioteca devem fixar em `=1.0.85`; veja a `Política de Estabilidade` abaixo.

```bash
cargo install sqlite-graphrag --locked --force
sqlite-graphrag --version
```


## O que é?
### sqlite-graphrag entrega memória durável para agentes de IA
- Armazena memórias, entidades e relacionamentos em um único arquivo SQLite abaixo de 25 MB
- **Build (v1.0.94):** LLM-only e one-shot — embeddings são gerados ao spawnar `claude -p`, `codex exec`, `opencode run` com OAuth, ou via API REST OpenRouter (`--embedding-backend openrouter`); sem modelo local, sem daemon, sem runtime ONNX, binário de ~14.6 MiB. Subprocessos LLM rodam em diretório temporário isolado (GAP-SPAWN-001) para que `.mcp.json` do projeto do chamador nunca seja herdado. Desde a v1.0.95, `enrich --mode openrouter` pode rodar o JUDGE de extração inteiramente pela API REST de chat do OpenRouter — sem necessidade de CLI local claude/codex/opencode (ADR-0054)
- **Build legado:** REMOVIDO na v1.0.79 — a feature `embedding-legacy` e o caminho local fastembed/ONNX não existem mais
- Combina busca full-text FTS5 com similaridade de cosseno em Rust puro em um ranqueador híbrido de Reciprocal Rank Fusion
- Armazena e atravessa um grafo explícito de entidades com arestas tipadas para recall multi-hop entre memórias
- Preserva cada edição através de uma tabela imutável de histórico de versões para auditoria completa
- Roda em Linux, macOS e Windows nativamente sem serviços externos (o build padrão precisa de `claude`, `codex` ou `opencode` CLI no `PATH`)


## Por que sqlite-graphrag?
### Diferenciais contra stacks RAG em nuvem
- **Fluxo LLM OAuth-only** — sem chaves de API no ambiente; o spawn ABORTA se `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estiverem definidas (defesa em profundidade desde v1.0.69)
- **Providers Anthropic-compatible customizados (v1.0.83+)** — preserva `ANTHROPIC_AUTH_TOKEN` e `ANTHROPIC_BASE_URL` para que o Claude Code possa rotear para MiniMax, OpenRouter ou gateways corporativos sem violar o mandato OAuth-only. Defina `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` (ou `--strict-env-clear`) para ambientes de compliance que proíbem encaminhamento de credenciais.
- **Sem custos recorrentes de embedding** — embeddings vêm da assinatura Claude Pro / Max ou ChatGPT Pro existente
- Armazenamento em arquivo SQLite único substitui clusters Docker de bancos vetoriais
- Recuperação com grafo supera RAG vetorial puro em perguntas multi-hop por design
- Saída JSON determinística habilita orquestração limpa por agentes de IA em pipelines
- Binário cross-platform nativo dispensa dependências Python, Node ou Docker (o build padrão precisa apenas de `claude`, `codex` ou `opencode` CLI)


## Política de Estabilidade (G53, v1.0.80)

- O **contrato público é a CLI**. Os envelopes `--json` documentados em `docs/schemas/*.schema.json` e as variáveis de ambiente listadas em `llms.txt` e `llms-full.txt` permanecem estáveis em todas as versões v1.x.y. Consumidores que dependem apenas da CLI não são afetados por bumps minor ou patch.
- A **API da biblioteca é instável** em v1.x.y. Re-exports, campos públicos de struct e assinaturas de função podem mudar em qualquer release v1.x.y sem bump de major.
- Mudanças quebrantes na API da biblioteca saem como bump **minor**, nunca patch (ex.: 1.0.79 -> 1.1.0 para re-export removido). Bumps de patch (1.0.79 -> 1.0.80) são limitados a mudanças aditivas sem quebra.
- Consumidores que dependem da API da biblioteca devem fixar versão exata (`sqlite-graphrag = "=1.0.80"`) e revisar CHANGELOG.md antes de bumpar.
- Esta postura está registrada em `docs/decisions/adr-0032-g53-lib-api-policy.md`.

## Superpoderes para Agentes de IA
### Contrato de CLI de primeira classe para orquestração
- Todo subcomando aceita `--json` produzindo payloads determinísticos em stdout
- **v1.0.76 é one-shot por padrão** — sem processo em segundo plano; cada chamada de embedding spawna um novo `claude -p`, `codex exec` ou `opencode run`
- Toda escrita é idempotente via restrições de unicidade em `--name` kebab-case
- Stdin é explícito: use `--body-stdin` para texto ou `--graph-stdin` para um objeto `{body?, entities, relationships}`; arrays crus de entidades e relacionamentos usam `--entities-file` e `--relationships-file`
- `remember` aceita payloads de body até `512000` bytes e até `512` chunks
- Payloads de relacionamento usam `strength` em `[0.0, 1.0]`, mapeado para `weight` nas saídas
- Stderr carrega saída de tracing apenas sob `SQLITE_GRAPHRAG_LOG_LEVEL=debug`
- `--help` é inglês por padrão; use `--lang` para mensagens humanas de runtime, não para o help estático do clap
- Comportamento cross-platform é idêntico em hosts Linux, macOS e Windows


## Schema do Grafo
### Tipos de entidade, rótulos de relação e peso de aresta
- `entity_type` aceita exatamente 13 valores: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- `relation` (entrada CLI) aceita qualquer string em kebab-case ou snake_case. 12 valores canônicos são bem conhecidos: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`. Valores customizados (ex.: `implements`, `tested-by`, `blocks`) são aceitos com um `tracing::warn!`. A saída JSON normaliza para underscores (ex.: `applies_to`).
- `strength` é um float em `[0.0, 1.0]` representando o peso da aresta; mapeado para `weight` em todos os outputs de leitura
- Valores de `entity_type` não listados são rejeitados na escrita com código de saída 1. Valores customizados de `relation` são aceitos desde v1.0.49.
- Use `sqlite-graphrag graph --format json` para inspecionar o grafo completo armazenado a qualquer momento


### 27 agentes de IA e IDEs suportados de imediato
| Agente | Fornecedor | Versão mínima | Padrão de integração |
| --- | --- | --- | --- |
| Claude Code | Anthropic | 1.0 | Subprocesso com stdout `--json` |
| Codex | OpenAI | 1.0 | Tool call envolvendo `cargo run -- recall` |
| Gemini CLI | Google | 1.0 | Function call retornando JSON |
| Opencode | Opencode | 1.0 | Shell tool com `hybrid-search --json` |
| OpenClaw | Comunidade | 0.1 | Subprocesso via pipe para filtros `jaq` |
| Paperclip | Comunidade | 0.1 | Invocação direta da CLI por mensagem |
| VS Code Copilot | Microsoft | 1.85 | Subprocesso de terminal via tasks |
| Google Antigravity | Google | 1.0 | Agent tool com JSON estruturado |
| Windsurf | Codeium | 1.0 | Registro de comando customizado |
| Cursor | Anysphere | 0.42 | Integração terminal ou wrapper MCP |
| Zed | Zed Industries | 0.160 | Extensão envolvendo subprocesso |
| Aider | Paul Gauthier | 0.60 | Hook de shell por turno |
| Jules | Google Labs | 1.0 | Integração de shell no workspace |
| Kilo Code | Comunidade | 1.0 | Invocação via subprocesso |
| Roo Code | Comunidade | 1.0 | Comando customizado via CLI |
| Cline | Saoud Rizwan | 3.0 | Ferramenta de terminal registrada manualmente |
| Continue | Continue Dev | 0.9 | Provedor de contexto via shell |
| Factory | Factory AI | 1.0 | Tool call com resposta JSON |
| Augment Code | Augment | 1.0 | Envolvimento de comando de terminal |
| JetBrains AI Assistant | JetBrains | 2024.3 | External tool por IDE |
| OpenRouter | OpenRouter | 1.0 | Roteamento de função via shell |
| Minimax | Minimax | 1.0 | Invocação via subprocesso |
| Z.ai | Z.ai | 1.0 | Invocação via subprocesso |
| Ollama | Ollama | 0.1 | Invocação via subprocesso |
| Hermes Agent | Comunidade | 1.0 | Invocação via subprocesso |
| LangChain | LangChain | 0.3 | Subprocesso via tool |
| LangGraph | LangChain | 0.2 | Subprocesso via nó |


## Início Rápido
### Instale e grave sua primeira memória em quatro comandos
```bash
cargo install sqlite-graphrag --locked --force
sqlite-graphrag init
sqlite-graphrag remember --name primeira-memoria --type user --description "primeira memória" --body "olá graphrag"
sqlite-graphrag recall "graphrag" --k 5 --json
```
> **Flags obrigatórias para `remember`:** `--name`, `--type`, `--description`. Body via `--body "texto"`, `--body-file <caminho>`, ou `--body-stdin` (pipe do stdin).
> **Limite do body: 500 KB (512000 bytes).** Entradas maiores são rejeitadas com código de saída 6 (`limit exceeded`); divida em múltiplas memórias ou reduza antes de enviar.
> **Usuários Windows (G29):** v1.0.68 é o primeiro release desde v1.0.65 que compila com sucesso via `cargo install` no Windows. Se você precisa ficar em v1.0.66 ou v1.0.67, veja [docs/CROSS_PLATFORM.pt-BR.md](./docs/CROSS_PLATFORM.pt-BR.md) para a solução manual.
- **GraphRAG está habilitado por padrão e roda automaticamente.** Cada subcomando auto-inicializa `graphrag.sqlite` no diretório de trabalho atual se ele não existir. A extração de entidades/relacionamentos vem do backend LLM (`--extraction-backend llm`, o padrão) ou de grafo curado (`--graph-stdin`, `--entities-file`).

### Extração automática (`--enable-ner`)
- Passe `--enable-ner` ou defina `SQLITE_GRAPHRAG_ENABLE_NER=1` para ativar extração automática em `remember` e `ingest`
- Desde a v1.0.79 isso executa APENAS extração de URL por regex — o pipeline local GLiNER zero-shot foi removido junto com a feature `ner-legacy`
- `--gliner-variant`, `SQLITE_GRAPHRAG_GLINER_MODEL` e `SQLITE_GRAPHRAG_GLINER_THRESHOLD` continuam aceitas por compatibilidade mas NÃO têm efeito
- Campo `extraction_method` na resposta reporta `url-regex`, `regex-only` ou `none:extraction-failed`
- Para extração de alta qualidade prefira `ingest --mode claude-code`/`--mode codex` (curada por LLM) ou passe entidades curadas via `--graph-stdin`
- `--skip-extraction` está obsoleto desde v1.0.45 e não tem efeito

- **`sqlite-graphrag init` é OPCIONAL** mas recomendado no primeiro uso porque cria o banco, aplica migrações e valida que uma CLI `claude`, `codex` ou `opencode` está alcançável no `PATH` (não há download de modelo desde a v1.0.76 — os embeddings vêm do subprocesso LLM).
- **`graphrag.sqlite` é criado no diretório de trabalho atual por padrão** (sobrescreva com `--db <caminho>` ou `SQLITE_GRAPHRAG_DB_PATH`)
- Para o checkout local, `cargo install --path .` é suficiente
- Reexecute `sqlite-graphrag --version` após qualquer upgrade para confirmar o binário ativo
- Depois da release pública, prefira `--locked` para preservar o grafo de dependências validado para o MSRV


## Destaques da Versão
- **v1.0.96**: Dead-letter no enrich + concorrência REST OpenRouter (GAP-ENRICH-BACKLOG-CONVERGE, GAP-OPENROUTER-REST-CONCURRENCY, ADR-0055) — a fila do enrich (`.enrich-queue.sqlite`) ganha um status terminal `dead` mais colunas `error_class`/`next_retry_at` (`ALTER TABLE` idempotente) e um índice `idx_enrich_queue_eligible` para que o backlog vivo seja estritamente decrescente e convirja; a classificação reutiliza `AttemptOutcome` + `compute_delay` de `src/retry.rs` (Transient rate-limit/timeout/5xx → `next_retry_at` com backoff exponencial, HardFailure validação/parse → terminal imediato), um item vira `dead` após `--max-attempts` retries Transient (padrão 5, faixa 1..=20) ou na 1ª HardFailure, e o dequeue respeita `next_retry_at` excluindo `dead`; novas flags `--until-empty` (loop interno scan→drain que substitui o loop bash externo), `--max-runtime <SECONDS>` (teto wall-clock para `--until-empty`, padrão 3600), `--max-attempts <N>`, `--status` (contagens JSON read-only — unbound_backlog, queue pending/done/failed/dead/skipped, eligible_now, waiting — sem chamada LLM, sem singleton) e `--rest-concurrency <N>` (fan-out REST para `--mode openrouter`, clamp 1..=16, padrão 8, distinta de `--llm-parallelism`); `embed_passages_parallel_with_embedding_choice` (`src/embedder.rs`) faz fan-out das chamadas REST OpenRouter por lote de 32 chunks via `tokio::task::JoinSet` bounded (in-flight clamp 1..16, Cloudflare-safe, sem dependência nova) com ordem preservada por índice de chunk, enquanto as escritas SQLite permanecem serializadas via WAL + claim atômico (single-writer intacto); prova de ordem (teste vivo): cosseno diagonal 0.9999, off-diagonal máx 0.899, argmax 64/64; nextest 1086 passed, 0 failed, 6 skipped; sem migração de schema (v15)
- **v1.0.95**: Enrichment via chat OpenRouter (GAP-OR-ENRICH, ADR-0054) — `enrich --mode openrouter` roteia o JUDGE de extração pelo endpoint REST `/chat/completions` do OpenRouter, então a extração estruturada (memory-bindings, entity-descriptions, body-enrich, etc.) não exige mais uma CLI local claude/codex/opencode; novo `src/chat_api.rs` (`OpenRouterChatClient`) espelha a política de retry/backoff de `src/embedding_api.rs` (aborta em 401/400/404, honra `retry-after` em 429, backoff exponencial + jitter em 5xx, apenas header Authorization: Bearer); novas flags `--openrouter-model` (obrigatória, sem default — ausência sai com exit 1 antes de qualquer chamada de rede), `--openrouter-api-key` (env `OPENROUTER_API_KEY`), `--openrouter-timeout` (padrão 300s), `--openrouter-base-url`; Structured Outputs via `response_format` json_schema `strict:true` + `provider.require_parameters:true`; `reasoning.enabled:false` com fallback gracioso reasoning-mandatory (re-tenta 1x omitindo reasoning); 13/13 modelos OpenRouter verificados (9 diretos, 4 via fallback); `usage.cost` lido da resposta; `OPENROUTER_API_KEY` mantida em `secrecy`, zeroizada no drop, nunca logada, nunca passada a subprocesso; pipeline SCAN→JUDGE→PERSIST inalterado; sem migração de schema (v15)
- **v1.0.94**: Backend de embedding OpenRouter (GAP-OR-INGEST) — `--embedding-backend auto|openrouter|llm` com `--embedding-model` para embeddings via API REST (~200ms vs 15s subprocess LLM); `EmbeddingBackendChoice` propagado para TODOS os 13 caminhos de embedding incluindo enrich, init, rename-entity, ingest_claude e remember chunks (GAP-OR-PROPAGATION); exit code 78 para erros de configuração OpenRouter (BUG-OR-EXIT-CODE); flag `--enrich-after` para ingest; 10 modelos verificados E2E (Qwen, OpenAI, Google Gemini, NVIDIA, Mistral, BAAI, Perplexity); 5 correções BUG-OR; 1059 testes, 0 falhas
- `v1.0.92`: Remediação de 8 gaps de documentação, auditoria de skills, expansão CRUD
- `v1.0.91`: Isolamento de CWD de spawn (GAP-SPAWN-001) — subprocessos LLM rodam em diretório temporário isolado; correção de inflação de `entities.degree` (BUG-17) via `recalculate_degree`; 7 correções de enum em JSON schemas (BUG-15); correção do schema `deep-research` (BUG-16); limpeza de diretórios de spawn órfãos (GAP-SPAWN-002); 877+ testes, 0 falhas
- `v1.0.90`: Integração do backend OpenCode (GAP-OPENCODE-001/002) — terceiro backend LLM junto com codex e claude; `--llm-backend opencode`, `--mode opencode` para ingest/enrich; cadeia de fallback estendida para `codex → claude → opencode → none`; 24 correções de bugs/gaps; 875+ testes, 0 falhas
- **v1.0.85**: Remediação dos cinco gaps (ADR-0043) — `FallbackReason` estendido de 3 para 7 variantes (`EmbeddingFailed | SlotExhausted | OAuthQuota { backend } | BackendMismatch { requested, resolved } | DimZero | Cancelled | Timeout`) com discriminador `reason_code` em envelopes `hybrid-search` e `recall` para diagnóstico granular; `try_embed_query_with_deterministic_fallback` re-tenta o backend alternativo (codex ↔ claude) em `OAuthQuota` e dorme 750ms em `SlotExhausted` antes de ceder para FTS5-puro; `LlmEmbedding::invoke_claude` captura 12-14 headers `anthropic-ratelimit-*-remaining` ANTES de checar o exit do subprocesso (G45-CR5 — exaustão de quota aborta o embed e dispara fallback imediato); `.github/workflows/embedder-ignore.yml` roda testes `#[ignore]` em env hermético (sem API keys); 5 novos testes de regressão em `tests/embedder.rs` cobrindo GAP-003, G58, G45-CR5, G55, G56
- **v1.0.84**: Split real do backend Claude para GAP-002 (ADR-0042) — `--llm-backend claude` não delega mais para `codex` via `LlmEmbedding::detect_available`; novo entry point `embed_via_claude_local` e `LlmEmbeddingBuilder` com `with_claude_builder`/`with_codex_builder`/`override_binary`/`override_model`; campo `backend_invoked` em 7 envelopes JSON (`embedding status`, `remember`, `edit`, `ingest`, `recall`, `hybrid-search`, `enrich`); campo `vec_degraded_reason` em `hybrid-search` e `recall`; flag global `--dry-run-backend` (ADR-0042 S6) resolve e imprime o backend sem spawnar subprocesso; helper `apply_env_whitelist_for_claude` para providers hardened; `LlmBackendKind::as_str` e `FallbackReason::reason_code` para serialização canônica em envelopes; 5 novos testes de regressão em `tests/embedder.rs`
- **v1.0.83**: Providers Anthropic-compatíveis customizados (ADR-0041) — `claude_runner`, `codex_spawn` e `ingest_claude` preservam `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY` e `OTEL_EXPORTER_OTLP_ENDPOINT` no ambiente do subprocesso; habilita providers Anthropic-compatíveis (MiniMax/api.minimax.io, OpenRouter, gateways corporativos) sem quebrar o mandato OAuth-only; nova flag global `--strict-env-clear` (`SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1`) para ambientes de compliance que proíbem encaminhamento de credenciais; novo módulo helper `src/spawn/env_whitelist.rs` consolidando a lógica de whitelist duplicada entre três spawners; 5 novos testes de integração em `tests/claude_runner_env.rs` cobrindo propagação de provider customizado, abort OAuth-only, herança de base-url pelo codex, queda de credenciais em modo estrito e auditoria de ausência de leak de token

- **v1.0.79**: G42 fechado — o pipeline de embedding LLM deixou de ser lento, serializado e frágil. **(S1)** dimensionalidade de embedding configurável, padrão 64 (`--embedding-dim`, `SQLITE_GRAPHRAG_EMBEDDING_DIM`, faixa [8, 4096]; precedência flag > env > `schema_meta.dim` > 64; bancos 384-dim existentes continuam funcionando sem mudança, ZERO alteração de schema). **(S2)** chamadas LLM em lote (schema `{items:[{i,v}]}` — chunks de 8, nomes de entidade de 25 em dim 64, adaptativos via clamp(base×64/dim, 1, base) desde o G44; 39 spawns viram 4-5). **(S3)** paralelismo real limitado via `Semaphore` + `JoinSet` com a nova flag `--llm-parallelism` em `remember` (padrão 4), `ingest` (padrão 2) e `edit`; resultados fluem por canal mpsc limitado. **(S4)** tempfiles de schema do codex são `NamedTempFile` RAII; o reaper também remove diretórios `codex-home-{pid}` obsoletos. **(S5)** override de modelo via env `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL`. **(S6)** `CLAUDE_CONFIG_DIR` vazio por padrão no caminho de embedding (~40-50s → ~10-15s por chamada). **(S7)** erro acionável no codex headless. **(S8)** handler de sinais sem panic (segundo sinal sai com 130 e ZERO I/O). **(S9)** re-embed canônico: `enrich --operation re-embed` mais `edit --force-reembed`. **(C5)** `validate_dim` falha em vetores divergentes em vez de normalizar silenciosamente. Todo subprocesso LLM usa `kill_on_drop` mais `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` (padrão 300s). Também REMOVIDOS: a infraestrutura do daemon e as features legadas `embedding-legacy`/`ner-legacy`/`full` com as dependências opcionais fastembed/ort/ndarray/tokenizers/hf-hub — todo build é LLM-only.
- **v1.0.78**: Correção G41 — `migrate --rehash` não insere mais linhas fantasma para migrações não aplicadas (a V013 era registrada sem executar o SQL)
- **v1.0.77**: Correção G40 — o INSERT do `run_rehash` agora grava `applied_on` (RFC3339); um NULL ali bloqueava todas as migrações seguintes
- **v1.0.76**: **Mudança arquitetural quebrante** — o build padrão vira LLM-only e one-shot: sem daemon, sem runtime ONNX, sem download de modelo local; embeddings/NER delegam para `claude -p` ou `codex exec` headless (OAuth). A migração V013 dropa as virtual tables `vec_*` em favor de tabelas de embedding BLOB com cosseno em Rust puro. Novos caminhos de upgrade `migrate --rehash` e `migrate --to-llm-only --drop-vec-tables`. 7 ADRs novos (0019-0025) mais o ADR-0026 documentando a causa raiz do drift da V002
- **v1.0.75**: novo trait `ExtractionBackend` (G21) atrás da flag global `--extraction-backend llm|embedding|none|both`; a extração via LLM vira o padrão
- **v1.0.74**: compatibilidade no-op de `--skip-extraction` restaurada (promessa da v1.0.45 honrada) — o erro de validação introduzido na v1.0.67 voltou a ser `tracing::warn!`
- **v1.0.73**: Correção de CI — `clang`/`mold`/`lld` instalados dentro do container `cross` para builds `aarch64-unknown-linux-gnu`
- **v1.0.72**: Correção de CI — linker mold instalado nos runners `ubuntu-latest` (12+ jobs falhavam com `invalid linker name in argument`)
- **v1.0.71**: Correção de CI — `Swatinem/rust-cache` repinado da ref inexistente `v2.8` para `v2.9.1` em 17 pontos
- **v1.0.70**: Correção de i18n — precedência POSIX manual `LC_ALL > LC_MESSAGES > LANG` (o locale de sistema cacheado ignorava env vars de runtime)
- **v1.0.69**: 12 gaps fechados (G28-G39) com enforcement OAuth-only total. **(Mudança comportamental OAuth-only)** Os spawns de `claude -p` e `codex exec` agora ABORTAM com `AppError::Validation` se `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estiverem definidas; a flag `--bare` foi REMOVIDA de todo código executável. Operadores que usam chaves de API DEVEM migrar para OAuth. **(G28 CRÍTICA)** 4 correções reforçadas para proliferação de processos: 7 flags de endurecimento em `claude_runner::build_claude_command` (sempre passa `--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions`), `SIGTERM` no timeout, novo `src/reaper.rs` que varre `/proc` no startup, e `src/system_load.rs` mais integração do `CircuitBreaker`. **(G29)** `enrich --operation body-enrich` agora tem sucesso 100% (era 100% falha de CHECK constraint), com trilha de auditoria via `memory_versions`, enum type-safe `MemorySource`, portão de preservação Jaccard (10 testes, padrão 0.7) e idempotência via `blake3`. **(G30)** Lock singleton com escopo por `(job_type, namespace, db_hash)` com novas flags `--wait-job-singleton` e `--force-job-singleton`. **(G31+G32+G33)** Novo `src/commands/codex_spawn.rs` (~700 linhas, 11 testes) unifica o pipeline de spawn, parser JSONL e validação de modelo ChatGPT Pro OAuth; `enrich --mode codex` e `ingest --mode codex` compartilham o mesmo comando canônico (antes divergentes, motivaram o wrapper `~/.local/bin/codex-clean`). **(G34)** Aviso de worker condicional ao modo (Claude > 4, Codex > 16). **(G35)** `--preflight-check`, `--fallback-mode`, `--rate-limit-buffer` evitam perda de batch em rate limit do Claude. **(G36)** `optimize` faz pré-verificação da saúde do FTS5 antes de reconstruir, mais novas `--fts-dry-run`, `--fts-progress`, `--yes`. **(G37)** `--names <NOME>` e `--names-file <CAMINHO>` para enriquecimento seletivo. **(G38)** Padrões de backup 25x mais rápidos (1000/5ms vs 100/50ms) com 4 novas flags de ajuste. **(G39)** Nova família de subcomandos `vec orphan-list`/`vec purge-orphan`/`vec stats` mais hook em `forget` para prevenir novos órfãos. **+53 testes** (692 → 745). 7 novos ADRs (`docs/decisions/adr-0011-0017-*.md`) documentam cada decisão arquitetural.
- **v1.0.68**: 2 correções CRÍTICAS para Windows + proliferação de processos.  **(G29)** `cargo install` no Windows estava quebrando com `error[E0308]` em `src/terminal.rs:29` porque `HANDLE` em `windows-sys >= 0.59` é `*mut c_void` (era `isize` em 0.48/0.52).  Substituímos pelo idiom type-safe `!handle.is_null() && handle != INVALID_HANDLE_VALUE`, fixamos `windows-sys` em `=0.59.0` exato, e adicionamos o job de CI `windows-build-check` que roda `cargo check --target x86_64-pc-windows-msvc` em todo push.  **(G28-B)** Adicionado `lock::acquire_job_singleton` por `(job_type, namespace)` para que duas invocações paralelas de `enrich`/`ingest --mode claude-code|codex` no mesmo banco falhem rápido com a nova variante de exit-75 `AppError::JobSingletonLocked { job_type, namespace }` em vez de empilhar 4 × N workers × 10 processos MCP (causa raiz do incidente de load average 276 em 2026-06-03).  **(G28-A)** `claude_runner::build_claude_command` agora respeita `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` — quando definido para um diretório vazio, o subprocesso é iniciado com `CLAUDE_CONFIG_DIR=<esse dir>`, suprimindo servidores MCP do escopo user e a fan-out de 8-10 processos.  Deliberadamente evita `--strict-mcp-config` / `--mcp-config '{}'` porque [anthropics/claude-code#10787] documenta que o Claude Code CLI ignora ambas as flags.  **(G28-D)** Helper `retry::CircuitBreaker` mais `tracing::warn!` quando `--llm-parallelism > 4` (combine com o override `CLAUDE_CONFIG_DIR` para manter a fan-out administrável).  Também corrigimos 3 falhas de teste pré-existentes em `src/commands/{history,list,read}.rs` que vazavam o env var `SQLITE_GRAPHRAG_DISPLAY_TZ` entre testes paralelos.
- **v1.0.67**: 2 NOVOS comandos: `remember-batch` (criação em lote via NDJSON com `--transaction`/`--force-merge`), `completions` (completions de shell para Bash/Zsh/Fish/PowerShell/Elvish); `read --id` para busca direta por memory_id, `enrich --llm-parallelism` para workers LLM paralelos, `health` detecta super-hubs (grau > 50), `edit` otimização skip-embed via comparação body_hash, `rename` purge de ghost para conflitos de nome soft-deleted, validação de flags em hybrid-search/recall/ingest, migração V012 timestamps em relationships, 24 correções de gaps no total
- **v1.0.66**: 35 correções BUG/GAP incluindo 3 CRÍTICAS (crash reclassify-relation, flooding de evidence chain, weight do link), flag `edit --type`, `graph_context` no deep-research, aliases LLM-friendly para graph/list JSON, auditoria completa de docs
- **v1.0.65**: 3 NOVOS comandos: `reclassify-relation` (renomeia tipos de relação em massa com tratamento de colisões UNIQUE), `normalize-entities` (normaliza nomes de entidade para kebab-case com auto-merge), `enrich` (qualidade do grafo aumentada por LLM: memory-bindings, entity-descriptions, body-enrich); Correções CRITICAL no deep-research: embeddings por sub-query (antes compartilhava um), fusão RRF para KNN+FTS5 (antes fixo em 0.5), cadeias de evidência direcionadas (antes dump flat global); novas flags deep-research `--rrf-k`, `--graph-decay`, `--graph-min-score`, `--max-neighbors-per-hop`; normalização de nomes de entidade em todos os paths de escrita; `health` reporta concentração de relações; warning `--max-entity-degree` em link/remember
- **v1.0.64**: NOVO comando `deep-research` para pesquisa profunda multi-hop paralela via decomposição de query (até 7 sub-queries) com fan-out bounded JoinSet + Semaphore e montagem de cadeias de evidência; ingest claude-code desabilita hooks via `--settings` para OAuth (falhava em 65% dos arquivos), detecta OAuth e omite `cost_usd` enganoso, valida tamanho do body ANTES da extração LLM (arquivos >512 KB ignorados); rename/rename-entity rejeitam mesmo nome com exit 1
- **v1.0.63**: restore preserva nome atual após rename (antes revertia para nome original da versão), ingest claude-code/codex normaliza relações antes de inserir no DB, edit regenera embeddings vetoriais quando body muda, documentação OAuth-first
- **v1.0.62**: 10 correções para ingest --mode claude-code (G01 CRÍTICO: recall agora funciona), NOVO --mode codex para extração via OpenAI Codex CLI, novas flags --codex-binary/--codex-model/--codex-timeout
- **v1.0.61**: 15 correções para ingest --mode claude-code (B00-B13), nova flag --claude-timeout, gerenciamento de subprocessos com wait-timeout
- **v1.0.60**: NOVO ingest --mode claude-code para extração curada por LLM via Claude Code CLI, banco de fila para resume/retry, 7 novas flags de ingest
- **v1.0.59**: validação de nome no rename-entity, correção schema unlink, campo `description_updated` no reclassify, testes contract+schema para rename-entity, testes E2E de validação de entidade, audit de docs (6 arquivos)
- **v1.0.58**: Correção FTS5 (CRÍTICO: remember --force-merge corrompia silenciosamente o índice FTS5), correção UNIQUE no merge-entities para memory_entities, novo comando `rename-entity`, validação de nomes de entidades, `memory-entities --entity` busca reversa, `reclassify --description`, campo `action` no purge, EXAMPLES no fts, tracing no health
- **v1.0.57**: 16 correções — UNIQUE constraint no merge-entities, coluna errada no memory-entities, validação --clear-body, WAL checkpoint para fts rebuild/check, recálculo de degree para delete-entity/merge-entities adjacentes, backup atômico via tempfile-rename, 18 novos testes de contrato+schema
- **v1.0.56**: 9 novos comandos (fts, backup, delete-entity, reclassify, merge-entities, memory-entities, prune-ner), 7 novas flags, 19 novos campos JSON, degradação graciosa FTS5, envelope de erro JSON
- **v1.0.55**: Auditoria completa de docs — export summary `total`→`exported`, campos de resposta do list corrigidos, exit code de `--tz` 1→2, exit 2 adicionado à tabela de exit codes, aliases legados do stats documentados
- **v1.0.54**: WAL checkpoint para `prune-relations` (último comando faltante), validação de body vazio com `--graph-stdin`, campo JSON `memory_type` em `list`/`export`, `Vec::with_capacity` em 9 cold paths
- **v1.0.53**: WAL checkpoint TRUNCATE após cada escrita para segurança com Dropbox/cloud-sync, correção do contrato `export --json`, `Vec::with_capacity` em 12 hot paths
- **v1.0.52**: 12 gaps corrigidos, novo subcomando `export`, exit code Duplicate 2→9 (breaking), `forget` not-found sem JSON (breaking)
- **v1.0.51**: Correção da env var de namespace (8 comandos), correção do remember em memória soft-deletada, watchdog de RSS por chunk (`--max-rss-mb`), cobertura de testes do daemon
- **v1.0.50**: Subcomando `prune-relations`, auto-restart do daemon em version mismatch, índice V011, 37 lacunas de docs corrigidas
- **v1.0.49**: Vocabulário extensível de relações, migração V010, 15 atualizações de docs
- **v1.0.48**: GLiNER NER funcional, 5 correções de bugs, auditoria completa de docs
- **v1.0.47**: Substituição do BERT NER pelo GLiNER zero-shot, 13 tipos de entidade customizados, flag `--gliner-variant`
- **v1.0.35**: Aliases de flags (`--from`/`--to`, `--old`/`--new`, `--limit` como alias de `--k`)


## Ciclo de Vida da Memória
### Sequência executável: init → remember → recall → forget → purge
```bash
# 1. Inicializar (uma vez por banco)
sqlite-graphrag init

# 2. Armazenar uma memória
sqlite-graphrag remember --name minha-nota --type user --description "demo" --body "primeira entrada"

# 3. Recuperar por similaridade semântica
sqlite-graphrag recall "primeira entrada" --k 5 --json

# 4. Exclusão suave (reversível)
sqlite-graphrag forget minha-nota

# 5. Remover permanentemente memórias soft-deleted com 0 dias de retenção
sqlite-graphrag purge --retention-days 0 --yes
```
> Todos os cinco comandos acima são seguros para executar em sequência em um banco recém-criado.


## Instalação
### Múltiplos canais de distribuição
- Instale a última release publicada com `cargo install sqlite-graphrag --locked`
- Atualize um binário publicado existente com `cargo install sqlite-graphrag --locked --force`
- Para fixar uma versão específica use `cargo install sqlite-graphrag --version <X.Y.Z> --locked`
- Instale a partir do checkout local com `cargo install --path .`
- Compile a partir do checkout local com `cargo build --release`


## Uso
### Inicialize o banco de dados
```bash
sqlite-graphrag init
sqlite-graphrag init --namespace projeto-foo
```
- Sem `--db` ou `SQLITE_GRAPHRAG_DB_PATH`, todo comando CRUD nessa pasta usa `./graphrag.sqlite`
### Grave uma memória com grafo de entidades explícito opcional
- Por padrão, `remember` NÃO executa extração automática de URLs (desligada por padrão)
- Passe `--enable-ner` para ativar a extração de URL por regex nessa chamada, ou defina `SQLITE_GRAPHRAG_ENABLE_NER=1` (o pipeline GLiNER foi removido na v1.0.79)
```bash
sqlite-graphrag remember \
  --name testes-integracao-postgres \
  --type feedback \
  --description "prefira Postgres real a mocks SQLite" \
  --body "Testes de integração devem usar banco real."
```
- A resposta JSON de `remember` inclui `urls_persisted` (URLs roteadas para a tabela `memory_urls`) e `relationships_truncated` (bool, ativo quando relacionamentos foram truncados)
- URLs são armazenadas em `memory_urls` via schema V007 e nunca poluem o grafo de entidades
- Exemplo de saída JSON ilustrando entidades e relacionamentos extraídos (chaves em inglês por convenção):
```json
{
  "memory": {"id": 42, "name": "audit-note", "type": "project"},
  "extracted_entities": [
    {"name": "OpenAI", "kind": "organization", "saliency": 0.92},
    {"name": "Rust", "kind": "technology", "saliency": 0.85}
  ],
  "extracted_relationships": [
    {"source": "OpenAI", "target": "GPT-4", "relation": "develops"}
  ],
  "urls_persisted": [],
  "relationships_truncated": false
}
```
### Status da extração automática (GLiNER removido na v1.0.79)
- O pipeline local GLiNER zero-shot NER foi REMOVIDO na v1.0.79 com a feature `ner-legacy`; `--enable-ner` agora executa apenas extração de URL por regex
- Para extração de entidades/relacionamentos curada por LLM use `ingest --mode claude-code` ou `ingest --mode codex`
- Para controle exato passe entidades curadas via `--graph-stdin`, `--entities-file` e `--relationships-file`
- O campo `extraction_method` na resposta JSON reporta qual caminho executou

```bash
sqlite-graphrag remember \
  --name notas-de-release-v1 \
  --type document \
  --description "notas de release para v1.0.0" \
  --enable-ner \
  --llm-parallelism 4 \
  --body-stdin < notas.md
```
### Backend de Embedding OpenRouter (v1.0.94)
- Use `--embedding-backend openrouter` com `--embedding-model` para embeddings rápidos via API REST (~200ms por chamada vs 15s subprocess)
- O usuário DEVE especificar `--embedding-model` — nenhum modelo padrão é hardcoded
- Defina `OPENROUTER_API_KEY` via env var ou passe `--openrouter-api-key`
```bash
# Remember com embedding OpenRouter
sqlite-graphrag --embedding-backend openrouter \
  --embedding-model "qwen/qwen3-embedding-8b" \
  remember --name minha-nota --type note \
  --description "embedding rápido" --body "conteúdo aqui"

# Ingest com OpenRouter + auto-enrich
sqlite-graphrag --embedding-backend openrouter \
  --embedding-model "google/gemini-embedding-001" \
  ingest ./docs --pattern "*.md" --recursive --enrich-after --json

# Recall com embedding de query OpenRouter
sqlite-graphrag --embedding-backend openrouter \
  --embedding-model "qwen/qwen3-embedding-8b" \
  recall "busca semântica" --k 10 --json
```
- Modelos suportados: `qwen/qwen3-embedding-8b` (melhor qualidade), `nvidia/llama-nemotron-embed-vl-1b-v2:free` (custo zero), `google/gemini-embedding-001` (scores mais altos), `openai/text-embedding-3-large`, e mais 6
- Todos os modelos produzem vetores de 384 dimensões por padrão via truncamento MRL — compatível com bancos existentes
### Leia, esqueça, edite e renomeie usando argumento posicional
<!-- skip-test: forget soft-deleta a memória no meio do bloco, invalidando o edit/rename seguintes. O bloco ilustra o ciclo de vida; não é um script executável. -->
```bash
sqlite-graphrag read testes-integracao-postgres --json
sqlite-graphrag forget testes-integracao-postgres
sqlite-graphrag history testes-integracao-postgres --json
sqlite-graphrag edit testes-integracao-postgres --body "Corpo atualizado."
sqlite-graphrag rename testes-integracao-postgres --new testes-postgres
```
- Nome posicional é equivalente a `--name <nome>` para `read`, `forget`, `history`, `edit` e `rename`

### Busque memórias por similaridade semântica
```bash
sqlite-graphrag recall "testes integração postgres" --k 3 --json
```
### Busca híbrida combinando FTS5 e KNN vetorial
```bash
sqlite-graphrag hybrid-search "rollback migração postgres" --k 10 --json
```
### Pesquisa profunda com decomposição multi-hop paralela (v1.0.64)
```bash
sqlite-graphrag deep-research "decisões de arquitetura de autenticação e incidentes" --k 20 --json
```
- Decompõe a query em até 7 sub-queries, executa em paralelo via `JoinSet` + `Semaphore` bounded, mescla resultados com deduplicação cross-query e monta cadeias de evidência da travessia do grafo
- Defaults calibrados contra benchmarks NovelHopQA, StepChain, HopRAG: `--k 20`, `--max-sub-queries 7`, `--max-hops 3`
### Inspecione saúde e estatísticas do banco
```bash
sqlite-graphrag health --json
sqlite-graphrag stats --json
```
### Purgue memórias soft-deleted após período de retenção
```bash
sqlite-graphrag purge --retention-days 90 --dry-run --json
sqlite-graphrag purge --retention-days 90 --yes
```
> **Retenção padrão: 90 dias.** Para purgar TODAS as memórias esquecidas independentemente da idade, passe `--retention-days 0`.

### Ingestão em massa de arquivos Markdown em um diretório
<!-- skip-test: requer um diretório `./docs` com arquivos Markdown relativo ao cwd da invocação. -->
```bash
sqlite-graphrag ingest ./docs --type document --pattern '*.md' --recursive
```
### Ingestão em massa em modo de baixa memória (worker único)
<!-- skip-test: requer um diretório `./docs`; demonstra a flag --low-memory. -->
```bash
# Força ingest single-threaded para reduzir pressão de RSS (recomendado para
# ambientes com <4 GB de RAM e restrições de container/cgroup). Trade-off: 3-4x
# mais tempo de relógio.
sqlite-graphrag ingest ./docs --type document --pattern '*.md' --low-memory

# Ou via variável de ambiente (a flag CLI tem precedência):
SQLITE_GRAPHRAG_LOW_MEMORY=1 sqlite-graphrag ingest ./docs --type document
```
### Ingestão em massa com entidades curadas por LLM via Claude Code (v1.0.61)
<!-- skip-test: requer Claude Code instalado com assinatura Pro/Max. -->
```bash
# Extrai entidades e relações usando Claude Code CLI instalado localmente
sqlite-graphrag ingest ./docs --mode claude-code --recursive --json

# Retomar ingestão interrompida
sqlite-graphrag ingest ./docs --mode claude-code --resume --json

# Definir limite de orçamento
sqlite-graphrag ingest ./docs --mode claude-code --max-cost-usd 5.00 --json

# Extrair entidades e relações usando OpenAI Codex CLI instalado localmente
sqlite-graphrag ingest ./docs --mode codex --recursive --json
```
> **Autenticação:** OAuth é o ÚNICO fluxo de credencial aceito. Chaves de API são PROIBIDAS.
> `--mode claude-code` lê OAuth de `~/.claude/.credentials.json` (Claude Pro/Max/Team).
> `--mode codex` lê autenticação de dispositivo via `codex login` (OpenAI ChatGPT).
> Definir `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` no ambiente ABORTA o spawn com `AppError::Validation` e código de saída 1. A flag `--bare` (que também exigiria uma chave de API) foi REMOVIDA de todo caminho executável.
> Veja `docs/decisions/adr-0011-oauth-only-enforcement.md` para a justificativa completa.
> `ingest` emite NDJSON no stdout: uma linha JSON por arquivo, seguida de uma linha de resumo.
> Valores de `status` por arquivo: `indexed` (criado), `skipped` (duplicata ou nome inválido), `failed` (erro).
> Duplicatas emitem `status: "skipped"` com `action: "duplicate"` e não contam como falhas.
> Passe `--dry-run` para pré-visualizar o mapeamento de nomes (basenames em kebab-case) sem escrever nada no banco.
> Schema: `docs/schemas/ingest-file-event.schema.json`, `docs/schemas/ingest-summary.schema.json`.

### Renomeie uma memória mantendo o histórico de versões
<!-- skip-test: nomes ilustrativos (`nome-antigo`, `nome-novo`) — a memória de origem não existe no banco isolado de teste. -->
```bash
sqlite-graphrag rename nome-antigo --new-name nome-novo --json
```
### Edite corpo ou descrição de uma memória (gera nova versão)
<!-- skip-test: depende da memória não ter sido soft-deleted por um bloco ilustrativo anterior. -->
```bash
sqlite-graphrag edit testes-integracao-postgres --body "Corpo atualizado."
sqlite-graphrag edit testes-integracao-postgres --description "Descrição atualizada."
```
### Restaure uma memória para uma versão anterior
<!-- skip-test: `restore --version 2` exige que a memória tenha pelo menos duas versões, o que não é o caso no banco isolado de exemplo. -->
```bash
sqlite-graphrag history testes-integracao-postgres --json
sqlite-graphrag restore --name testes-integracao-postgres --version 2 --json
```
### Aplique migrações de schema pendentes
```bash
sqlite-graphrag migrate --status --json
sqlite-graphrag migrate --json
```
### Resolva a precedência de namespace para a invocação atual
```bash
sqlite-graphrag namespace-detect --json
sqlite-graphrag namespace-detect --namespace projeto-foo --json
```
### Atualize as estatísticas do planejador de queries do SQLite
```bash
sqlite-graphrag optimize --json
```
### Recupere espaço em disco e faça checkpoint do WAL
```bash
sqlite-graphrag vacuum --json
```
### Crie um relacionamento tipado entre duas entidades
<!-- skip-test: requer que as entidades `OpenAI` e `GPT-4` já existam no namespace. -->
```bash
sqlite-graphrag link --from "OpenAI" --to "GPT-4" --relation uses --weight 0.8 --json
```
### Remova um relacionamento específico entre duas entidades
<!-- skip-test: requer o relacionamento criado pelo exemplo `link` anterior. -->
```bash
sqlite-graphrag unlink --from "OpenAI" --to "GPT-4" --relation uses --json
```
### Percorra memórias conectadas via grafo de entidades
```bash
sqlite-graphrag related primeira-memoria --max-hops 2 --limit 10 --json
```
> **Resultados vazios são normais** para memórias sem arestas no grafo ainda — extraia entidades primeiro via `remember` ou `ingest`. Arestas se formam quando ≥2 entidades co-ocorrem no mesmo corpo de memória.

### Exporte um snapshot do grafo em json, dot ou mermaid
<!-- skip-test: `--output graph.json` escreve um arquivo relativo ao cwd da invocação; polui o workspace de teste. Os demais subcomandos read-only do graph são exercitados pelos testes de integração do cookbook. -->
```bash
sqlite-graphrag graph --format json --output graph.json
sqlite-graphrag graph stats --json
sqlite-graphrag graph traverse --from "OpenAI" --depth 2 --json
sqlite-graphrag graph entities --entity-type organization --limit 50 --json
```
### Remova entidades órfãs sem memórias e sem relacionamentos
```bash
sqlite-graphrag cleanup-orphans --dry-run --json
sqlite-graphrag cleanup-orphans --yes --json
```
### Remoção em massa de relacionamentos por tipo
<!-- skip-test: requer que existam relacionamentos no namespace. -->
```bash
sqlite-graphrag prune-relations --relation mentions --dry-run --show-entities --json
sqlite-graphrag prune-relations --relation mentions --yes --json
```
### Limpe os modelos de embedding/NER em cache no diretório XDG
<!-- skip-test: apaga o cache de modelos de embedding; seguro em produção, mas no suite de integração obriga um re-download caro nos comandos seguintes. -->
```bash
sqlite-graphrag cache clear-models --yes
```
### Liste todas as versões de uma memória
<!-- skip-test: depende do estado do ciclo de vida estabelecido por blocos ilustrativos anteriores (também marcados `skip-test`). -->
```bash
sqlite-graphrag history testes-integracao-postgres --no-body --json
```


## Comandos
### Núcleo de ciclo de vida do banco
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `init` | `--namespace <ns>` | Inicializa banco, aplica migrações e valida que uma CLI `claude`/`codex`/`opencode` está alcançável (sem download de modelo) |
| `health` | `--json` | Exibe integridade, teste funcional FTS5, versão SQLite, detecção de super-hub (grau > 50) |
| `stats` | `--json` | Conta memórias, entidades e relacionamentos |
| `migrate` | `--json` | Aplica migrações pendentes via `refinery` |
| `vacuum` | `--json` | Faz checkpoint do WAL e libera espaço |
| `optimize` | `--json`, `--skip-fts` | Executa `PRAGMA optimize` e reconstrói índice FTS5 (pule com `--skip-fts`) |
| `backup` | `--output <caminho>` | Cria backup do banco via SQLite Online Backup API |
| `sync-safe-copy` | `--dest <caminho>` (alias `--output`) | Gera cópia segura para sincronização em nuvem |
### Ciclo de vida do conteúdo de memória
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `remember` | `--name`, `--type`, `--description`, `--body` (ou `--body-file`/`--body-stdin`), `--entities-file`, `--relationships-file`, `--graph-stdin`, `--llm-parallelism <N>` (padrão 4), `--enable-ner` (apenas regex de URL desde v1.0.79), `--force-merge`, `--clear-body`, `--dry-run` | Salva memória com grafo opcional; `--type`/`--description` opcionais com `--force-merge` (herdados do existente); `--dry-run` valida sem persistir |
| `remember-batch` | `--transaction`, `--force-merge`, `--fail-fast` | Criação em lote de memórias via NDJSON no stdin; uma invocação, um slot, uma conexão DB |
| `recall` | `<query>`, `-k`/`--k` (alias `--limit` desde v1.0.35), `--type`, `--max-hops`, `--max-distance`, `--all-namespaces`, `--no-graph` | Busca memórias semanticamente via KNN + travessia do grafo |
| `read` | `[nome]` ou `--name <nome>`, `--id <N>`, `--with-graph` | Recupera memória por nome kebab-case exato ou `memory_id` inteiro via `--id`; `--with-graph` inclui entidades e relacionamentos vinculados |
| `list` | `--type`, `--limit`, `--offset`, `--include-deleted` | Pagina memórias por `updated_at`; limite padrão é tudo com `--json`, 50 para texto; resposta inclui `total_count`, `truncated`, `body_length` |
| `forget` | `[nome]` ou `--name <nome>` | Remove memória logicamente preservando histórico |
| `rename` | `[antigo]`, ou `--name`/`--old`/`--from <NOME>` (desde v1.0.35), `--new-name`/`--new`/`--to <NOME>` (desde v1.0.35) | Renomeia memória mantendo versões |
| `edit` | `[nome]` ou `--name`, `--body`, `--description`, `--type`, `--force-reembed`, `--llm-parallelism <N>` | Edita corpo, descrição ou tipo gerando nova versão; pula re-embedding quando conteúdo do body é inalterado; `--force-reembed` (v1.0.79) regenera o embedding sem alterar o corpo |
| `history` | `[nome]` ou `--name <nome>`, `--diff` | Lista versões da memória; `--diff` inclui resumo de mudanças por caractere |
| `memory-entities` | `[nome]` ou `--name <nome>`, `--entity <nome>` | Lista entidades de uma memória, ou memórias vinculadas a uma entidade (busca reversa via `--entity`) |
| `restore` | `--name`, `--version` | Restaura memória para versão anterior |
| `ingest` | `<DIR>`, `--type`, `--pattern <GLOB>` (padrão `*.md`), `--recursive`, `--mode` (`none`/`claude-code`/`codex`; `gliner` aceito mas apenas regex de URL desde v1.0.79), `--ingest-parallelism N`, `--llm-parallelism N` (padrão 2, workers de embedding), `--low-memory`, `--enable-ner` (apenas regex de URL desde v1.0.79), `--fail-fast`, `--dry-run`, `--claude-binary`, `--claude-model`, `--resume`, `--retry-failed`, `--max-cost-usd`, `--claude-timeout`, `--rate-limit-wait`, `--keep-queue`, `--queue-db` | Ingere em massa cada arquivo como memória separada (NDJSON); `--mode claude-code` usa Claude Code CLI local para extração curada por LLM; `--dry-run` pré-visualiza mapeamento; `--claude-timeout` define timeout por arquivo (padrão 300s) |
| `export` | `--namespace`, `--type`, `--include-deleted`, `--limit`, `--offset` | Exporta memórias como NDJSON para backup ou migração |
| `cache clear-models` | `--yes` | Remove arquivos de modelo cacheados por versões ≤ v1.0.75 do diretório XDG cache (nenhum build baixa modelos desde a v1.0.76) |

> **Validação de nomes de memória.** Nomes devem corresponder a `[a-z0-9-]+` (kebab-case, somente ASCII).
> Unicode e maiúsculas são rejeitados com exit code 1. Nomes maiores que 60 caracteres
> emitidos por `ingest` são truncados; revise o log WARN para identificar nomes mutilados.
### Recuperação e grafo
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `hybrid-search` | `<query>`, `--k`, `--rrf-k`, `--with-graph`, `--max-hops`, `--min-weight`, `--weight-vec`, `--weight-fts` | FTS5 + vetor via RRF; degradação graciosa quando FTS5 corrompido (`fts_degraded`, auto-rebuild); `normalized_score` para comparabilidade |
| `namespace-detect` | `--namespace <nome>` | Resolve precedência de namespace para invocação |
| `link` | `--from`, `--to`, `--relation`, `--weight`, `--create-missing`, `--entity-type`, `--strict-relations` | Cria relacionamento; `--strict-relations` rejeita tipos não-canônicos; warnings no JSON |
| `unlink` | `--from`, `--to`, `--relation`, `--entity`, `--all` | Remove relacionamentos; `--relation` agora opcional (remove todos entre o par); `--entity X --all` remove todas edges da entidade |
| `related` | `--name`, `--limit`, `--hops` | Percorre memórias conectadas pelo grafo a partir de uma memória base |
| `graph` | `--format`, `--output` | Exporta snapshot do grafo em `json`, `dot` ou `mermaid` |

> **Breaking change em v1.0.44.** O JSON de `graph entities` renomeou o array de nível superior
> de `items` para `entities`. Atualize filtros jaq/jq: `.items[]` vira `.entities[]`.
> O comando `list` continua usando `items`.

### Subcomandos do graph
| Subcomando | Descrição | Flags principais |
| --- | --- | --- |
| `graph traverse --from <ENTIDADE>` | Percorre o grafo de entidades a partir de um nó inicial usando BFS | `--depth` (padrão 2), `--namespace` |
| `graph stats` | Imprime estatísticas do grafo (nós, arestas, distribuição de grau) | `--namespace` |
| `graph entities` | Lista entidades com grau e ordenação | `--limit` (padrão 50), `--entity-type`, `--namespace`, `--sort-by degree\|name\|created_at`, `--order asc\|desc` |

### Manutenção
| Comando | Argumentos | Descrição |
| --- | --- | --- |
| `purge` | `--retention-days <n>`, `--dry-run`, `--yes` | Apaga permanentemente memórias soft-deleted |
| `cleanup-orphans` | `--namespace`, `--dry-run`, `--yes` | Remove entidades sem memórias e sem relacionamentos |
| `prune-relations` | `--relation <tipo>`, `--namespace`, `--dry-run`, `--yes`, `--show-entities` | Remove em massa todos os relacionamentos de um tipo; `--show-entities` lista entidades afetadas |
| `delete-entity` | `--name <entidade>`, `--cascade` | Remove entidade e cascateia remoção de relacionamentos e bindings |
| `rename-entity` | `--name <entidade>`, `--new-name <nome>` | Renomeia uma entidade preservando todos os relacionamentos e vínculos com memórias; re-gera vetor |
| `reclassify` | `--name <entidade> --new-type <tipo>`, `--description <texto>`, ou `--from-type <antigo> --to-type <novo> --batch` | Reclassifica tipos de entidade individual ou em massa; `--description` atualiza descrição no modo individual |
| `merge-entities` | `--names <a,b,c> --into <destino>` | Funde entidades-fonte no destino, movendo todas as edges |
| `prune-ner` | `--entity <nome>` ou `--all`, `--dry-run`, `--yes` | Remove bindings NER da tabela memory_entities |
| `fts rebuild` | `--json` | Reconstrói o índice FTS5 de busca textual do zero |
| `fts check` | `--json` | Executa integrity-check do FTS5 sem modificar o índice |
| `fts stats` | `--json` | Exibe estatísticas do índice FTS5 (contagem, páginas shadow) |
| `completions` | `bash`, `zsh`, `fish`, `powershell`, `elvish` | Gera completions de shell para o shell especificado |

### Subcomandos de `cache`
| Subcomando | Descrição |
| --- | --- |
| `clear-models` | Remove os arquivos de modelo de embedding/NER em cache (força novo download no próximo `init`) |


## Variáveis de Ambiente
### Overrides de configuração em runtime
| Variável | Descrição | Padrão | Exemplo |
| --- | --- | --- | --- |
| `SQLITE_GRAPHRAG_DB_PATH` | Caminho para override do arquivo SQLite | `./graphrag.sqlite` no diretório da invocação | `/dados/graphrag.sqlite` |
| `SQLITE_GRAPHRAG_HOME` | Sobrescreve diretório base para `graphrag.sqlite` (usado quando `--db` e `SQLITE_GRAPHRAG_DB_PATH` estão ausentes) | indefinido | `/var/lib/sqlite-graphrag` |
| `SQLITE_GRAPHRAG_CACHE_DIR` | Diretório de override para cache do modelo e lock files | Diretório XDG cache | `~/.cache/sqlite-graphrag` |
| `SQLITE_GRAPHRAG_LANG` | Idioma da saída da CLI como `en` ou `pt` (alias: `pt-BR`, `portuguese`) | `en` | `pt` |
| `SQLITE_GRAPHRAG_LOG_LEVEL` | Nível do filtro de tracing para saída em stderr | `info` | `debug` |
| `SQLITE_GRAPHRAG_LOG_FORMAT` | Formato da saída de tracing em stderr (`pretty` ou `json`) | `pretty` | `json` |
| `SQLITE_GRAPHRAG_NAMESPACE` | Override de namespace ignorando detecção | nenhum | `projeto-foo` |
| `SQLITE_GRAPHRAG_DISPLAY_TZ` | Fuso horário IANA para campos `*_iso` no JSON | `UTC` | `America/Sao_Paulo` |
| `SQLITE_GRAPHRAG_EMBEDDING_DIM` | Override da dimensionalidade do embedding (v1.0.79); precedência: flag `--embedding-dim` > esta env > `schema_meta.dim` > 64; faixa [8, 4096] | `64` (bancos novos) | `384` |
| `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` | Override de modelo para chamadas de embedding `claude -p` (v1.0.79, simétrica à variável do codex) | modelo padrão da CLI | `claude-haiku-4-5-20251001` |
| `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` | Timeout por chamada de subprocesso LLM de embedding (v1.0.79) | `300` | `600` |
| `SQLITE_GRAPHRAG_ENABLE_NER` | Habilita extração automática em `remember`/`ingest`. Desde a v1.0.79 executa apenas extração de URL por regex (o pipeline GLiNER foi removido). Aceita `1`/`true`/`yes`/`on` | indefinido (desligado) | `1` |
| `SQLITE_GRAPHRAG_GLINER_VARIANT` | SEM EFEITO desde a v1.0.79 (GLiNER removido) — aceita por compatibilidade, ignorada | — | — |
| `SQLITE_GRAPHRAG_GLINER_THRESHOLD` | SEM EFEITO desde a v1.0.79 (GLiNER removido) — aceita por compatibilidade, ignorada | — | — |
| `SQLITE_GRAPHRAG_GLINER_MODEL` | SEM EFEITO desde a v1.0.79 (GLiNER removido) — aceita por compatibilidade, ignorada | — | — |
| `SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS` | Budget de tokens para extração de entidades/relações por memória; valores fora de [512, 100.000] utilizam o padrão | `5000` | `8000` |
| `SQLITE_GRAPHRAG_MAX_ENTITIES_PER_MEMORY` | Máximo de entidades distintas persistidas por memória; valores fora de [1, 1.000] utilizam o padrão. Nota: o pipeline de extração limita internamente os candidatos a 30 antes da deduplicação, portanto o cap de persistência (padrão 50) funciona como teto de segurança e só é atingido se o extrator for estendido ou substituído. | `50` | `100` |
| `SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY` | Máximo de relações distintas persistidas por memória; valores fora de [1, 10.000] utilizam o padrão | `50` | `200` |
| `SQLITE_GRAPHRAG_LOW_MEMORY` | Força ingest single-threaded para reduzir RSS. Aceita `1`/`true`/`yes`/`on` (case-insensitive) | indefinido (multi-thread) | `1` |
| `SQLITE_GRAPHRAG_CLAUDE_BINARY` | Caminho explícito para o binário Claude Code; afeta TODOS os comandos LLM (`recall`, `hybrid-search`, `remember`, `edit`, `ingest --mode claude-code`, `enrich`, `deep-research`). v1.0.89: agora propagado da flag CLI `--claude-binary` | busca no PATH | `/usr/local/bin/claude` |
| `SQLITE_GRAPHRAG_CODEX_BINARY` | Caminho explícito para o binário Codex CLI; afeta TODOS os comandos LLM (`recall`, `hybrid-search`, `remember`, `edit`, `ingest --mode codex`, `enrich`, `deep-research`). v1.0.89: nova flag `--codex-binary` | busca no PATH | `/usr/local/bin/codex` |
| `SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE` | Quando definida, comandos persistem memórias com embedding NULL em vez de abortar com exit 11 em falha do LLM. Use `enrich --operation re-embed` para preencher depois. Aceita `1`/`true`/`yes`/`on` (v1.0.89) | desativado (abortar em falha) | `1` |
| `SQLITE_GRAPHRAG_LLM_MODEL` | Modelo padrão para chamadas de embedding LLM; sobrescrito pelas variáveis específicas por backend (`SQLITE_GRAPHRAG_CODEX_EMBED_MODEL`, `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL`). Mapeia para flag CLI `--llm-model` (v1.0.89) | `gpt-5.5` (codex) / `claude-sonnet-4-6` (claude) | `gpt-5.4` |
| `SQLITE_GRAPHRAG_LLM_FALLBACK` | Cadeia de fallback separada por vírgula para `--llm-backend auto`. Tokens: `codex`, `claude`, `none`. Mapeia para flag CLI `--llm-fallback` (v1.0.89) | `codex,claude,none` | `claude,none` |
| `SQLITE_GRAPHRAG_LLM_MAX_HOST_CONCURRENCY` | Máximo de subprocessos LLM concorrentes no host. Mapeia para flag CLI `--llm-max-host-concurrency` (v1.0.89) | `4` | `8` |
| `SQLITE_GRAPHRAG_LLM_SLOT_NO_WAIT` | Quando definida, aborta imediatamente em vez de esperar por slot LLM. Aceita `1`/`true`/`yes`/`on`. Mapeia para flag CLI `--llm-slot-no-wait` (v1.0.89) | desativado (esperar) | `1` |
| `OPENROUTER_API_KEY` | Chave API para backend de embedding OpenRouter (v1.0.94); também aceita via flag `--openrouter-api-key` ou config XDG | não definida | `sk-or-v1-...` |
| `SQLITE_GRAPHRAG_EMBEDDING_BACKEND` | Seleção padrão de backend de embedding (v1.0.94); valores: `auto`, `openrouter`, `llm`. Mapeia para flag `--embedding-backend` | `auto` | `openrouter` |
| `ORT_DYLIB_PATH` | HISTÓRICA (≤ v1.0.75) — nenhum build carrega ONNX desde a v1.0.76; a variável é ignorada | — | — |


## Padrões de Integração
### Compondo com pipelines e ferramentas Unix
```bash
sqlite-graphrag recall "testes auth" --k 5 --json | jaq -r '.results[].name'
```
### Alimente busca híbrida em endpoint sumarizador
```bash
sqlite-graphrag hybrid-search "migração postgres" --k 10 --json \
  | jaq -c '.results[] | {name, combined_score}' \
  | xh POST http://localhost:8080/summarize
```
### Backup com snapshot atômico e compressão
```bash
sqlite-graphrag sync-safe-copy --dest /tmp/ng.sqlite
ouch compress /tmp/ng.sqlite /tmp/ng-$(date +%Y%m%d).tar.zst
```
### Exemplo de subprocesso no Claude Code em Node
```javascript
const { spawn } = require('child_process');
const proc = spawn('sqlite-graphrag', ['recall', query, '--k', '5', '--json']);
```
### Build Docker Debian para pipelines de CI
```dockerfile
FROM rust:1.88-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo install --path .
```


## Códigos de Saída
### Status determinísticos para orquestração
| Código | Significado | Causa Possível |
| --- | --- | --- |
| `0` | Sucesso | Comando concluído e payload JSON impresso quando solicitado |
| `1` | Erro de validação ou falha em runtime | `--type` inválido, `--relation` malformado (vazio ou fora de snake_case), violação de kebab-case, erro genérico anyhow |
| `2` | Erro de uso da CLI | Flag inválida, argumento obrigatório ausente, timezone `--tz` inválido (Clap `FromStr` rejeita antes do código da aplicação) |
| `9` | Duplicata detectada | `--name` existente sem `--force-merge`; o `ingest` pula o arquivo e emite `status: "skipped"` com `action: "duplicate"` |
| `3` | Conflito durante atualização otimista | `edit` ou `restore` competiu com outro escritor |
| `4` | Memória ou entidade não encontrada | Alvo de `read`, `forget`, `edit`, `rename`, `restore` ou `graph traverse` ausente |
| `5` | Namespace não pôde ser resolvido | Sem `SQLITE_GRAPHRAG_NAMESPACE`, sem flag, sem padrão detectado |
| `6` | Payload excedeu limites configurados | `--name` maior que 80 bytes, body acima de `512000` bytes, mais de `512` chunks |
| `10` | Erro do banco SQLite | Arquivo corrompido, schema divergente, migração ausente |
| `11` | Geração de embedding falhou | Erro no subprocesso LLM ou falha ao carregar modelo |
| `12` | Extensão `sqlite-vec` falhou ao carregar | Extensão nativa ausente ou build do SQLite incompatível |
| `13` | Falha parcial em lote | `import`, `reindex` ou stdin batch com pelo menos um registro com falha |
| `14` | Erro de I/O do sistema de arquivos | Diretório de cache ou de banco sem permissão de escrita, diretório de destino `ingest` inexistente |
| `15` | Banco ocupado após tentativas | Contenção do WAL excedeu o orçamento de `with_busy_retry` |
| `20` | Erro interno ou de serialização JSON | Falha inesperada do serde ou violação de invariante |
| `75` | `EX_TEMPFAIL` lock timeout ou todos os slots ocupados | Cinco ou mais invocações concorrentes ou `flock` esperou mais de 300s |
| `77` | RAM disponível abaixo do mínimo | Menos de 2 GB de RAM livre detectados antes do load do modelo |
| `78` | Erro de configuração OpenRouter | `--embedding-backend openrouter` sem `--embedding-model`, ou `OPENROUTER_API_KEY` inválida/ausente |


## Desempenho
### Medido em banco com 1000 memórias
- A latência de embedding é dominada pelo round-trip do LLM headless (~1-3 s por chamada em lote); leituras puras (`read`, `list`, `graph`) ficam em poucos milissegundos
- Desde a v1.0.79 as chamadas LLM são EM LOTE (bases de calibração de 8 chunks / 25 nomes de entidade em dim 64, adaptativas à dim — G44) e PARALELAS (`--llm-parallelism`, `Semaphore` + `JoinSet` limitados), então uma memória de 39 itens embeda em 4-5 chamadas em vez de 39 spawns serializados
- `--embedding-dim 384` (o padrão desde a v1.0.94) casa com o corpus de produção; sob OpenRouter REST o truncamento MRL é no servidor a custo zero de token
- `init` não baixa modelo algum — apenas cria o banco e valida que uma CLI `claude`/`codex`/`opencode` está alcançável
- **Build (v1.0.79):** cada chamada de embedding spawna `claude -p`, `codex exec` ou `opencode run` — RSS de ~350 MB por worker LLM (a carga de 1100 MB do modelo ONNX não existe mais em nenhum build)


## Requisitos de Memória
### Dimensionando RAM para cargas de ingest e recall
- A CLI em si é leve (binário de ~14.6 MiB); a RAM é dominada pelos subprocessos LLM com aproximadamente 350 MB de RSS por worker (`LLM_WORKER_RSS_MB`)
- Orçamento de workers: o paralelismo efetivo é `min(--llm-parallelism, cpus, ram_livre × 0.5 / 350 MB, 32)` — o portão de concorrência se adapta automaticamente à memória disponível
- O paralelismo padrão aumenta o RSS de forma quase linear por worker (`--llm-parallelism 4` ≈ 4 × 350 MB de RSS de subprocessos além da CLI)
- Modo de baixa memória: passe `--low-memory` (ou defina `SQLITE_GRAPHRAG_LOW_MEMORY=1`) para forçar ingest single-threaded. Equivale a `--ingest-parallelism 1` e sobrescreve qualquer valor explícito, ao custo de 3-4x mais tempo de relógio.
- Usuários de container/cgroup: orce `MemoryMax` para a CLI mais N × 350 MB de workers LLM (o antigo piso de 3 GB do ONNX não existe mais)


## Espaço em Disco
### Tamanho esperado do banco em relação ao conteúdo ingerido
> **Overhead esperado: aproximadamente 8× o tamanho total dos corpos ingeridos** (ex.: 7,6 MB de texto → ~62,9 MB de banco).
> O overhead vem dos embeddings float (padrão de 64 dimensões desde a v1.0.79; bancos pré-existentes mantêm a dimensionalidade gravada, ex.: 384), do índice FTS5 e do grafo de entidades/relacionamentos.
> Execute `sqlite-graphrag vacuum --json` após ciclos de `forget`+`purge` em massa para recuperar espaço.


## Invocação Paralela Segura
### Semáforo de contagem com até quatro slots simultâneos
- Cada worker LLM de embedding (subprocesso `claude -p`/`codex exec`/`opencode run`) consome aproximadamente 350 MB de RSS — a unidade de orçamento do portão de concorrência desde a v1.0.79
- `MAX_CONCURRENT_CLI_INSTANCES` continua sendo o teto rígido de 4 subprocessos cooperantes
- Comandos pesados `init`, `remember`, `recall` e `hybrid-search` podem ser reduzidos dinamicamente para baixo desse teto quando a RAM disponível não sustenta o paralelismo com segurança
- Arquivos de lock em `~/.cache/sqlite-graphrag/cli-slot-{1..4}.lock` usando `flock`
- Uma quinta invocação aguarda até 300 segundos e então encerra com código 75
- Use `--max-concurrency N` para solicitar o limite de slots na invocação atual; comandos pesados ainda podem ser reduzidos automaticamente
- Memory guard aborta com saída 77 quando há menos de 2 GB de RAM disponível
- SIGINT e SIGTERM disparam shutdown graceful via atômica `shutdown_requested()`


## Solução de Problemas
### Segurança com cloud sync (Dropbox, iCloud, OneDrive)
- sqlite-graphrag usa modo WAL por padrão para escrita de alta concorrência
- Desde v1.0.54, todo comando de escrita executa `PRAGMA wal_checkpoint(TRUNCATE)` após commit (v1.0.53 cobriu 11 de 12; v1.0.54 adicionou o `prune-relations` faltante)
- Isso garante que o arquivo `.sqlite` esteja sempre autocontido quando ferramentas de cloud sync o leem
- Se ocorrer corrupção apesar do checkpoint, recupere com `sqlite3 corrompido.sqlite ".recover" | sqlite3 reparado.sqlite`

### Problemas comuns e correções
- O comportamento padrão sempre cria ou abre `graphrag.sqlite` no diretório atual
- Banco travado após crash exige `sqlite-graphrag vacuum` para fazer checkpoint do WAL
- `init` é quase instantâneo desde a v1.0.76 — não há download de modelo; se falhar, verifique se uma CLI `claude`, `codex` ou `opencode` está alcançável no `PATH`
- Chamadas de embedding falhando com exit 11 normalmente indicam CLI LLM ausente, sem autenticação (OAuth obrigatório) ou timeout — aumente `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` (padrão 300) em links lentos
- A orientação sobre `ORT_DYLIB_PATH`/`libonnxruntime.so` é HISTÓRICA (≤ v1.0.75) — nenhum build carrega ONNX desde a v1.0.76
- Permissão negada no Linux indica falta de escrita no diretório de cache do usuário
- Detecção de namespace cai para `global` quando não há override explícito
- Invocações paralelas que excedem o limite seguro efetivo recebem saída 75 e DEVEM tentar com backoff; durante auditorias inicie comandos pesados com `--max-concurrency 1`


## Crates Rust Compatíveis
### Invoque sqlite-graphrag de qualquer framework Rust de IA via subprocesso
- Cada crate chama o binário via `std::process::Command` com a flag `--json`
- Nenhuma memória compartilhada ou FFI necessária: o contrato é JSON puro em stdout
- Fixe a versão do binário no `Cargo.toml` do workspace para builds reproduzíveis
- Todos os 18 crates abaixo funcionam identicamente em Linux, macOS Apple Silicon e Windows

### rig-core
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "project goals", "--k", "5", "--json"])
    .output().unwrap();
```

### swarms-rs
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "agent memory", "--k", "10", "--json"])
    .output().unwrap();
```

### autoagents
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "task-context", "--type", "project",
           "--description", "current sprint goal", "--body", "finish auth module"])
    .output().unwrap();
```

### graphbit
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "decision log", "--k", "3", "--json"])
    .output().unwrap();
```

### agentai
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "previous decisions", "--k", "5", "--json"])
    .output().unwrap();
```

### llm-agent-runtime
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "user preferences", "--k", "5", "--json"])
    .output().unwrap();
```

### anda
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["stats", "--json"])
    .output().unwrap();
```

### adk-rust
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "tool outputs", "--k", "5", "--json"])
    .output().unwrap();
```

### rs-graph-llm
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "graph relations", "--k", "10", "--json"])
    .output().unwrap();
```

### genai
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "model context", "--k", "5", "--json"])
    .output().unwrap();
```

### liter-llm
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "session-notes", "--type", "user",
           "--description", "resumo da sessão", "--body", "discutimos arquitetura"])
    .output().unwrap();
```

### llm-cascade
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "fallback context", "--k", "3", "--json"])
    .output().unwrap();
```

### async-openai
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "system prompt history", "--k", "5", "--json"])
    .output().unwrap();
```

### async-llm
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "chat context", "--k", "5", "--json"])
    .output().unwrap();
```

### anthropic-sdk
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "tool use patterns", "--k", "5", "--json"])
    .output().unwrap();
```

### ollama-rs
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "local model outputs", "--k", "5", "--json"])
    .output().unwrap();
```

### mistral-rs
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "inference context", "--k", "10", "--json"])
    .output().unwrap();
```

### llama-cpp-rs
```rust
use std::process::Command;
let out = Command::new("sqlite-graphrag")
    .args(["recall", "llama session context", "--k", "5", "--json"])
    .output().unwrap();
```


## Contribuindo
### Pull requests são bem-vindos
- Leia as diretrizes de contribuição em [CONTRIBUTING.md](CONTRIBUTING.md)
- Abra issues no repositório do GitHub para bugs ou pedidos de funcionalidade
- Siga o código de conduta descrito em [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)


## Segurança
### Política de divulgação responsável
- Reportes de segurança seguem a política descrita em [SECURITY.md](SECURITY.md)
- Contate o mantenedor em privado antes de divulgar vulnerabilidades publicamente


## JSON Schemas
### Contratos canônicos para cada resposta de subcomando
- JSON Schemas autoritativos para cada resposta `--json` ficam em [`docs/schemas/`](docs/schemas/) e são versionados junto com a crate
- 64 schemas cobrem `init`, `remember`, `remember-batch` (+ summary), `recall`, `hybrid-search`, `deep-research`, `list`, `read`, `forget`, `purge`, `rename`, `edit`, `history`, `restore`, `link`, `unlink`, `prune-relations`, `health`, `stats`, `migrate` (+ `migrate-rehash` + `migrate-to-llm-only`), `vacuum`, `optimize`, `cleanup-orphans`, `sync-safe-copy`, `backup`, `graph` (+ stats/traverse/entities), `related`, `namespace-detect`, `debug-schema`, `entities-input`, `relationships-input`, `ingest-file-event` (+ `ingest-summary`), `ingest-claude-phase` (+ file-event + summary), `export-memory-line` (+ summary), `enrich-phase` (+ item-event + summary), `fts rebuild` (+ `fts check` + `fts stats`), `vec orphan-list` (+ `vec purge-orphan` + `vec stats`), `codex-models`, `error-envelope`
- Trate estes schemas como o contrato de agente; SKILL.md documenta as mesmas formas em formato humano
- Valide consumidores downstream com qualquer validador JSON Schema padrão (e.g. `ajv`, `jsonschema`)


## Histórico de Mudanças
### Histórico de releases mantido em arquivo separado
- Leia o histórico completo de releases em [CHANGELOG.pt-BR.md](CHANGELOG.pt-BR.md)


## Agradecimentos
### Construído sobre excelente código aberto
- `fastembed` e `sqlite-vec` sustentaram o pipeline de embedding local até a v1.0.75 (removidos desde então — os embeddings agora vêm de subprocessos `claude`/`codex`)
- `refinery` executa migrações de schema com garantias transacionais
- `clap` potencializa o parsing de argumentos da CLI com macros derive
- `rusqlite` encapsula o SQLite com bindings Rust seguros e build embutido


## Licença
### Licença dual MIT OR Apache-2.0
- Licenciado sob Apache License 2.0 ou MIT License à sua escolha
- Veja `LICENSE-APACHE` e `LICENSE-MIT` na raiz do repositório para texto completo
