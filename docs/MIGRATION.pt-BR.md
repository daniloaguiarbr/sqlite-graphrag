# MIGRANDO PARA v1.0.85 — Remediação de Cinco Gaps + Split do Claude + Hotfixes (ADR-0042, ADR-0043, ADR-0044)

> Este guia é para operadores na v1.0.82 que querem atualizar para a v1.0.83 sem perder dados. Esta release é bump PATCH sem NENHUMA migração de banco. O schema permanece em v15. O comportamento é ADITIVO para operadores OAuth padrão.

## O Que Mudou na v1.0.83

- **GAP-058 resolução parcial (ADR-0041)** — seis variáveis de ambiente de provider customizado agora são preservadas ao spawnar subprocessos `claude -p` ou `codex exec`. Habilita providers compatíveis com Anthropic (Minimax/api.minimax.io, OpenRouter, AWS Bedrock, gateways corporativos) sem alterar o mandato OAuth-only que continua rejeitando `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`. As vars preservadas são `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY` e `OTEL_EXPORTER_OTLP_ENDPOINT`.
- **Helper compartilhado de whitelist** — a lógica duplicada de `env_clear` + re-injeção em `claude_runner.rs`, `codex_spawn.rs` e `ingest_claude.rs` é consolidada em `src/spawn/env_whitelist.rs`. Os três spawners delegam para `apply_env_whitelist(cmd, strict)` em vez de inlinear o array.
- **Flag opt-out de compliance** — `--strict-env-clear` / `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` ativa o modo estrito que preserva apenas `PATH`. Use em ambientes PCI-DSS, SOC2, HIPAA onde encaminhamento de credenciais via env vars é proibido por política. Sem esta flag, o padrão é encaminhar as seis vars de provider customizado junto com o guard OAuth-only.
- **Guard OAuth-only permanece intacto** — os quatro guards em `claude_runner.rs:273`, `codex_spawn.rs:259`, `ingest_claude.rs:282` e `extract/llm_embedding.rs:237-253` ainda abortam o spawn com `AppError::Validation` (exit 1) quando `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` estão setadas. A mensagem de erro agora aponta para `ANTHROPIC_AUTH_TOKEN` e `~/.codex/auth.json` como resoluções legítimas.
- **SEM telemetria** — o fix é silencioso. Nenhum novo `tracing::info!` registra qual provider o operador está usando. O teste de auditoria no-leak em `tests/claude_runner_env.rs` garante que o valor literal do token NUNCA aparece em stdout ou stderr mesmo com `RUST_LOG=trace`.
- **6 novos testes de regressão** — `tests/claude_runner_env.rs` cobre propagação de custom-provider, preservação do abort OAuth-only, herança de base-URL codex, drop de credenciais em modo estrito e auditoria no-leak. Todos com `#[serial_test::serial(env)]`.

## Quem É Afetado

- Todos os usuários da v1.0.82 rodando providers Anthropic-compatíveis customizados (Minimax, OpenRouter, AWS Bedrock, gateways corporativos) — antes tinham falhas de embedding com `exit 11` e `401 Invalid authentication credentials` no stderr (cenário G58 S5)
- Operadores OAuth padrão (Claude Pro/Max, ChatGPT Pro) NÃO são afetados — o guard rejeita `ANTHROPIC_API_KEY` e `OPENAI_API_KEY` identicamente à v1.0.82
- Operadores de host compartilhado com política estrita de credenciais devem setar `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` ANTES de rodar o novo binário para evitar encaminhar segredos inadvertidamente
- Consumidores da biblioteca veem UM símbolo público aditivo: `crate::spawn::env_whitelist::{apply_env_whitelist, is_strict_env_clear, PRESERVED_ENV_VARS}` — re-fixar em `=1.0.83`

## Distinção Semântica que o Fix Resolve

- `ANTHROPIC_API_KEY` — chave de API Anthropic paga (`sk-ant-...`), PROIBIDA pelo mandato OAuth-only do ADR-0011
- `ANTHROPIC_AUTH_TOKEN` — token OAuth usado pelo Claude Code com provider customizado, semanticamente distinto e agora PRESERVADO
- `OPENAI_API_KEY` — chave de API OpenAI paga, PROIBIDA
- `OPENAI_BASE_URL` — override de endpoint para providers OpenAI-compatíveis customizados, agora PRESERVADO
- `ANTHROPIC_BASE_URL` — override de endpoint para providers Anthropic-compatíveis customizados, agora PRESERVADO

O mandato da v1.0.69 estava correto ao rejeitar as vars de API paga; o whitelist env-clear era amplo demais e acidentalmente descartava as vars legítimas de provider customizado também. A v1.0.83 corrige a implementação preservando o invariante OAuth-only.

