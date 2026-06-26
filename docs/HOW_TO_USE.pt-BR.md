## O Que Mudou na v1.0.93 — Backend de Embedding OpenRouter (GAP-OR-INGEST)
- Novos flags globais: `--embedding-backend auto|openrouter|llm`, `--embedding-model MODEL`, `--openrouter-api-key KEY`
- Embedding via API REST OpenRouter substitui subprocess LLM para geração de vetores (~200ms vs 15s por chamada)
- `EmbeddingBackendChoice` propagado para TODOS os 13 paths de embedding: `remember`, `remember-batch`, `ingest`, `recall`, `edit`, `restore`, `hybrid-search`, `deep-research`, `enrich`, `init`, `rename-entity`, `ingest` (modo claude), `remember` (embedding de chunks)
- Novo flag `--enrich-after` para ingest dispara `enrich --operation memory-bindings` após embedding
- O usuário DEVE especificar `--embedding-model` ao usar `--embedding-backend openrouter` — SEM modelo padrão
- Defina chave API via env var `OPENROUTER_API_KEY` ou flag `--openrouter-api-key`
- 10 modelos verificados E2E: Qwen 4B/8B, NVIDIA Nemotron (gratuito), OpenAI small/large, Perplexity, Mistral, BAAI bge-m3, Google Gemini 001/002
- Todos os modelos produzem vetores de 64 dims via MRL — zero mudança de schema, zero migração
- **GAP-OR-PROPAGATION** (v1.0.93): 5 paths de embedding adicionais corrigidos — `enrich --operation re-embed`, `init` (probe de dimensão), `rename-entity`, `ingest --mode claude-code` (4 call sites) e `remember` (embedding paralelo de chunks) agora honram `--embedding-backend openrouter`
- **BUG-OR-EXIT-CODE** (v1.0.93): Erros de configuração OpenRouter (chave ausente, modelo ausente, chave inválida) agora retornam exit code 78 (`EX_CONFIG`) em vez de exit 1
```bash
# Configuração
export OPENROUTER_API_KEY="sk-or-v1-sua-chave-aqui"

# Remember com OpenRouter
sqlite-graphrag --embedding-backend openrouter \
  --embedding-model "qwen/qwen3-embedding-8b" \
  remember --name minha-nota --type note \
  --description "embedding rápido" --body "conteúdo" --json

# Ingest com OpenRouter + auto-enrich
sqlite-graphrag --embedding-backend openrouter \
  --embedding-model "qwen/qwen3-embedding-8b" \
  ingest ./docs --pattern "*.md" --recursive \
  --enrich-after --llm-backend codex --json
```


## Custom Providers (v1.0.83+)
- O sqlite-graphrag suporta providers Anthropic-compatíveis (Minimax/api.minimax.io, OpenRouter, AWS Bedrock, gateways corporativos) preservando as seguintes env vars ao spawnar `claude -p` ou `codex exec`
- Vars preservadas: `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT`
- O mandato OAuth-only permanece ativo: `ANTHROPIC_API_KEY` e `OPENAI_API_KEY` ainda abortam o spawn com exit 1
- Os quatro guards OAuth-only em `claude_runner.rs:273`, `codex_spawn.rs:259`, `ingest_claude.rs:282`, `extract/llm_embedding.rs:237-253` não foram alterados; apenas o whitelist env-clear foi estendido
- Helper compartilhado `src/spawn/env_whitelist.rs` expõe `apply_env_whitelist(cmd, strict)`; os três spawners delegam em vez de inlinear o array
- Para ambientes compliance que exigem env_clear estrito (PCI-DSS, SOC2, HIPAA), setar `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` ou passar `--strict-env-clear`; modo estrito preserva apenas `PATH`
- Sem telemetria nova: o fix é silencioso. Nenhum macro `tracing::info!` registra qual provider está em uso. O teste de auditoria no-leak `audit_no_token_leak_in_subprocess_stderr` em `tests/claude_runner_env.rs` garante que o valor literal do token NUNCA aparece em stdout ou stderr mesmo com `RUST_LOG=trace`
- Veja `docs/decisions/adr-0041-preserve-custom-provider-env.pt-BR.md` e `docs/COOKBOOK.pt-BR.md#como-usar-providers-anthropic-compativeis-customizados-v1083` para a receita completa
- Resolve GAP-058 parcialmente: env vars de custom-provider roteiam em torno de contenção de quota OAuth; `recall`/`hybrid-search` permanecem determinísticos sob fadiga OAuth oficial
# COMO USAR sqlite-graphrag (v1.0.93 — Embedding OpenRouter, GAP-OR-PROPAGATION, 1059 testes)

> Entregue memória persistente a qualquer agente de IA com um binário local, um único arquivo SQLite, e a CLI de LLM que você já confia.