## MIGRANDO PARA v1.0.84 — Split do Backend Claude (ADR-0042, GAP-002)

Se você dependia de `--llm-backend claude` em v1.0.83 para forçar o entry point Claude, agora essa flag realmente funciona como documentado. Anteriormente era um sinônimo para codex (GAP-002). O split passa por `LlmEmbeddingBuilder` (novo em v1.0.84) e a nova função `embed_via_claude_local` em `src/embedder.rs:190+`. Use `--dry-run-backend` para verificar qual backend será invocado antes de qualquer chamada de embedding.

## MIGRANDO PARA v1.0.85 — Remediação de Cinco Gaps (ADR-0043)

O enum `FallbackReason` agora distingue 7 causas via `reason_code`: `embedding_failed | slot_exhausted | oauth_quota | backend_mismatch | dim_zero | cancelled | timeout`. Scripts que parseiam o campo `vec_degraded: bool` dos envelopes `recall` e `hybrid-search` devem ser atualizados para ler `vec_degraded_reason: Option<String>` para diagnósticos finos. O caminho `try_embed_query_with_deterministic_fallback` retenta em `OAuthQuota` e aplica um teto de 750ms em `SlotExhausted` antes de cair em modo FTS5-puro.

Os 12-14 headers HTTP `anthropic-ratelimit-*-remaining` retornados por `claude -p` agora são capturados por `LlmEmbedding::invoke_claude` (G45-CR5). Um valor `0` aborta o embed e dispara fallback para codex em vez de esperar pela ativação do circuit breaker.

A dimensionalidade default de embedding está travada em 64 (Matryoshka Representation Learning, arXiv 2205.13147). Bancos 384-dim pré-existentes continuam funcionando inalterados; bancos novos criados sob v1.0.85 consomem 6x menos tokens OAuth por chamada (G56).

## HOTFIX v1.0.85.1 — Fallback Gracioso `--llm-backend none` em `recall`/`hybrid-search` (GAP-004)

Se você passa `--llm-backend none` para `recall` ou `hybrid-search`, a resposta agora emite corretamente `vec_degraded: true` + `source: "fts_fallback"` + `vec_degraded_reason: "dim_zero"` e sai com exit 0. Antes do hotfix, o failsafe do v1.0.80 estava quebrado para essa escolha específica de backend. O fix vive em `src/embedder.rs:351` como braço intermediário `Ok((v, _backend)) if v.is_empty() => Err(FallbackReason::DimZero)`.

## HOTFIX v1.0.85.2 — `--dry-run-backend` Standalone + `embed_via_backend` Resolved Kind (ADR-0044)

`--dry-run-backend` agora funciona como flag standalone sem exigir subcommand. O fix é `pub command: Option<Commands>` em `src/cli.rs:248`. Chamar `sqlite-graphrag --llm-backend claude --dry-run-backend` sai com exit 0 e JSON `{action, backend, binary, model, flavour, chain, strict_env_clear}`.

`embed_via_backend` agora retorna `Result<(Vec<f32>, LlmBackendKind), AppError>` em vez de apenas `Result<Vec<f32>, AppError>`. O `resolved_kind` propaga para 7 envelopes (edit, embedding-status, enrich-summary, hybrid-search, ingest-summary, recall, remember) que agora reportam `backend_invoked: "claude" | "codex" | "none"` consistentemente.

## Como Atualizar

```bash
# 1. Backup antes do upgrade (recomendado, espelha o padrão da v1.0.82)
sqlite-graphrag backup --output /var/backups/graphrag-pre-v1-0-83.sqlite --json

# 2. Instalar a nova versão
cargo install sqlite-graphrag --version 1.0.83 --force
sqlite-graphrag --version   # deve reportar 1.0.83

# 3. SEM migração necessária — schema permanece em v15
sqlite-graphrag health --json | jaq '.schema_version'   # confirma 15

# 4. Para operadores Minimax (o cenário canônico deste fix)
export ANTHROPIC_AUTH_TOKEN="sk-cp-seu-token-minimax"
export ANTHROPIC_BASE_URL="https://api.minimax.io/anthropic"

# 5. Smoke test — valida que env de custom-provider propaga para o subprocesso
sqlite-graphrag remember \
  --name v183-smoke \
  --type note \
  --description "smoke test custom provider v1.0.83" \
  --body "se você consegue ler isto, o custom provider está conectado corretamente"

# 6. Verificar que o embedding foi gravado
sqlite-graphrag read --name v183-smoke --json | jaq '.body, .memory_id'
sqlite-graphrag health --json | jaq '.counts.memories'

# 7. Para hosts compartilhados com política estrita (compliance)
export SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1
# OU passar --strict-env-clear por invocação
sqlite-graphrag remember --name v183-strict --body "x" --strict-env-clear
```

## O Que Acontece Automaticamente

- Todos os comandos da v1.0.82 se comportam identicamente para operadores OAuth padrão — nenhuma flag precisa mudar
- As seis vars de custom-provider agora são encaminhadas SOMENTE quando setadas no ambiente do operador (sem habilitação manual necessária)
- O opt-out strict-mode é a única mudança acionável pelo operador; padrão permanece permissivo
- A mensagem de erro do guard OAuth-only agora referencia `ANTHROPIC_AUTH_TOKEN` e `~/.codex/auth.json` como resoluções legítimas quando um operador seta `ANTHROPIC_API_KEY` por engano
- Contagem de testes aumenta de 812 para 818 (6 novos testes seriais de env)

## Pinning da API da Biblioteca

Se você depende da API da lib, fixe na versão EXATA em `Cargo.toml`:

```toml
[dependencies]
sqlite-graphrag = "=1.0.83"
```

O atalho `^1.0` te mantém na trilha de estabilidade da CLI. O atalho `^1.0.83` permite 1.0.83..<1.1.0, o que pode incluir uma futura 1.0.84 com mudanças quebrantes na lib.

## O Que Quebra

- **NADA para operadores OAuth padrão** — comportamento idêntico à v1.0.82
- **Consumidores da biblioteca que enumeram o tamanho de `PRESERVED_ENV_VARS`** — o slice ganhou 4 entradas (`ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`); patterns não-exaustivos não são afetados
- **Operadores que dependiam de `ANTHROPIC_AUTH_TOKEN` ser descartado** — cenário improvável mas possível: a var agora chega ao subprocesso, o que pode alterar comportamento do lado do LLM. Use `--strict-env-clear` para restaurar a semântica da v1.0.82

## Cenários de Verificação

### Cenário A — Operador OAuth padrão (sem custom provider)

```bash
unset ANTHROPIC_AUTH_TOKEN ANTHROPIC_BASE_URL
sqlite-graphrag remember --name test-oauth-default --body "x"
# Esperado: exit 0, subscription OAuth usada, idêntico à v1.0.82
```

### Cenário B — Custom provider Minimax

```bash
export ANTHROPIC_AUTH_TOKEN="sk-cp-minimax-test"
export ANTHROPIC_BASE_URL="https://api.minimax.io/anthropic"
sqlite-graphrag remember --name test-minimax --body "x"
# Esperado: exit 0, custom provider roteado, sem 401 no stderr
```

### Cenário C — Abort OAuth-only preservado

```bash
unset ANTHROPIC_AUTH_TOKEN ANTHROPIC_BASE_URL
export ANTHROPIC_API_KEY="sk-ant-violation"
sqlite-graphrag remember --name test-oauth-abort --body "x"
# Esperado: exit 1, stderr menciona mandato OAuth-only e ANTHROPIC_AUTH_TOKEN como resolução
```

### Cenário D — Modo compliance estrito

```bash
export ANTHROPIC_AUTH_TOKEN="sk-cp-strict-test"
export SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1
sqlite-graphrag remember --name test-strict --body "x"
# Esperado: subprocesso recebe APENAS PATH; ANTHROPIC_AUTH_TOKEN NÃO é encaminhado
# Confirma postura de compliance: segredos ficam no processo pai
```

### Cenário E — Auditoria no-leak

```bash
export ANTHROPIC_AUTH_TOKEN="sk-cp-secret-value-XYZ-12345"
export RUST_LOG=trace
sqlite-graphrag remember --name test-no-leak --body "x" 2> /tmp/stderr.log
# Esperado: token literal NUNCA aparece em /tmp/stderr.log
# Validado por audit_no_token_leak_in_subprocess_stderr em tests/claude_runner_env.rs
```

## Rollback

Se a v1.0.83 não estiver funcionando para você:

```bash
cargo install sqlite-graphrag --version 1.0.82 --force
```

Seu banco está inalterado. A v1.0.83 não fez modificações de schema; a v1.0.82 lê o mesmo arquivo SQLite.

Para restaurar o comportamento da v1.0.82 em hosts compartilhados sem fazer rollback, setar `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` — apenas PATH será encaminhado.
# MIGRANDO PARA v1.0.80 — Política de Estabilidade, Infra Windows, Resiliência de SHUTDOWN

> Este guia é para operadores na v1.0.79 que querem atualizar para a v1.0.80 sem perder dados. Esta release é bump PATCH sem NENHUMA migração de banco.

## O Que Mudou na v1.0.80