- Versão em inglês: [HOW_TO_USE.md](HOW_TO_USE.md)
- Voltar ao [README.md](../README.md) para referência de comandos


## O Que Mudou na v1.0.90, v1.0.91

### v1.0.91 — Isolamento de CWD, Correção de Degree, 6-Gap Doc Remediation

- **GAP-SPAWN-001**: `apply_cwd_isolation()` adicionado em `src/spawn/mod.rs` — define `current_dir(temp_dir)` e `CLAUDE_CONFIG_DIR=temp_dir` em TODOS os 10 sites de spawn de subprocessos LLM. Elimina interferência de walk-up de `.mcp.json`. O workaround manual `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1 CLAUDE_CONFIG_DIR=/tmp/graphrag-empty-config` NÃO É MAIS NECESSÁRIO
- **GAP-SPAWN-002**: `cleanup_spawn_dir()` adicionado em `src/main.rs` — remove diretório de spawn ao final do processo via `remove_dir()` não-recursivo
- **BUG-14**: Teste `opencode_adapter_build_args` corrigido — assertava `"headless"` mas adapter retorna `"run"` desde refatoração v1.0.90
- **BUG-15**: 7 JSON schemas atualizados de `backend_invoked: enum ["claude", "codex", "none"]` para `["claude", "codex", "opencode", "none", "auto"]`. Afetados: `embedding-status`, `enrich-summary`, `hybrid-search`, `recall`, `remember`, `ingest-summary`, `edit`
- **BUG-16**: `deep-research.schema.json` ganhou `vec_degraded: boolean` em `ResearchStats` (ausente, violava `additionalProperties: false`)
- **BUG-17 (ALTA)**: Inflação de `entities.degree` corrigida — `remember` e `ingest` agora usam `recalculate_degree()` após inserção de relações em vez de `increment_degree()` por entidade. `graph stats`, `graph entities` e tabela `entities` agora consistentes

### v1.0.90 — Integração Backend OpenCode (ADR-0051)

- Terceiro backend LLM: `--llm-backend opencode` spawna OpenCode CLI headless via `opencode run --format json --dangerously-skip-permissions`
- Novas flags: `--opencode-binary`, `--opencode-model`, `--opencode-timeout`; env vars `SQLITE_GRAPHRAG_OPENCODE_BINARY`, `SQLITE_GRAPHRAG_OPENCODE_MODEL`, `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL`, `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT`
- Modelo padrão: `opencode/big-pickle`; modelos gratuitos: `opencode/deepseek-v4-flash-free`, `opencode/mimo-v2.5-free`, `opencode/nemotron-3-ultra-free`, `opencode/north-mini-code-free`
- Cadeia de fallback: `--llm-backend codex,claude,opencode,none` tenta cada backend em ordem
- `--mode opencode` para pipelines de extração de entidades em `ingest` e `enrich`
- Saída NDJSON do opencode tem 3 tipos de evento: `step_start`, `text`, `step_finish`
- 24 bugs/gaps remediados; auditoria completa de skills com ADR-0051

## O Que Mudou na v1.0.86, v1.0.87, v1.0.88, v1.0.89 (ADR-0045, ADR-0046, ADR-0047, ADR-0048, ADR-0049)

Desde a v1.0.85.2, quatro releases introduziram a superfície LLM-heavy, a camada de validação pre-flight, três hotfixes e o contrato de schema como artefato derivado.

### v1.0.86 — Superfície LLM-Heavy e Semáforo de Slots Host-Wide

- Cinco novos subcomandos expõem o pipeline de subprocessos LLM: `pending list`, `pending show`, `pending cleanup`, `embedding status`, `embedding list`, `embedding abandon`, `pending-embeddings list`, `pending-embeddings process`, `slots status`, `slots release`
- `pending` (V014 — tabela `pending_memories`) fornece checkpoint de 3 estágios para o pipeline `remember`. O checkpointer sobrevive a crash; no restart, `pending list` inspeciona a fila e `pending show <id>` lê uma entrada
- `embedding status --filter-status queued|processing|done|failed|skipped` e `--llm-backend codex,claude,none` expõem o pipeline retry-fallback
- `slots status` reporta `max_concurrency`, `acquired`, `waiting`, `held_by_pid[]`; `slots release --slot-id N --yes` ceifa slots órfãos
- Novas flags globais: `--max-concurrency <N>`, `--wait-lock <SECONDS>`, `--llm-parallelism <N>` (padrão 4, clamp [1, 32]), `--ingest-parallelism <N>`, `--graceful-shutdown-secs <N>`, `--skip-embedding-on-failure` (válido apenas com `--llm-backend …,none`)
- Contenção de lock via `fs4 = 0.9` com `fcntl(F_SETLK)` em Unix e `LockFileEx` em Windows (ADR-0039)

### v1.0.87 — Camada de Validação Pre-Flight (ADR-0045, GAP-META-005)