- **Política de estabilidade declarada** (ADR-0032, G53): o contrato público é a CLI; a API da biblioteca é instável em v1.x.y. Consumidores da biblioteca devem fixar em `=1.0.80` e revisar CHANGELOG.md antes de bumpar
- **Job de CI `semver-checks`** adicionado em modo informativo (vira bloqueante em v1.0.81 quando as 9 violações MAJOR pendentes forem resolvidas)
- **G45 singleton de embedding cross-process** (follow-up do ADR-0032): `acquire_embedding_singleton` serializa chamadas de embedding LLM por par `(namespace, db)`; `--wait-embed-singleton SEGUNDOS` faz poll do lock; `AppError::EmbeddingSingletonLocked` é a nova variante estrutural (exit 75, retentável)
- **G55 S2 `MemoryNotFound` estrutural**: substitui o caminho legado `NotFound(String)` que mascarava qual alvo de lookup falhou; mensagens em pt-BR agora carregam nome e namespace explicitamente
- **G56 cache de entity-embed em processo**: `embed_entity_texts_cached` chaveado por `blake3(model || \0 || text)`; taxa de hit alta em `ingest`, modesta em `remember`/`remember-batch`
- **G58 fallback FTS5 de recall e hybrid-search**: `recall --fallback-fts-only` e `hybrid-search --fallback-fts-only` roteiam a query via FTS5 BM25 quando o subprocesso LLM falha; novos campos do envelope `vec_degraded`, `vec_error`, `warning` são preenchidos simetricamente
- **G53-WINDOWS-INFRA** (ADR-0033): os jobs da matrix windows-2025 ganharam steps de pre-warm e verify gateados em `if: matrix.os == windows-2025`. Os 2 modos históricos de falha de infra (download do rustup com erros transitórios de rede e `E0463 can't find crate for core` quando a stdlib do target está ausente) agora são recuperáveis na primeira re-run
- **Resiliência de SHUTDOWN** (ADR-0034): `src/signals.rs` é envolvido em uma barreira de captura de panic; o terceiro Ctrl-C consecutivo sai com código 130 e ZERO I/O, casando com a receita canônica de bypass SHUTDOWN em 3 camadas (`nohup` então `setsid` então `disown`)

## Quem É Afetado

- Todos os usuários da v1.0.79; as mudanças são todas aditivas no nível binário e de banco
- Consumidores da biblioteca (usuários do crate cargo, não da CLI) são FORTEMENTE aconselhados a fixar em `=1.0.80` porque a API da lib é instável dentro de v1.x.y
- Operadores multi-sessão (agentes concorrentes escrevendo no mesmo banco) se beneficiam do singleton G45 sem nenhuma ação

## Como Atualizar

```bash
cargo install sqlite-graphrag --version 1.0.80 --force
sqlite-graphrag --version   # deve reportar 1.0.80
```

NENHUMA migração de banco é necessária. O schema continua v13, a adoção de dim do G43 já roda em `open_rw` e `open_ro`, e as adições da API da biblioteca são todas ADITIVAS (nenhum re-export removido, nenhum campo renomeado, nenhuma assinatura alterada em 1.0.80).

## O Que Acontece Automaticamente

- Todos os comandos da v1.0.79 se comportam identicamente; as novas flags (`--wait-embed-singleton`, `--fallback-fts-only`, `--force-reembed` da v1.0.79) são opt-in
- Os steps de pre-warm do Windows são no-op em ubuntu e macos; só rodam em `matrix.os == windows-2025`
- O job de CI `semver-checks` é informativo na v1.0.80; ele reporta drift sem falhar o pipeline

## Pinning da API da Biblioteca

Se você depende da API da lib, fixe na versão EXATA em `Cargo.toml`:

```toml
[dependencies]
sqlite-graphrag = "=1.0.80"
```

O atalho `^1.0` te mantém na trilha de estabilidade da CLI. O atalho `^1.0.80` permite 1.0.80..<1.1.0, o que pode incluir uma futura 1.0.81 com mudanças quebrantes na lib. Para usuários da lib, o pin exato é mandatório.

## O Que Quebra

- **Consumidores da biblioteca que dependem de símbolos NÃO na superfície da lib 1.0.80**: nenhum adicionado além dos 6 documentados no CHANGELOG. Todos os 6 são aditivos
- **Workflows de CI que referenciam `windows-latest`**: esta release não altera a label do runner; a referência explícita `windows-2025` (adicionada na v1.0.73) continua sendo a escolha certa até a data de corte do redirect do VS2026 (2026-06-15)

## Rollback

Se a v1.0.80 não estiver funcionando para você:

```bash
cargo install sqlite-graphrag --version 1.0.79 --force
```

Seu banco está inalterado. A v1.0.80 não fez modificações de schema; a v1.0.79 lê o mesmo arquivo SQLite.


# MIGRANDO PARA v1.0.82 — Cinco Gaps Fechados, Duas Migrations, Quatro Subcomandos, Mitigação OAuth 401

> Este guia é para operadores na v1.0.80 ou v1.0.81 que querem atualizar para a v1.0.82 sem perder dados. Esta release é bump PATCH mas carrega DUAS migrations aditivas (V014 e V015) que rodam automaticamente no primeiro `init` ou `migrate`. A versão de schema avança de 13 para 15.

## O Que Mudou na v1.0.82

- **GAP-001 fechado (ADR-0036)** — fila de checkpoint do `remember` em três estágios. A tabela `pending_memories` (V014) guarda separadamente o body, as entidades e os relacionamentos; se um SIGTERM/SIGINT chega durante os estágios 2 ou 3, a linha fica no estado `queued` para reprocessamento posterior via `sqlite-graphrag pending list|show|cleanup`. Veja `docs/decisions/adr-0036-pending-memories-staging.md`.
- **GAP-002 fechado (ADR-0037)** — Envelope JSON de shutdown no exit code 19. Qualquer comando que spawna LLM e recebe SIGTERM, SIGINT ou SIGHUP agora emite um envelope JSON determinístico no stdout e sai com `SHUTDOWN_EXIT_CODE = 19`. Os campos do envelope `error`, `code`, `signal`, `graceful` e `message` são validados por `docs/schemas/shutdown-envelope.schema.json`.
- **GAP-003 fechado (ADR-0038)** — flag `--llm-backend` de escolha do usuário. Operadores podem passar `--llm-backend codex,claude,none` (ou qualquer subconjunto) para controlar a cadeia de backends tentada em ordem. O primeiro backend que não der erro vence; `none` como última entrada grava a memória com embedding NULL quando combinado com `--skip-embedding-on-failure`.
- **GAP-004 fechado (ADR-0039)** — Semáforo host-wide de slots LLM via `fs4 = "0.9"` com feature `sync`. Coordenação cross-process usa `fcntl(F_SETLK)` no Linux/macOS e `LockFileEx` no Windows. O padrão é `min(ncpus, oauth_tier_max)` (Pro=4, Max=8). Inspecione com `sqlite-graphrag slots status --json`; reapa órfãos com `sqlite-graphrag slots release --slot-id <N> --yes`. Combine com `--llm-max-host-concurrency N` para sobrescrever o teto padrão.
- **GAP-005 fechado (ADR-0040)** — Cadeia de fallback de captura de stderr para falhas de embedding. A tabela `pending_embeddings` (V015) guarda linhas que falharam em todos os backends da cadeia. A cadeia detecta `refresh_token_reused` (o incidente codex de 2026-06-14) e roteia para o próximo backend; se todos falharem, a linha é enfileirada para retry via `sqlite-graphrag pending-embeddings list|process`. A struct `LlmBackendError` ganhou 4 variantes (`Codex401`, `CodexRateLimit`, `ClaudeTimeout`, `Generic`) e `EXIT_CODE_HINTS` documenta 9 códigos.

## Quem É Afetado

- Todos os usuários da v1.0.80 e v1.0.81
- Operadores que rodam `codex exec` intensamente e tiveram HTTP 401 `refresh_token_reused` em 2026-06-14 — DEVEM rodar `codex login` após atualizar para refrescar o refresh token; a cadeia de fallback do GAP-005 mitiga mas não elimina o modo de falha
- Consumidores da biblioteca devem re-fixar em `=1.0.82`; as 4 novas superfícies de subcomando são aditivas mas o novo exit code 19 e a nova flag global `--llm-backend` são visíveis para consumidores de lib que enumeram `CommandKind`
- Workflows de CI: a whitelist `codex-models` agora inclui `gpt-5.5` como padrão; testes de CI que fixavam `gpt-4*`, `o4-mini` ou `gpt-5-codex` precisam migrar para o conjunto whitelisted

## Como Atualizar

```bash
# 1. Backup antes do upgrade (recomendado)
sqlite-graphrag backup --output /var/backups/graphrag-pre-v1-0-82.sqlite --json

# 2. Instalar a nova versão
cargo install sqlite-graphrag --version 1.0.82 --force
sqlite-graphrag --version   # deve reportar 1.0.82

# 3. Aplicar migrations V014 e V015 (automático, mas pode ser explícito)
sqlite-graphrag migrate --json

# 4. codex login OBRIGATÓRIO pós-upgrade (mitigação do incidente 2026-06-14)
codex login

# 5. Smoke test — valida que os subcomandos novos funcionam
sqlite-graphrag pending list --json
sqlite-graphrag slots status --json
sqlite-graphrag embedding status --json
sqlite-graphrag pending-embeddings list --json

# 6. Validar saúde geral
sqlite-graphrag health --json
```