- Novo módulo `src/spawn/preflight.rs` (≥200 linhas, 7 guards, 15 testes unitários) porta todo spawn de subprocesso LLM ANTES do fork
- Nova variante `AppError::PreFlightFailed(PreFlightError)` com `exit_code() == 16` e `is_permanent() == true`
- Novo exit code 16 (`EX_CONFIG`) para falhas pre-flight. Não documentado em nenhuma tabela de exit code pré-existente
- Os 7 guards em ordem: `check_argv_size` (argv excederia ARG_MAX menos 4 KB), `check_binary_exists` (claude/codex alcançável em PATH), `check_mcp_config_inline` (substitui `--mcp-config "{}"` literal por tempfile com `{"mcpServers":{}}`), `check_mcp_config_path` (valida conteúdo JSON), `check_walkup_mcp_json` (rejeita `.mcp.json` inválido em cadeia ancestral do workspace), `check_output_buffer` (eleva buffer do parser acima de 64 KB), `check_claude_config_dir` (evita vazamento MCP user-level)
- Bypass em emergências: `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` desabilita todos os 7 guards. Bypassing reverte para `Command::spawn()` direto e herda todas as 5 classes BUG do GAP-META-005
- Os 4 spawners (`claude_runner`, `codex_spawn`, `ingest_claude`, `extract/llm_embedding`) compartilham este módulo único

### v1.0.88 — Hotfixes BUG-11/12/13 (ADR-0046, ADR-0047)

- **BUG-11 (CRÍTICO)** corrigido: falha pre-flight em `extract/llm_embedding.rs:563-565` agora propaga para `remember` via `embed_via_backend_strict` em vez de persistência silenciosa com `backend_invoked: "none"`
- **BUG-12 (MÉDIO)** corrigido: enforço OAuth-only agora emite 1 linha stderr (eram 2) — `eprintln!` duplicado removido
- **BUG-13 (MÉDIO)** corrigido: `link --create-missing` agora respeita validação de nome de entidade; abreviações ALL_CAPS rejeitadas eram aceitas via CLI
- 11 novos regression tests: `tests/bug11_preflight_regression.rs` (2), `oauth_stderr_emits_single_line_v1088` (1), `tests/entity_validation_integration.rs` (8)
- Renomeação de teste `embed_with_fallback_succeeds_via_none_when_chain_exhausts` → `embed_with_fallback_chain_of_only_none_aborts_without_skip_on_failure_v1088` documenta o contrato corrigido

### v1.0.89 — Schema Drift, Flag Parity, Description Heuristic (ADR-0048, ADR-0049)

- **GAP-E2E-007 (P1)**: `health.schema.json` regenerado via `schemars` derive macro. 17 novos campos adicionados; `additionalProperties: true` (política Must-Ignore por RFC 7493 I-JSON). Novo binário: `cargo run --bin dump-schema` regenera 70+ schemas
- **GAP-E2E-008 (P3)**: `embedding status/list/abandon`, `pending list/show` agora aceitam `--db <PATH>`. `clap::Arg::global = true` foi REJEITADO (invasivo, polui help). 5 novos testes em `tests/cli_db_flag_parity_regression.rs`
- **GAP-E2E-009 (P3)**: `migrate --dry-run --json` agora reporta migrações pendentes sem aplicar. 1 novo teste em `tests/migrate_dry_run_regression.rs`
- **GAP-E2E-010 (P3)**: `codex-models --json` aceito como no-op; paridade de `pending list --db <PATH>`. Ambos com `#[arg(long, hide = true)]`. 1 novo teste em `tests/codex_models_json_regression.rs`
- **GAP-E2E-011 (P2)**: `ingest --auto-describe` (padrão true) extrai descrição da primeira linha significativa do corpo (>20 chars, não header). `extract_heuristic_description(body, path_hint)` cai para o stem do arquivo. Opt-out via `--no-auto-describe`. 5 novos testes em `tests/ingest_auto_describe_regression.rs`
- **GAP-E2E-002 (P3)**: `health --namespace <NS> --json` filtra contagens para um único namespace. 1 novo teste em `tests/health_namespace_regression.rs`
- **GAP-E2E-001 (P2)**: Tamanho do binário 14.6 MiB documentado em `Cargo.toml:6` (era 6 MB desde v1.0.76). 1 novo teste em `tests/binary_size_documented_regression.rs`
- Total: 1059 testes passando. Binário 15.3 MB ELF stripped
## O Que Mudou na v1.0.80 (G45, G53, G55 S2, G56, G58, ADR-0033, ADR-0034)

A v1.0.80 é bump **patch** SEM migração de banco. O schema continua
v13, a adoção de dim do G43 já roda em todo `open_rw` e `open_ro`,
e as mudanças são todas aditivas no nível binário e de banco.
Consumidores da biblioteca devem fixar em `=1.0.80` porque a API
da lib é instável dentro de v1.x.y (ADR-0032).