## O Que Acontece Automaticamente

- `V014__pending_memories.sql` e `V015__pending_embeddings.sql` rodam na primeira invocação de `init` ou `migrate`; ambas usam `CREATE TABLE IF NOT EXISTS` então re-rodar é seguro
- A flag `--llm-backend` padroniza em `codex` se não definida; comportamento é idêntico ao da v1.0.81 para operadores que nunca setaram a flag
- O semáforo de slots é criado sob demanda em `${XDG_RUNTIME_DIR:-~/.local/share}/sqlite-graphrag/llm-slots/`; nenhuma ação do operador necessária
- O envelope JSON de shutdown substitui a antiga saída de "panic no terceiro Ctrl-C" (ADR-0034, v1.0.80) quando o sinal chega durante um subprocesso LLM; o exit 130 legado no terceiro sinal ainda vale para caminhos sem LLM
- A tabela `pending_embeddings` começa vazia; bancos v1.0.81 existentes têm zero linhas nela

## Fixação da API de Biblioteca

Se você depende da API de biblioteca, fixe na versão EXATA em `Cargo.toml`:

```toml
[dependencies]
sqlite-graphrag = "=1.0.82"
```

A forma curta `^1.0` mantém você na trilha de estabilidade da CLI. A forma curta `^1.0.82` permite 1.0.82..<1.1.0, que pode incluir uma futura 1.0.83 com mudanças breaking de lib. Para usuários de lib, o pin exato é mandatório.

## O Que Quebra

- **Consumidores de biblioteca que enumeram o enum `CommandKind`**: 4 novas variantes (`Pending`, `Slots`, `Embedding`, `PendingEmbeddings`) são anexadas; patterns não-exaustivos vão falhar ao compilar
- **Workflows de CI que referenciam `--llm-backend claude` ou `--llm-backend codex` como escolhas exclusivas**: a nova flag é uma cadeia separada por vírgula; invocações pré-v1.0.82 de `--llm-backend foo` agora falham a validação com exit 1 (backend único não pode conter vírgula; cadeia precisa conter ao menos um de `codex`, `claude`, `none`)
- **Pipelines shell que fazem grep em stderr por "panic"**: a mensagem de panic do terceiro Ctrl-C da v1.0.80 não aparece mais na v1.0.82; em vez disso um envelope JSON aparece no stdout no exit 19

## Rollback

Se a v1.0.82 não estiver funcionando para você:

```bash
cargo install sqlite-graphrag --version 1.0.81 --force
```

As duas novas migrations (V014, V015) NÃO são revertidas automaticamente no rollback. Se você precisa de um revert de schema real, restaure do backup pré-upgrade:

```bash
sqlite-graphrag --version  # confirma rollback para 1.0.81
cp /var/backups/graphrag-pre-v1-0-82.sqlite ./graphrag.sqlite
sqlite-graphrag health --json   # confirma schema_v13
```

AVISO: o binário v1.0.81 não vai entender as tabelas V014 e V015; elas serão ignoradas mas ainda presentes no arquivo. Um re-upgrade subsequente para v1.0.82 vai pulá-las via `CREATE TABLE IF NOT EXISTS`.


# MIGRAÇÃO PARA v1.0.78 — Correção do Registro Fantasma de V013 (G41)

## O Que Mudou

- `run_rehash` não insere mais linhas fantasma para migrações não aplicadas
- Novo helper `ensure_v013_tables_exist` repara bancos onde V013 foi registrada mas as tabelas nunca foram criadas
- Reparo automático roda incondicionalmente em `ensure_db_ready` — qualquer comando repara bancos corrompidos

## Quem É Afetado

- Usuários que rodaram `migrate --rehash` ou `migrate --to-llm-only --drop-vec-tables` na v1.0.76 ou v1.0.77
- Sintomas: `no such table: memory_embeddings` (exit 10) em `recall`, `hybrid-search`, `remember`

## Como Atualizar

```bash
cargo install sqlite-graphrag --version 1.0.78 --force
sqlite-graphrag migrate --rehash   # reparo explícito (opcional — qualquer comando repara automaticamente)
```

## O Que Acontece Automaticamente

- Qualquer comando CRUD (`remember`, `recall`, `hybrid-search`, etc.) detecta e repara o estado corrompido
- O helper `ensure_v013_tables_exist` verifica se V013 está em `refinery_schema_history` mas as tabelas BLOB-backed estão ausentes, e executa o SQL de V013 diretamente
- O SQL de V013 é idempotente (`CREATE TABLE IF NOT EXISTS`) — seguro para executar múltiplas vezes