- **G45 singleton de embedding cross-process**: `acquire_embedding_singleton(namespace, db_path, wait_seconds, force)` serializa chamadas de embedding LLM por par `(namespace, db)` entre invocações CLI concorrentes. Uma segunda CLI tentando embedar contra o mesmo banco recebe `AppError::EmbeddingSingletonLocked { namespace }` (exit 75, retentável). Passe `--wait-embed-singleton <SEGUNDOS>` para fazer poll até a soltura do lock; bancos ou namespaces distintos adquirem locks independentes. Operacionalmente previne a patologia de "duas invocações de remember, dois subprocessos LLM, dois batches paralelos" que o cache em processo da v1.0.79 não conseguia endereçar.
- **G53 política de estabilidade e gate de CI `semver-checks`**: o contrato público é a CLI; a API da biblioteca é instável em v1.x.y. Novo job de CI `semver-checks` roda `cargo semver-checks check-baseline --baseline-version 1.0.79` em modo informativo (vira bloqueante em v1.0.81 quando as 9 violações MAJOR pendentes forem resolvidas). README e CHANGELOG carregam a seção `Política de Estabilidade`. Fixe em `=1.0.80` para consumidores da lib; use `^1.0` para permanecer na trilha de estabilidade da CLI.
- **G55 S2 `MemoryNotFound` estrutural**: o caminho legado `NotFound(String)` que mascarava qual alvo de lookup falhou é substituído por `AppError::MemoryNotFound { name, namespace }` e `AppError::MemoryNotFoundById { id }` dentro de `read` e `hybrid-search`. O identificador agora é parte da variante, eliminando a classe de bugs `not found: unknown`. As mensagens em pt-BR carregam nome e namespace explicitamente.
## O Que Mudou em v1.0.85, v1.0.85.1, v1.0.85.2 (ADR-0043, ADR-0044)

Desde v1.0.84 (GAP-002 split do backend Claude, ADR-0042), três releases adicionais apertaram o embedder:

### v1.0.85 — Remediação de Cinco Gaps (ADR-0043)
- Enum `FallbackReason` estendido de 3 para 7 variantes: `embedding_failed | slot_exhausted | oauth_quota | backend_mismatch | dim_zero | cancelled | timeout`
- Discriminador `reason_code` nos envelopes `recall` e `hybrid-search` distingue quota vs mismatch vs timeout
- `try_embed_query_with_deterministic_fallback` retenta em `OAuthQuota` e aplica teto de 750ms em `SlotExhausted` antes de cair em FTS5
- 12-14 headers `anthropic-ratelimit-*-remaining` capturados em `LlmEmbedding::invoke_claude` (G45-CR5); `0` aborta embed e dispara fallback codex
- Lock de `dim 64` (Matryoshka Representation Learning, arXiv 2205.13147) reduz gasto de tokens OAuth em 6x (G56)
- 5 testes de regressão em `tests/embedder.rs`

### v1.0.85.1 — Fallback Gracioso `--llm-backend none` em `recall`/`hybrid-search` (hotfix GAP-004)
- `--llm-backend none` agora retorna exit 0 com `vec_degraded: true` + `source: "fts_fallback"` + `vec_degraded_reason: "dim_zero"`
- Failsafe do v1.0.80 restaurado para o caso `--llm-backend none`
- Braço intermediário `Ok((v, _backend)) if v.is_empty() => Err(FallbackReason::DimZero)` em `try_embed_query_with_choice`

### v1.0.85.2 — `embed_via_backend` Resolved Kind, `--dry-run-backend` Standalone (BUG-001/002/003, ADR-0044)
- `--dry-run-backend` funciona standalone (sem subcommand) graças a `pub command: Option<Commands>` em `src/cli.rs:248`
- `embed_via_backend` retorna `Result<(Vec<f32>, LlmBackendKind), AppError>` propagando `resolved_kind`
- 7 envelopes agora reportam `backend_invoked: "claude" | "codex" | "none"` consistentemente
- `setup_mock_path()` em `tests/embedder.rs:37-77` alinhado para emitir JSON (não JSONL)

### v1.0.84 — Split do Backend Claude (ADR-0042, GAP-002)
- `--llm-backend claude` agora força invocação de `claude -p`, sem fallback silencioso para codex
- `LlmEmbeddingBuilder` em `src/extract/llm_embedding.rs` com `with_claude_builder`, `with_codex_builder`, `override_binary`, `override_model`
- `embed_via_claude_local` em `src/embedder.rs:190+` é o entry point do split real
- `apply_env_whitelist_for_claude` em `src/spawn/env_whitelist.rs` (compartilhado por `invoke_claude` e `embed_via_claude_local`)
- 5 testes de regressão em `tests/embedder.rs`