# MIGRAÇÃO PARA v1.0.77 — Correção do G40

> Este guia é para operadores afetados pelo bug G40 da v1.0.76 onde `migrate --rehash` inseria linhas com `applied_on = NULL`

## O Que Mudou na v1.0.77

- Correção do INSERT em `run_rehash` que omitia o campo `applied_on`
- Sanitização automática de linhas com `applied_on = NULL` antes de rodar o migration runner
- Remoção de vec virtual tables via `PRAGMA writable_schema` quando o módulo `vec0` está ausente
- Correção do `debug-schema` que crashava em bancos com `applied_on = NULL`

## Quem É Afetado

- Operadores que rodaram `migrate --rehash` ou `migrate --to-llm-only` na v1.0.76
- Bancos que apresentam o erro `InvalidColumnType(Null at index: 2, name: applied_on)`
- Bancos v1.0.74 com vec virtual tables presentes

## Como Atualizar

```bash
cargo install sqlite-graphrag --version 1.0.77 --force
sqlite-graphrag migrate
```

- Nenhuma intervenção manual em SQL é necessária
- A v1.0.77 detecta e corrige automaticamente linhas com `applied_on = NULL`
- Vec virtual tables são removidas automaticamente via `writable_schema` se `vec0` estiver ausente


# MIGRAÇÃO PARA v1.0.76 — LLM-Only One-Shot

> Este guia é para operadores em v1.0.74 ou v1.0.75 que querem atualizar para v1.0.76 sem perder dados.

## O Que Mudou na v1.0.76

O build padrão agora é **apenas LLM e one-shot**:

- Geração de embedding: `claude code` (OAuth Anthropic) ou `codex` (OAuth OpenAI ChatGPT Pro), spawnado por chamada. Sem daemon. Sem runtime ONNX. Sem download de modelo.
- NER: o `LlmBackend` extrai entidades e relacionamentos via tool-use JSON. O `extract_graph_auto` padrão é apenas regex de URL; NER completo roda sob demanda com `--extraction-backend llm`.
- Busca vetorial: similaridade de cosseno em Rust puro sobre as tabelas BLOB-backed `memory_embeddings`, `entity_embeddings`, `chunk_embeddings`. A extensão C do `sqlite-vec` foi REMOVIDA.

## Pré-Requisitos

Você precisa de UMA destas no `PATH` depois do `cargo install`:

- `claude` — CLI do Claude Code 2.1.0+ ([docs](https://docs.claude.com/claude-code))
- `codex` — CLI do OpenAI Codex 0.130.0+
  ([repositório](https://github.com/openai/codex))

Ambas precisam estar logadas com o fluxo OAuth (assinatura Claude Pro/Max ou ChatGPT Pro). Chaves de API NÃO são suportadas e fazem o spawn ABORTAR com `AppError::Validation`.

Para verificar:

```bash
which claude || which codex
claude --version  # precisa reportar 2.1.0 ou superior
codex --version   # precisa reportar 0.130.0 ou superior
```

## Passo 1 — Instalar o Binário Atual (v1.0.79)

```bash
cargo install sqlite-graphrag --version 1.0.79 --force
```

Instale a v1.0.79 (não a 1.0.76): ela carrega os reparos de
migração G40/G41 e os fixes de embedding G42/G43 dos quais o
caminho de upgrade depende.

Isso instala o build padrão LLM-only (binário de ~6 MB, sem runtime ONNX, sem download de modelo). Se você quer o pipeline legado fastembed para a janela de transição:

```bash
cargo install sqlite-graphrag --version 1.0.76 --features embedding-legacy --force
```

A feature `embedding-legacy` foi REMOVIDA na v1.0.79 (antecipando o
cronograma da v1.1.0); o comando acima só funciona fixando 1.0.76-1.0.78.

## Passo 2 — Migrar o Banco Existente

A migração é automática no próximo `init`, `remember` ou `ingest`. A migração V013 dropa as virtual tables `vec_memories`, `vec_entities`, `vec_chunks` e cria as novas tabelas de embedding BLOB-backed. Memórias existentes são preservadas; seus embeddings são recomputados lazy na próxima escrita.

Para forçar uma migração explícita:

```bash
sqlite-graphrag init --force
```

A saída inclui `schema_version: 13` quando a migração completa. Bancos v1.0.74 ou v1.0.75 existentes reportarão `schema_version: 12` até `init` rodar.

### Comando Dedicado de Migração

A v1.0.76 introduz dois subcomandos novos para migração controlada:

```bash
# Recalcular checksums de migração para casar com o conteúdo atual
sqlite-graphrag migrate --rehash --json

# Upgrade one-shot para LLM-only (rehash + V013 + drop das vec tables)
sqlite-graphrag migrate --to-llm-only --drop-vec-tables --json
```

O `--drop-vec-tables` é uma guarda de segurança explícita: a CLI exige confirmação consciente antes de destruir dados. Use `--dry-run` antes para auditar.

## Passo 3 — Re-Embed (Opcional)

Se você tem um corpus grande, re-embede com o loop one-shot canônico (G42/S9, v1.0.79). Cada invocação processa um lote PEQUENO e ENCERRA, então o job sobrevive a qualquer janela de supervisor externo:

```bash
# Re-embedar memórias sem linha vetorial, 5 por invocação.
# Repita (loop externo) até o resumo reportar 0 itens completados.
sqlite-graphrag enrich --operation re-embed --limit 5 --resume --json
```

Para forçar UMA memória a re-embedar sem tocar no body, use `edit --force-reembed` (v1.0.79):

```bash
sqlite-graphrag edit --name minha-memoria --force-reembed
```

ATENÇÃO — a receita pré-v1.0.79 (`edit --description "rewarm embedding"`) estava ERRADA: edições somente de descrição pulam o re-embedding por design (v1.0.63) e deixam `memory_embeddings` intocada.

## Passo 4 — Verificar o Caminho LLM

Rode um único `remember` para confirmar que a LLM está cabeada corretamente:

```bash
sqlite-graphrag remember \
    --name smoke-test \
    --type note \
    --description "smoke test" \
    --body "se você consegue ler isso, a LLM está funcionando"
```

A primeira chamada leva 1-3 segundos (spawn de subprocesso LLM). Chamadas subsequentes no mesmo processo não são amortizadas (a CLI é one-shot), mas o lado da LLM pode fazer cache do modelo de embedding internamente.

## O Que Quebra em Bancos v1.0.74

| Comportamento v1.0.74 | Comportamento v1.0.76 |
| --- | --- |
| `sqlite-graphrag daemon` mantém o modelo de embedding em memória | `sqlite-graphrag daemon` foi totalmente removido na v1.0.76; cada chamada de embedding spawna um subprocesso LLM |
| `--enable-ner` dispara o loader GLiNER ONNX (~30s cold start, 1.1 GB de download de modelo) | `--enable-ner` dispara só regex de URL. Use `--extraction-backend llm` para obter NER completo via LLM. |
| `vec_memories`, `vec_entities`, `vec_chunks` são virtual tables sqlite-vec | `memory_embeddings`, `entity_embeddings`, `chunk_embeddings` são tabelas BLOB-backed regulares |
| Modelo fastembed: `multilingual-e5-small` (local, determinístico) | Modelo LLM: `claude-sonnet-4-6` (claude) ou `gpt-5.4` (codex) (round-trip de rede) |
| Primeiro `init` baixa 1.1 GB de pesos ONNX | Primeiro `init` faz um round-trip LLM de 1-3 s |
| Dimensionalidade de embedding fixa em 384 | Default 64 desde a v1.0.79, configurável via `SQLITE_GRAPHRAG_EMBEDDING_DIM` (faixa [8, 4096]); bancos migrados mantêm a 384 registrada em todo comando (G43) e continuam pesquisáveis; `enrich --operation re-embed` re-embeda na dim ativa |

## Rollback

Se a v1.0.76 não está funcionando para você, a escotilha de escape é:

```bash
cargo install sqlite-graphrag --version 1.0.75 --force
```

Seu banco v1.0.76 já foi migrado para o novo schema (a migração V013 rodou no primeiro `init`). Reverter para v1.0.75 vai exigir `init --force` para recriar as vec tables — você vai perder os embeddings que construiu na v1.0.76 a menos que faça dump antes.

Para dumpar os embeddings da v1.0.76 antes do rollback:

```bash
sqlite3 graphrag.sqlite "SELECT memory_id, embedding FROM memory_embeddings" > embeddings-v1076.json
```

Depois de reinstalar a v1.0.75, você pode reimportar os embeddings rodando `init --force` da v1.0.75 e depois um `ingest` em lote dos corpos de memória originais. O pipeline fastembed da v1.0.75 vai re-embutir tudo do zero.

## Features Removidas

| Feature | Removida em | Substituta |
| --- | --- | --- |
| `--enable-ner` (GLiNER ONNX) | padrão v1.0.76 | `--extraction-backend llm` |
| `vec_memories` / `vec_entities` / `vec_chunks` (sqlite-vec) | v1.0.76 | `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` (BLOB) |
| `daemon` (infraestrutura totalmente removida) | v1.0.76 | Nenhuma — o subprocesso LLM é o novo "carregador de modelo" |
| Variáveis `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` | v1.0.69 (ainda aplicadas) | OAuth via `claude login` / `codex login` |