- **G56 cache de entity-embed em processo**: `embed_entity_texts_cached` fica na frente de `embed_passages_parallel_local` para batches de nome de entidade. Chave do cache é `blake3(model || "\0" || text)`. Taxa de hit alta em `ingest` (entidades canônicas re-embedadas entre muitas memórias), modesta em `remember` e `remember-batch`. `remember.rs`, `ingest.rs` e `remember_batch.rs` roteiam embeddings de entidade pelo cache; embeddings de chunk continuam no caminho raw. Stats são emitidas via `tracing::debug!` (contagens hit / miss / request).
- **G58 fallback FTS5 para `recall` e `hybrid-search`**: `recall --fallback-fts-only` e `hybrid-search --fallback-fts-only` roteiam a query via FTS5 BM25 quando o subprocesso LLM falha (rate limit, contenção OAuth, dim divergente). Os novos campos do envelope `vec_degraded` (bool), `vec_error` (string) e `warning` (string) são preenchidos simetricamente em ambos os comandos. Os testes de `recall` e `hybrid-search` ganharam cobertura para o caminho FTS5-only; 1 teste é `#[ignore]` porque o stub G58 S1 exige `PATH` sem `codex` ou `claude` para exercitar `EmbeddingFailed`.
- **G53-WINDOWS-INFRA (ADR-0033)**: os jobs `clippy` e `test` da matrix windows-2025 ganharam 2 steps novos cada (gateados `if: matrix.os == 'windows-2025'`, no-op em ubuntu/macos): um pre-warm que baixa o toolchain rustup no cache do runner antes do build, e um verify step que re-checa `rustup show active-toolchain` após install. Os 2 modos históricos de falha de infra (download do rustup com erros transitórios de rede e `E0463 can't find crate for core` quando a stdlib do target está ausente) agora são recuperáveis na primeira re-run em vez de acumularem como CI vermelho. Validação local de cross-compile: `cargo check --target x86_64-pc-windows-msvc --lib --all-features` reproduzido e o `E0463` resolvido via `rustup target add x86_64-pc-windows-msvc --toolchain 1.88`; o build então atinge a fronteira `cc-rs: failed to find tool "lib.exe"`, que é o limite esperado de cross-compile MSVC a partir de host Linux.
- **Resiliência de SHUTDOWN (ADR-0034)**: `src/signals.rs` é envolvido em uma barreira de captura de panic; mesmo quando o stderr do pai é um pipe fechado (o cenário de processo órfão que a auditoria G42/C2 identificou), o handler retorna limpo em vez de `SIGABRT`-ar em `BrokenPipe`. O terceiro Ctrl-C consecutivo sai com código 130 e ZERO I/O, casando com o contrato documentado em ADR-0034 e a receita em `docs/HEADLESS_INVOCATION.md`. A receita de bypass SHUTDOWN em 3 camadas (`nohup` então `setsid` então `disown`) é a referência canônica para o harness do agente ao rodar jobs longos de embedding em background.

## O Que Mudou na v1.0.79 (G42 + G43)

O trabalho do G42 tornou o pipeline de embedding rápido, paralelo e em lote; o G43 tornou universal a adoção da dimensionalidade:

- A dimensionalidade default de embedding caiu de 384 para 64 (configurável via `SQLITE_GRAPHRAG_EMBEDDING_DIM`, faixa [8, 4096]); bancos pré-existentes mantêm a `schema_meta.dim` registrada em todo comando (adoção em `open_rw`/`open_ro`, G43).
- Chamadas de embedding são em lote (`{items:[{i,v}]}`; chunks em 8, nomes de entidade em 25 em dim 64; adaptativos à dim — G44) e rodam em paralelo sob semáforo bounded: `--llm-parallelism` em `remember` (default 4), `ingest` (default 2) e `edit` (default 4), clamp [1, 32].
- `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` seleciona o modelo de embedding do claude; `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` (default 300) limita cada chamada LLM.
- `enrich --operation re-embed` e `edit --force-reembed` são os caminhos canônicos de re-embedding.
- O código restante do daemon foi deletado; as features `embedding-legacy` e `ner-legacy` foram removidas; `--enable-ner` é somente URL-regex e as flags da era GLiNER avisam como no-ops.


## O Que Mudou na v1.0.76

O build padrão agora é **apenas LLM e one-shot**. Não há modelo local de embedding, não há NER GLiNER, não há runtime ONNX, não há extensão C do `sqlite-vec`. Cada `remember`, `ingest`, `edit` spawna um subprocesso headless de LLM (CLI do claude code ou codex) que devolve o embedding e, opcionalmente, as entidades extraídas.

A CLI é one-shot: não há daemon, não há modelo a manter em memória, não há socket a limpar. O binário de release tem ~14.6 MiB (era 39 MB) e o cold start é 1-3 s (era 30 s com a carga do modelo ONNX).


## Pré-Requisitos

Você precisa de UMA destas CLIs instalada e no `PATH`:

- `claude` — CLI do Claude Code 2.1.0+
  ([instalação](https://docs.claude.com/claude-code))
- `codex` — CLI do OpenAI Codex 0.130.0+
  ([repositório](https://github.com/openai/codex))
- `opencode` — CLI do OpenCode (v1.0.90+)

`claude` e `codex` precisam estar logados com o **fluxo OAuth** (assinatura Claude Pro/Max ou ChatGPT Pro). `opencode` usa sistema de auth próprio.
Chaves de API NÃO são suportadas — veja a seção "Validação OAuth" abaixo.

Para verificar:

```bash
which claude || which codex
claude --version
codex --version
```


## Validação OAuth

A v1.0.76 herda o mandato OAuth-only da v1.0.69. Se `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estiverem definidas no ambiente, o spawn da LLM ABORTA com `AppError::Validation` e a CLI sai com código 1.

Para remover:

```bash
unset ANTHROPIC_API_KEY
unset OPENAI_API_KEY
```

As duas variáveis de chave de API também são excluídas da whitelist de env-clear, então não conseguem burlar a checagem mesmo quando definidas em um processo pai.


## Instalação

```bash
cargo install sqlite-graphrag --version 1.0.91 --force
```

Isso instala o build padrão LLM-only. Verifique:

```bash
sqlite-graphrag --version
# sqlite-graphrag 1.0.91
```

Para o pipeline legado fastembed (REMOVIDO na v1.0.79):

```bash
# REMOVIDO na v1.0.79: a feature embedding-legacy não existe mais.
# As versões 1.0.76-1.0.78 a aceitavam; fixe uma dessas versões se
# precisar do pipeline fastembed legado (sem suporte).
```


## Inicializar um Banco

```bash
sqlite-graphrag init --namespace meu-projeto
```

O comando `init`:

1. Cria `graphrag.sqlite` no diretório atual.
2. Roda todas as migrações incluindo V013 (dropa vec tables, cria `memory_embeddings`, `entity_embeddings`, `chunk_embeddings`).
3. Spawna a LLM uma vez para confirmar que a sessão OAuth é válida.
4. Reporta `schema_version: 15` no sucesso.

O primeiro `init` é lento (1-3 s de round-trip LLM). Chamadas subsequentes são no-ops (o schema já está na versão alvo).


## Persistir Sua Primeira Memória

```bash
sqlite-graphrag remember \
    --name decisao-auth-2026-06 \
    --type decision \
    --description "Estratégia de rotação de token JWT com expiração de 15 min" \
    --body "Escolhemos JWT com access token de 15 minutos e
    refresh token de 7 dias. O fluxo de refresh usa cookies HttpOnly.
    Veja https://auth0.com/docs/refresh-tokens para a especificação." \
    --entities-file entidades.json
```

Onde `entidades.json` é:

```json
[
  {"name": "JWT", "entity_type": "concept"},
  {"name": "Auth0", "entity_type": "tool"}
]
```

O comando `remember`:

1. Chama a LLM para embutir o corpo — em lote e em paralelo desde a v1.0.79 (`--llm-parallelism`, default 4; 1-3 s por chamada).
2. Armazena a memória em `memories` (indexada por FTS5).
3. Armazena o embedding como BLOB em `memory_embeddings`.
4. Liga as entidades via tabela `entities`.
5. Retorna JSON com `memory_id`, `version`, `elapsed_ms`.


## Buscar Memórias

Os dois comandos principais de busca são:

```bash
# Busca por token exato + semântica, fundida via RRF
sqlite-graphrag hybrid-search "design auth jwt" --k 10 --json

# Apenas semântica (sem componente FTS5)
sqlite-graphrag recall "design auth jwt" --k 5 --no-graph --json
```

Para o tamanho padrão de namespace (10k memórias ou menos), o refinamento por cosseno sobre o BLOB de embedding é rápido o suficiente (ms de dígito único). Para namespaces maiores, prefira `hybrid-search` para que o FTS5 faça a filtragem grossa.


## Extrair Entidades via LLM

O `remember` padrão faz apenas extração de URL. Para NER completo (entidades + relacionamentos tipados), use o backend LLM:

```bash
sqlite-graphrag remember \
    --name revisao-design-t2 \
    --type note \
    --description "Notas da revisão de design do T2" \
    --body "$(cat revisao-design.md)" \
    --extraction-backend llm
```

A LLM devolve JSON estruturado com entidades e relacionamentos no mesmo prompt que produz o embedding. O round-trip total é 3-8 s (mais longo que o caminho de só embedding porque o prompt inclui o schema e a resposta é maior).


## Ferramentas de Qualidade LLM (herdadas da v1.0.69)
### `enrich` — Qualidade do Grafo Aumentada por LLM
- O subcomando `enrich` executa operações de qualidade do grafo curadas por LLM. Três estão totalmente implementadas: `memory-bindings` (extrai entidades de memórias órfãs), `entity-descriptions` (preenche descrições de entidade NULL ou vazias) e `body-enrich` (expande corpos curtos de memória em conteúdo mais rico).
- Duas operações adicionais são apenas de varredura e exibem listas candidatas sem reescrever: `weight-calibrate`, `relation-reclassify`, `entity-connect`, `entity-type-validate`, `description-enrich`, `cross-domain-bridges`, `domain-classify`, `graph-audit`, `deep-research-synth`, `body-extract`.
- `--mode claude-code` ou `--mode codex` seleciona o provedor LLM. O padrão é `claude-code`. Ambos os provedores são OAuth-only desde a v1.0.69.
- `--preflight-check` emite um ping de 1 turno ANTES de varrer o conjunto candidato. Em rate limit OAuth do Claude, a sondagem aborta com erro claro (ou troca para `--fallback-mode` quando fornecido). Padrão desligado para manter `--dry-run` e fluxos de CI com custo zero.
- `--fallback-mode <claude-code|codex>` troca automaticamente de provedor quando a sondagem de preflight ou uma chamada em voo atinge rate limit. Ignorado quando `--mode` já é `codex`.
- `--rate-limit-buffer <SEGUNDOS>` padrão 300. Quando a sondagem detecta que o reset do rate limit OAuth está a menos do que o buffer de distância, aborta com sugestão para esperar.
- `--names <a,b,c>` e `--names-file <CAMINHO>` selecionam um subconjunto específico de nomes de memória em vez de varrer todos os candidatos. `--names-file` aceita comentários `#` e linhas em branco. As duas flags se combinam como união quando ambas estão setadas.
- `--preserve-threshold <FLOAT>` (padrão 0.7) controla o portão de similaridade trigrama Jaccard para `body-enrich`. Quando a reescrita do LLM pontua abaixo do threshold, o corpo enriquecido é REJEITADO e emitido como `EnrichItemResult::PreservationFailed`. Protege contra invenção do LLM.
- `--llm-parallelism <N>` spawna N threads de worker LLM em paralelo (padrão 1, máximo 32). Codex tolera até 16 em produção; Claude avisa acima de 4 por causa da fan-out OAuth-MCP. Desde a v1.0.79 a mesma flag também existe em `remember` (default 4), `ingest` (default 2) e `edit` (default 4) para o fan-out de embedding.
- `--max-load-check` recusa iniciar quando o load average de 1 minuto excede `2 × ncpus`. Defina como false em runners de CI disputados.
- `--circuit-breaker-threshold <N>` (padrão 5) aborta o job após N resultados `HardFailure` consecutivos. Erros transient de rate limit e timeout não contam.
- `--codex-model-validate` (padrão true) verifica `--codex-model` contra a lista de modelos aceitos pelo ChatGPT Pro OAuth ANTES de o subprocesso ser spawnado. Use `--codex-model-fallback <MODELO>` para auto-substituir um modelo conhecido em vez de abortar.
- `--dry-run` faz preview do conjunto candidato sem spawnar nenhum LLM. A saída é NDJSON com um evento por memória e um resumo final.
- `--resume` continua um batch interrompido anteriormente a partir do queue DB. `--retry-failed` retenta apenas os itens que falharam.
### `vec` — Manutenção do Índice Vetorial (G39)
- `vec orphan-list --json` lista linhas de embedding de memória cujo `memory_id` não existe mais na tabela `memories`. Cada linha reporta o `vector_hash` (BLAKE3 do blob de embedding) para rastreabilidade.
- `vec purge-orphan --yes --dry-run --json` faz preview da contagem de deleção sem remover nada.
- `vec purge-orphan --yes --json` purga as TRÊS vec tables (`vec_memories`, `vec_entities`, `vec_chunks`) em uma única transação implícita. A resposta reporta `deleted`, `deleted_entities`, `deleted_chunks` e `elapsed_ms`.
- `vec stats --json` expõe `vec_memories_rows`, `vec_entities_rows`, `vec_chunks_rows`, `orphans` e o timestamp do último vacuum. Use para auditar a saúde das vec tables após ciclos de `forget` em massa.
- O subcomando `forget` agora chama `memories::delete_vec` ANTES do soft-delete, prevenindo novos órfãos em estado estável.
### `codex-models` — Descobrir Modelos ChatGPT Pro OAuth (G33)
- `codex-models --json` retorna a lista de modelos aceitos, a contagem e o padrão. Atualmente: `codex-auto-review`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`.
- `codex-models --suggest <substring> --json` retorna a correspondência mais próxima via busca por substring com fallback Levenshtein. Útil quando um operador digita `o4-mini` e quer saber a alternativa aceita mais próxima.
### Endurecimento de `optimize` e `backup` (G36 + G38)
- `optimize` agora faz pré-verificação da saúde do FTS5 via `check_fts_functional` ANTES de reconstruir. Um índice saudável não é mais reconstruído (economiza ~10 minutos em um banco de 4.3 GB). Force a reconstrução com `--no-fts-skip-when-functional`.
- `optimize --fts-dry-run --json` sai com código 1 se o índice FTS5 precisar de reconstrução, 0 caso contrário. Amigável para CI.
- `optimize --fts-progress <N>` (padrão 30) emite uma linha de progresso a cada N segundos durante a reconstrução. Defina como 0 para desabilitar.
- `optimize --yes` pula o prompt de confirmação. Obrigatório para CI não interativo.
- `backup` usa por padrão `run_to_completion(1000, Duration::from_millis(5), None)` (era 100/50ms). Para um banco de 4.3 GB isso é um speedup de 25x (~21s vs ~9 min).
- `backup --backup-step-size <PAGES>` e `--backup-step-sleep-ms <MS>` ajustam a granularidade de cópia de páginas. `--backup-no-sleep` remove o sleep entre steps totalmente para máximo throughput. `--backup-progress <PAGES>` (padrão 100) emite uma linha de progresso a cada N páginas.
### Família de Subcomandos `migrate` (v1.0.76, atualizado v1.0.77 e v1.0.78)
- `migrate --rehash --json` reescreve os checksums registrados de migração para casar com o conteúdo atual do arquivo. Idempotente. Obrigatório para upgrades v1.0.74 → v1.0.76 onde a migração V002 foi intencionalmente esvaziada para um no-op.
- `migrate --to-llm-only --drop-vec-tables --json` é o upgrade one-shot para bancos v1.0.74 / v1.0.75. Combina `--rehash` com o descarte da V013 das vec tables. A flag `--drop-vec-tables` é OBRIGATÓRIA como rede de segurança explícita. As tabelas com backing BLOB `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` permanecem e são a fonte de verdade daqui em diante; embeddings são recomputados preguiçosamente no próximo `remember` / `edit` / `ingest`.
- Correção v1.0.77 (G40): a resposta JSON de ambos os comandos agora inclui `null_rows_fixed` (inteiro) e `vec_tables_removed_via_writable_schema` (inteiro). Bancos com linhas `applied_on = NULL` são sanitizados automaticamente antes do migration runner executar.
- Correção v1.0.78 (G41): a resposta JSON de ambos os comandos agora inclui `v013_tables_created` (boolean). Bancos onde V013 foi registrada em `refinery_schema_history` mas as tabelas BLOB-backed de embedding nunca foram criadas são reparados automaticamente. Qualquer comando CRUD também dispara esse reparo incondicionalmente via `ensure_db_ready`.


## Migração da v1.0.74 ou v1.0.75

Veja [MIGRATION.md](MIGRATION.md) para o passo a passo completo. A versão curta:

1. Instale a v1.0.76 (LLM-only).
2. Rode `sqlite-graphrag init` — a migração V013 roda automaticamente.
3. As vec tables antigas são dropadas; a nova `memory_embeddings` começa vazia.
4. As memórias são re-embutidas lazy no próximo `edit` ou `ingest`.

Para um corpus grande, use o loop one-shot canônico de re-embed (G42/S9, v1.0.79) — cada invocação processa um lote pequeno e encerra:

```bash
sqlite-graphrag enrich --operation re-embed --limit 5 --resume --json
```

Nota: a receita antiga `edit --description "<mesmo>"` nunca re-embedou nada (edições somente de descrição são no-op para embeddings); use `edit --force-reembed` para uma única memória.


## Ambiente de Teste em CI

Se você quer rodar a suíte completa de testes em CI, precisa de uma CLI de LLM no `PATH`. O build da v1.0.76 não embute via fastembed na configuração padrão, então `v1044_features`, `signal_handling_integration` e `v2_breaking_integration` vão falhar com `no LLM CLI found on PATH` quando nem `claude` nem `codex` estiverem instalados.

Soluções alternativas:

1. Instale `claude` na imagem de CI e autentique via OAuth (requere guardar tokens OAuth em segredos de CI).
2. Use uma CLI de LLM mock que devolve uma resposta JSON fixa para o prompt de embedding (usada internamente pelos testes unitários em `src/extract/llm_embedding.rs`).


## Veja Também

- [COOKBOOK.md](COOKBOOK.md) para receitas comuns
- [MIGRATION.md](MIGRATION.md) para upgrade v1.0.74 → v1.0.76
- [CROSS_PLATFORM.md](CROSS_PLATFORM.md) para Windows e macOS
- [AGENTS.md](AGENTS.md) para integração com agentes
- [HEADLESS_INVOCATION.md](HEADLESS_INVOCATION.md) para invocação headless OAuth-safe de Claude/Codex/OpenCode
- [decisions/](decisions/) para os 45 ADRs
