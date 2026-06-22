# Gaps — sqlite-graphrag v1.0.89


## GAP-RECALL-001 — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: CRÍTICA
- Afeta: `recall`, `hybrid-search`, `deep-research`
- Versão afetada: v1.0.88
- Versão corrigida: v1.0.89
- Ambiente: Linux Fedora 44, codex-cli 0.141.0, Claude Code 2.1.185
- ADR: ADR-0050
- Correções aplicadas: FIX-1 (drop stdin explícito), FIX-2 (timeout 300→30s), FIX-3 (limpeza stale slots), FIX-4 (reaper limpa slots), FIX-5 (reaper mata sqlite-graphrag órfãos), FIX-6 (telemetria slots no health), FIX-7 (testes de regressão)

## Problema
- `sqlite-graphrag recall` e `hybrid-search` travam indefinidamente no passo "Calculando embedding da consulta..."
- O processo nunca produz saída JSON no stdout
- O timeout interno de embedding é 300s (5 minutos), mas o subprocesso LLM nunca completa
- Múltiplas sessões Claude Code executando `hybrid-search` simultaneamente criam processos pendurados que saturam o semáforo host-wide de slots LLM
- Novos comandos `recall`/`hybrid-search` ficam na fila de espera indefinidamente
- Comandos sem LLM (`list`, `read`, `graph stats`, `fts stats`) completam em 11-22ms — o banco está saudável

## Consequências do Problema
- Qualquer comando que dependa de busca semântica (`recall`, `hybrid-search`, `deep-research`) fica inacessível
- O pipeline de memória GraphRAG fica limitado a comandos read-only textuais (`list`, `read`, `graph entities`)
- Múltiplas sessões Claude Code competindo pelo mesmo banco acumulam processos pendurados que NUNCA liberam os slots LLM
- O deadlock é AUTO-PERPETUANTE: processos pendurados seguram slots → novos processos esperam slots → ninguém libera → acúmulo crescente
- O sistema se torna progressivamente mais lento até necessitar intervenção manual (kill de processos)
- A busca textual FTS5 funciona (`fts stats` retorna `fts_functional: true`), mas não é acessível isoladamente sem o embedding vetorial

## Causa Raiz — Cadeia de Causa e Efeito
- CAUSA 1 (raiz): O subprocesso LLM (`codex exec` ou `claude -p`) spawnado para gerar embedding da query trava ou nunca retorna uma resposta
- CAUSA 2 (contribuinte): O `.mcp.json` em `/home/comandoaguiar/Dropbox/ai/.mcp.json` contém `mcpServers.docs-rs` ativo, o que causa interferência no walk-up do preflight em cada invocação
- CAUSA 3 (amplificadora): Múltiplas sessões Claude Code (instâncias 01, 02, 03, 05) executam `hybrid-search` simultaneamente, cada uma spawnando subprocessos LLM que travam
- CAUSA 4 (deadlock): O semáforo host-wide de slots LLM (`acquire_llm_slot_for_embedding`) fica saturado com processos que NUNCA liberam o slot
- CAUSA 5 (propagação): O timeout interno de embedding é 300s, mas o processo pai (`sqlite-graphrag`) recebe SIGINT/SIGTERM do shell antes desse timeout, gerando exit 19 (shutdown) sem cleanup do subprocesso filho

## Evidência Diagnóstica Coletada
- `health --json`: `integrity_ok: true`, `schema_version: 15`, `fts_query_ok: true`, 1233 memórias, 9528 entidades
- `vec_memories: 1232` vs `memories: 1233` — 1 memória sem embedding vetorial
- `list`, `read`, `graph stats`, `fts stats` completam em 11-22ms (banco saudável)
- `recall "test" --k 1` com SKIP_PREFLIGHT, IGNORE_SHUTDOWN, CLAUDE_CONFIG_DIR vazio — trava 90s+ sem saída
- `codex exec` chamado DIRETAMENTE com stdin + `--output-schema` — retorna embedding de 64 dims em 2-3 segundos
- `procs --tree` mostra 3 processos `sqlite-graphrag hybrid-search` de outras sessões com filhos `ctrl-c` (signal handler) mas SEM subprocessos `codex exec` ou `claude -p` ativos
- O stderr mostra a sequência: `cli slot acquire (wait_secs=300)` → `recall: searching` → `Calculando embedding da consulta...` → silêncio indefinido

## Solução Proposta
- CORRIGIR a lógica de timeout e cleanup do subprocesso LLM em `invoke_codex` e `invoke_claude` para garantir liberação do slot LLM
- IMPLEMENTAR watchdog no `embed_passage`/`embed_query` que detecta subprocesso filho morto sem resposta e libera o slot LLM
- ADICIONAR coleta de orfãos de subprocessos LLM no startup (similar ao `reaper.rs` que já existe para `claude`/`codex` órfãos com PPID=1)
- ADICIONAR flag `--fallback-fts-only` como padrão quando detectar saturação de slots LLM (já existe no código mas NÃO é ativada automaticamente)
- REDUZIR o `DEFAULT_EMBED_TIMEOUT_SECS` de 300 para 60 segundos para queries simples (embedding de query curta não deveria levar 5 minutos)
- IMPLEMENTAR circuit breaker no `try_embed_query_with_deterministic_fallback` que conta tentativas consecutivas falhadas e degrada para FTS5-only automaticamente

## Benefícios da Solução
- Eliminação do deadlock auto-perpetuante por processos pendurados
- Recuperação automática de slots LLM quando subprocessos morrem sem resposta
- Busca semântica funcional em ambiente multi-sessão Claude Code
- Redução da latência de fallback de 300s para 60s (timeout menor)
- Degradação graciosa automática para FTS5-only quando LLM está indisponível

## Como Solucionar — Etapas Ordenadas
- Etapa 1: Auditar `invoke_codex` e `invoke_claude` em `src/extract/llm_embedding.rs` para garantir que o `drop` do `ChildStdin` fecha o fd antes do `wait_with_output`
- Etapa 2: Adicionar `drop(stdin)` explícito após `write_all` no bloco `if let Some(mut stdin)` (linha 726-731) antes de chamar `child.wait_with_output()`
- Etapa 3: Implementar verificação de processo filho vivo via `child.try_wait()` antes de entrar no `tokio::time::timeout` de 300s
- Etapa 4: Reduzir `DEFAULT_EMBED_TIMEOUT_SECS` para 60 (queries de embedding são curtas)
- Etapa 5: Adicionar auto-degradação para FTS5-only quando `acquire_llm_slot_for_embedding` retorna `SlotExhausted` após backoff de 750ms (já existe parcialmente no código)
- Etapa 6: Implementar reaper de subprocessos LLM órfãos no startup de `main.rs` (expandir o existente `reaper.rs`)
- Etapa 8: Testes de integração que simulam multi-sessão com slots esgotados

## Causa e Efeito — Diagrama
- `.mcp.json` com MCP servers ativos → preflight walk-up detecta → interferência no spawn do subprocesso LLM
- Subprocesso LLM trava ou morre silenciosamente → slot LLM NÃO é liberado → próximo `recall`/`hybrid-search` espera na fila
- Múltiplas sessões Claude Code → múltiplos processos pendurados → todos os slots LLM esgotados → deadlock sistêmico
- Timeout externo (15-90s) mata o processo pai com SIGINT → exit 19 (shutdown) → mas subprocessos filhos podem sobreviver como orfãos → acúmulo de orfãos
- Operador observa "failed to parse: value expected" quando tenta parsear a saída vazia do recall com `jaq` → erro de parsing é SINTOMA, não causa raiz


## GAP-DEEPRESEARCH-001 — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `deep-research`
- Versão corrigida: v1.0.89

## Problema
- `deep-research` chamava `embed_query_local()` (hard-fail exit 11) em `src/commands/deep_research.rs:301`
- `recall` e `hybrid-search` chamam `try_embed_query_with_deterministic_fallback()` que degrada graciosamente para FTS5
- Quando LLM indisponível, `deep-research` retornava exit 11 sem resultados

## Correção Aplicada
- Substituído `embed_query_local` por `try_embed_query_with_deterministic_fallback` no loop de sub-queries
- `execute_sub_query` aceita `Option<&[f32]>` — pula KNN e usa FTS5-only quando embedding indisponível
- Entity KNN também aceita Option — pula seed por entidade quando embedding indisponível
- Campo `vec_degraded` adicionado ao `ResearchStats` para telemetria de degradação
- Fusão RRF continua funcionando com KNN vazio (apenas FTS5 scores)


## GAP-JSON-FLAG-001 — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: BAIXA
- Afeta: `pending list`, `embedding status`, `slots status`, `pending-embeddings list`, `pending-embeddings abandon`
- Versão corrigida: v1.0.89

## Correção Aplicada
- Adicionado `#[arg(long, hide = true)] pub json: bool` em `PendingListArgs`, `EmbeddingStatusArgs`, `EmbeddingListArgs`, `EmbeddingAbandonArgs`, `SlotsStatusArgs`, `PendingEmbeddingsListArgs`, `PendingEmbeddingsAbandonArgs`
- Campo `hide = true` evita poluir a saída do `--help`
- O valor nunca é lido — existe apenas para que clap aceite `--json` sem exit 2
- Auditoria e2e de 2026-06-21 confirmou 16/16 subcomandos aceitam `--json` com exit 0


## GAP-INIT-EMBEDDING-001 — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: MÉDIA
- Afeta: `init`
- Versão corrigida: v1.0.89

## Correção Aplicada
- `init` agora captura falha de `embed_passage_local` em `match` em vez de propagar com `?`
- Quando embedding falha: `dim` vem de `crate::constants::embedding_dim()`, `status` retorna `"ok_no_embedding"`
- Schema, tabelas, FTS5 e schema_meta são criados normalmente sem LLM
- `init` SEMPRE retorna exit 0 — a ausência de LLM é warning, não erro

===


## GAP-CODEX-BINARY — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: MÉDIA
- Afeta: seleção de backend LLM para embedding

## Problema
- `--claude-binary` existia como flag global mas `--codex-binary` NÃO
- Assimetria impedia override do PATH do codex via flag CLI

## Correção Aplicada
- Adicionada flag `--codex-binary` em `src/cli.rs` com env var `SQLITE_GRAPHRAG_CODEX_BINARY`
- `detect_available()` em `llm_embedding.rs` agora honra `SQLITE_GRAPHRAG_CODEX_BINARY`


## GAP-FLAGS-MORTAS — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: CRÍTICA
- Afeta: 6 flags globais LLM (`--llm-model`, `--llm-fallback`, `--skip-embedding-on-failure`, `--claude-binary`, `--llm-max-host-concurrency`, `--llm-slot-wait-secs`, `--llm-slot-no-wait`)

## Problema
- Clap populava os campos do struct Cli via CLI flag ou env var como fallback
- MAS os módulos internos liam via `std::env::var()` diretamente
- Clap NÃO seta a env var quando a flag é passada via CLI (apenas lê como fallback)
- RESULTADO: flags passadas via CLI eram SILENCIOSAMENTE IGNORADAS

## Correção Aplicada
- Adicionado bloco de propagação no `main.rs` que seta as env vars correspondentes via `std::env::set_var()` ANTES do dispatch de comandos
- Todas as 7 flags agora propagam para os módulos internos


## GAP-BACKEND-PROPAGATION — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `deep-research`, `remember-batch`

## Problema
- `deep-research` e `remember-batch` NÃO recebiam `cli.llm_backend` no `main.rs`
- `--llm-backend claude` era silenciosamente ignorado por esses 2 comandos

## Correção Aplicada
- Propagado `cli.llm_backend` para ambos os comandos no `main.rs`
- Assinaturas de `run()` atualizadas para aceitar `LlmBackendChoice`


## GAP-ADAPTIVE-TIMEOUT — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `remember` com corpos grandes e múltiplos chunks

## Problema
- `embed_timeout()` retornava o mesmo Duration (60s) para 1 chunk e para 50 chunks
- Corpos grandes geravam timeout falso porque múltiplos embeddings numa chamada de batch excediam 60s

## Correção Aplicada
- Adicionada `embed_timeout_for_batch(batch_size)` que escala: base + 15s por item adicional
- `embed_batch_async()` agora usa timeout adaptativo via env var temporária
- Batch de 1 item = 60s; batch de 8 items = 60 + 105 = 165s


## GAP-OAUTH-HINT — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: BAIXA
- Afeta: `invoke_claude()` quando OAuth expirado

## Correção Aplicada
- Detecta padrões de OAuth expirado no stderr ("401", "Unauthorized", "expired", "login")
- Adiciona hint acionável: "Claude OAuth token may be expired; run `claude login` to renew"


## GAP-MODEL-HARDCODE — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: seleção de modelo LLM para embedding

## Problema
- `codex_embed_model()` hardcodava "gpt-5.5" como default
- `claude_embed_model()` hardcodava "claude-sonnet-4-6" como default
- `--llm-model` flag leia env var `SQLITE_GRAPHRAG_LLM_MODEL` mas as funções internas liam env vars DIFERENTES (`SQLITE_GRAPHRAG_CODEX_EMBED_MODEL` / `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL`)

## Correção Aplicada
- Removidos defaults hardcoded de modelo
- `codex_embed_model()` e `claude_embed_model()` agora consultam `SQLITE_GRAPHRAG_LLM_MODEL` como fallback
- Se nenhum modelo é especificado, emite warning com instruções para o usuário
- O usuário DEVE selecionar o modelo via `--llm-model`, `SQLITE_GRAPHRAG_LLM_MODEL`, `SQLITE_GRAPHRAG_CODEX_EMBED_MODEL`, ou `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL`


## GAP-META-006 — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: CRÍTICA
- Afeta: TODOS os comandos que invocam LLM headless (`remember`, `edit`, `recall`, `hybrid-search`, `deep-research`, `ingest`, `enrich`, `remember-batch`, `init`, `restore`, `rename-entity`)
- Versão afetada: v1.0.88
- Versão corrigida: v1.0.89

## Problema
- O sqlite-graphrag hardcoda `codex` como CLI headless padrão em MÚLTIPLOS pontos do código-fonte
- O `LlmExtractorConfig::default()` em `src/extract/llm_backend.rs:27` hardcoda `backend: "codex".to_string()`
- O `default_backend()` em `src/extract/composite_backend.rs:124` SEMPRE chama `LlmBackend::with_default_codex()`
- O `backend_from_kind()` em `src/extract/composite_backend.rs:131` SEMPRE usa `with_default_codex()` para o tipo `BackendKind::Llm`
- O `detect_available()` em `src/extract/llm_embedding.rs:293` SEMPRE tenta `codex` primeiro via `which::which("codex")`
- O `LlmBackendChoice::Auto` em `src/cli.rs:52` resolve para `[Codex, Claude, None]` — codex é SEMPRE o primeiro da cadeia
- O usuário NÃO tem mecanismo na CLI para DESCOBRIR quais modelos estão disponíveis em cada backend
- Quando NENHUM modelo é especificado, as funções `codex_embed_model()` e `claude_embed_model()` retornam `String::new()` (string vazia) — o subprocesso LLM recebe um modelo vazio e falha com erro críptico
- O `--llm-backend` aceita `auto` como default, mas `auto` é SINÔNIMO de "codex-first" — o usuário que quer claude DEVE saber que precisa passar `--llm-backend claude` explicitamente

## Consequências do Problema
- O usuário NÃO controla qual CLI headless é usada sem conhecer a flag `--llm-backend`
- O `Auto` parece neutro mas é PARCIAL — favorece codex em 100% dos casos quando ambos estão no PATH
- Quando codex está indisponível (rate limit, OAuth expirado), o fallback para claude acontece SILENCIOSAMENTE sem informar o usuário
- Quando NENHUM modelo é especificado, o usuário recebe um erro críptico como "Model metadata for `` not found" em vez de uma mensagem explicativa
- O `composite_backend.rs` IGNORA completamente o `--llm-backend` do usuário — usa `with_default_codex()` hardcoded
- Em ambientes onde APENAS claude está instalado, o `detect_available()` funciona (fallback), mas `LlmExtractorConfig::default()` e `composite_backend.rs` AINDA referenciam "codex" nos metadados
- O usuário NÃO sabe quais modelos estão disponíveis para escolher — precisa consultar documentação externa
- A falta de seleção explícita gera desperdício de tokens: codex carrega ~11K tokens de system context quando claude pode ser mais leve para a tarefa

## Causa Raiz — Cadeia de Causa e Efeito

```
RAIZ 1: Decisão de design da v1.0.76 de hardcodar codex como padrão
  ↓ LlmExtractorConfig::default() seta backend = "codex"
  ↓ composite_backend::default_backend() chama with_default_codex()
  ↓ composite_backend::backend_from_kind(Llm) chama with_default_codex()
  ↓ RESULTADO: 3 pontos de hardcode no backend de extração

RAIZ 2: detect_available() implementa "codex-first" sem opt-out
  ↓ Tenta which::which("codex") ANTES de which::which("claude")
  ↓ Se codex existe no PATH, SEMPRE retorna codex
  ↓ O usuário NÃO tem flag para inverter a prioridade
  ↓ RESULTADO: seleção de CLI implícita, não explícita

RAIZ 3: Ausência de mecanismo de descoberta de modelos
  ↓ codex CLI NÃO tem subcomando `list-models` no modo headless
  ↓ claude CLI NÃO tem subcomando `list-models` no modo headless
  ↓ Codex: modelos listáveis APENAS via curl api.openai.com/v1/models (requer OPENAI_API_KEY)
  ↓ Codex: modelos listáveis via /model no TUI interativo (NÃO headless)
  ↓ Claude Code: modelos listáveis via /model no TUI interativo (NÃO headless)
  ↓ Claude API: modelos listáveis via GET api.anthropic.com/v1/models (requer API key)
  ↓ O sqlite-graphrag é OAuth-only — API keys são PROIBIDAS
  ↓ RESULTADO: o usuário não tem caminho programático para descobrir modelos

RAIZ 4: String vazia como fallback silencioso
  ↓ codex_embed_model() retorna String::new() quando nenhum modelo é configurado
  ↓ claude_embed_model() retorna String::new() quando nenhum modelo é configurado
  ↓ O subprocesso LLM recebe --model "" (string vazia)
  ↓ Codex: "Model metadata for `` not found" (exit 1)
  ↓ Claude: comportamento indefinido
  ↓ RESULTADO: falha críptica em vez de mensagem explicativa
```

## Solução Proposta
- REMOVER o default `Auto` do `--llm-backend` — tornar a seleção OBRIGATÓRIA ou validar com mensagem clara
- ADICIONAR validação no startup: se `--llm-model` NÃO está definido E nenhuma env var de modelo está configurada, ABORTAR com exit 1 e mensagem explicativa listando as opções
- REMOVER hardcodes de "codex" em `composite_backend.rs` — usar o `--llm-backend` do usuário para resolver o backend
- PROPAGAR `LlmBackendChoice` para `LlmExtractorConfig` e `composite_backend` em vez de hardcodar
- ADICIONAR subcomando `sqlite-graphrag models --json` que lista modelos conhecidos por backend
- DOCUMENTAR como o usuário lista modelos disponíveis em cada backend

## Como o Usuário Lista Modelos Disponíveis

### Codex CLI (OpenAI)
- No TUI interativo: digitar `/model` para abrir o seletor de modelos
- Via API (requer OPENAI_API_KEY): `curl -s -H "Authorization: Bearer $OPENAI_API_KEY" https://api.openai.com/v1/models | jaq -r '.data[].id' | sort`
- Via flag: `codex --model <model-id>` ou `codex -m <model-id>`
- Via config: `model = "gpt-5.4"` em `~/.codex/config.toml`
- Via env var: NÃO há env var nativa para modelo no codex CLI

### Claude Code CLI (Anthropic)
- No TUI interativo: digitar `/model` para abrir o seletor de modelos
- Via API (requer ANTHROPIC_API_KEY): `curl -s -H "x-api-key: $ANTHROPIC_API_KEY" -H "anthropic-version: 2023-06-01" https://api.anthropic.com/v1/models | jaq -r '.data[].id' | sort`
- Modelos com 1M tokens: adicionar sufixo `[1m]` — ex: `claude-opus-4-6[1m]`, `claude-sonnet-4-6[1m]`
- Via flag: `claude --model <alias-ou-id>`
- Via env var: `ANTHROPIC_MODEL=<alias-ou-id>`
- Via settings: campo `"model": "opus"` em settings.json

## Benefícios da Solução
- O usuário CONTROLA explicitamente qual CLI headless e qual modelo é usado
- ZERO hardcodes de backend ou modelo no código-fonte
- Mensagens de erro ACIONÁVEIS quando nenhum modelo é configurado
- Subcomando `models` permite descoberta programática de modelos
- Eliminação de desperdício de tokens por seleção implícita de backend inadequado
- Transparência: o usuário SABE qual CLI e modelo está sendo usado em cada operação

## Como Solucionar — Etapas Ordenadas
- Etapa 1: Substituir `LlmExtractorConfig::default()` para NÃO hardcodar `"codex"` — usar `LlmBackendChoice` do CLI
- Etapa 2: Refatorar `composite_backend::default_backend()` e `backend_from_kind()` para aceitar `LlmBackendChoice` como parâmetro
- Etapa 3: Propagar `cli.llm_backend` para TODO ponto que chama `with_default_codex()` ou `with_default_claude()`
- Etapa 4: Adicionar validação no startup: se modelo vazio, emitir mensagem explicativa com lista de opções e exit 1
- Etapa 5: Adicionar subcomando `sqlite-graphrag models --json` que emite modelos conhecidos por backend
- Etapa 6: Documentar o fluxo de seleção no `--help` de cada subcomando que usa LLM
- Etapa 7: Adicionar testes de regressão que verificam: (a) backend não é hardcoded, (b) modelo vazio gera erro acionável, (c) `--llm-backend claude` é honrado em TODOS os comandos

## Causa e Efeito — Diagrama

```
Hardcode "codex" em 5 pontos do código → usuário sem controle do backend
  ↓
detect_available() codex-first → claude NUNCA é selecionado quando codex existe
  ↓
Modelo vazio (String::new()) → erro críptico "Model metadata for `` not found"
  ↓
Sem subcomando list-models → usuário não sabe quais modelos existem
  ↓
Codex rate-limited + Claude OAuth expirado → ZERO backends funcionais, erro genérico
  ↓
IMPACTO: pipeline de memória GraphRAG INOPERANTE até intervenção manual
```

## Correção Aplicada (v1.0.89)
- `src/extract/llm_backend.rs` — `LlmExtractorConfig::default()` agora usa `detect_available_backend()` em vez de hardcodar "codex"
- `src/extract/composite_backend.rs` — `default_backend()` e `backend_from_kind()` agora resolvem via `detect_available_backend()` em vez de chamar `with_default_codex()`
- `src/commands/remember_batch.rs` — `_llm_backend` renomeado para `llm_backend` e propagado para `embed_passage_with_choice()` e `process_line()`
- `src/commands/deep_research.rs` — `_llm_backend` renomeado para `llm_backend` e propagado para `run_async()` e `try_embed_query_with_deterministic_fallback()`
- `src/extract/llm_backend.rs:239` — `cfg.backend = "codex"` MANTIDO intencionalmente: pertence ao `CodexFactory` que DEVE hardcodar "codex" (é a fábrica do codex)
- `src/extract/llm_backend.rs:267` — `cfg.backend = "claude"` MANTIDO intencionalmente: pertence ao `ClaudeFactory` que DEVE hardcodar "claude" (é a fábrica do claude)
- `src/cli.rs:167` — `Auto` MANTIDO como default mas agora honrado por `detect_available_backend()` que resolve dinamicamente em vez de favorecer codex incondicionalmente

## Pontos de Hardcode Eliminados (Inventário)
- `src/extract/llm_backend.rs:27` — `backend: "codex".to_string()` ELIMINADO (agora usa `detect_available_backend()`)
- `src/extract/composite_backend.rs:124` — `LlmBackend::with_default_codex()` ELIMINADO (agora usa `default_backend()` com resolução dinâmica)
- `src/extract/composite_backend.rs:131` — `LlmBackend::with_default_codex()` ELIMINADO (reutiliza `default_backend()`)
- `src/extract/composite_backend.rs:135` — `LlmBackend::with_default_codex()` ELIMINADO (reutiliza `default_backend()`)
- `src/commands/remember_batch.rs:82` — `_llm_backend` ELIMINADO (agora usado e propagado)
- `src/commands/deep_research.rs:251` — `_llm_backend` ELIMINADO (agora usado e propagado)


## GAP-LATENCY-001 — DOCUMENTAÇÃO (NÃO é bug do sqlite-graphrag)
- Data de identificação: 2026-06-21
- Severidade: INFORMATIVA
- Afeta: latência de `remember`, `edit`, `ingest` com codex exec

## Diagnóstico
- Latência de ~30-50s por chamada de embedding é custo fixo do codex exec
- Codex carrega ~11K tokens de system context por invocação
- As flags `--ephemeral --skip-git-repo-check` já estão aplicadas
- NÃO é bug do sqlite-graphrag — é custo intrínseco do codex CLI headless

## Workarounds Existentes
- `--llm-parallelism 8` para paralelizar chamadas de chunks em `remember`
- `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS=120` para corpos grandes
- `--llm-backend claude` quando Claude tem menor latência na rede local
- Migrar banco para dim=64 com `enrich --operation re-embed` para batches maiores (8 chunks vs 1 em dim=384)
- `--llm-model <modelo-mais-leve>` para escolher modelo com menor latência


## BUG-SKIP-EMBED — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `remember`, `edit`, `ingest`, `remember-batch`, `restore`, `rename-entity`, `init`
- Versão afetada: v1.0.82 a v1.0.88
- Versão corrigida: v1.0.89

## Problema
- A flag `--skip-embedding-on-failure` é aceita pelo clap e propagada para a env var `SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE` no `main.rs`
- NENHUM módulo interno lê essa env var
- Todos os comandos que geram embedding abortam com exit 11 quando o backend LLM falha
- A flag deveria permitir persistir a memória com embedding NULL para reprocessamento posterior via `enrich --operation re-embed`

## Causa Raiz
- O `main.rs` (linha 330-332) propaga `cli.skip_embedding_on_failure` para env var via `set_var`
- O `embedder.rs` NÃO tem nenhuma função que leia essa env var
- O `embed_passage_local` e `embed_passage_with_choice` propagam o erro diretamente sem consultar a flag
- Resultado: flag morta funcional desde v1.0.82

## Correção Aplicada
- Criada função `should_skip_embedding_on_failure()` em `embedder.rs` que lê `SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE`
- Criada função `embed_passage_or_skip()` que combina `embed_passage_with_choice` com a lógica de skip
- Em falha de embedding (exceto `AppError::Validation` que continua fatal): retorna `Ok(None)` quando a flag está ativa, ou propaga o erro quando inativa
- `AppError::Validation` (OAuth-only enforcement) permanece FATAL mesmo com a flag ativa

## Arquivos Modificados
- `src/embedder.rs` — adicionadas funções `should_skip_embedding_on_failure()` e `embed_passage_or_skip()`


## GAP-EMBED-PROPAGATION — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `init`, `ingest --mode claude-code`, `rename-entity`, `restore`
- Versão afetada: v1.0.79 a v1.0.88
- Versão corrigida: v1.0.89

## Problema
- 7 call sites usam `embed_passage_local` que resolve o backend via PATH probe (`detect_available`), ignorando `--llm-backend`
- O usuário passa `--llm-backend claude` mas esses comandos continuam usando codex (se disponível no PATH)
- Inconsistência: `remember` e `edit` honram `--llm-backend` (via `embed_passage_with_choice`), mas `init`, `restore`, `rename-entity` e `ingest --mode claude-code` ignoram

## Call Sites Corrigidos (7)
- `src/commands/init.rs:131` — smoke test de embedding
- `src/commands/ingest_claude.rs:1054` — embedding de body single-chunk
- `src/commands/ingest_claude.rs:1060` — embedding de chunk individual
- `src/commands/ingest_claude.rs:1107` — fallback de embedding de body inteiro
- `src/commands/ingest_claude.rs:1135` — embedding de entidade
- `src/commands/rename_entity.rs:95` — embedding do novo nome da entidade
- `src/commands/restore.rs:168` — re-embedding do body restaurado

## Correção Aplicada
- Substituídos todos os 7 call sites de `embed_passage_local` por `embed_passage_with_choice` com `None` (resolve via env var propagada pelo `main.rs`)
- O `embed_passage_with_choice(path, text, None)` usa o embedder ativo (que respeita `SQLITE_GRAPHRAG_CODEX_BINARY`, `SQLITE_GRAPHRAG_CLAUDE_BINARY` etc.)

## Arquivos Modificados
- `src/commands/init.rs`
- `src/commands/ingest_claude.rs`
- `src/commands/rename_entity.rs`
- `src/commands/restore.rs`


## GAP-WITH-DEFAULT-CODEX — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: BAIXA
- Afeta: nenhum caller em produção
- Versão corrigida: v1.0.89

## Problema
- `LlmBackend::with_default_codex()` em `llm_backend.rs:52` era método público legado
- Desde v1.0.89 `LlmExtractorConfig::default()` resolve o backend dinâmicamente via `detect_available_backend()`
- O nome `with_default_codex` é enganoso — sugere que SEMPRE usa codex, mas na verdade delega para `Default` que resolve dinâmicamente
- 6 callers existiam nos testes de integração (`tests/extract_backend.rs`)

## Correção Aplicada
- Método marcado com `#[deprecated(since = "1.0.89")]` com nota direcionando para `LlmBackend::new(LlmExtractorConfig::default())` ou `factory_for_choice()`
- 6 callers em `tests/extract_backend.rs` migrados para `LlmBackend::new(LlmExtractorConfig::default())`
- Teste `llm_backend_kind_and_model` ajustado para aceitar qualquer backend válido (codex, claude ou none) em vez de assertar apenas "codex"

## Arquivos Modificados
- `src/extract/llm_backend.rs` — `#[deprecated]` adicionado
- `tests/extract_backend.rs` — 6 callers migrados


## BUG-MODEL-VAZIO — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: CRÍTICA
- Afeta: `remember`, `edit`, `ingest`, `recall`, `hybrid-search`, `init`
- Versão afetada: v1.0.79 a v1.0.89 (pré-correção)
- Versão corrigida: v1.0.89

## Problema
- `codex_embed_model()` e `claude_embed_model()` retornam `String::new()` quando nenhuma env var está definida
- O codex recebe `--model ""` e falha com "The '' model is not supported when using Codex with a ChatGPT account"
- O claude falha silenciosamente com modelo vazio
- O `init` reportava `status: "ok_no_embedding"` por causa da falha do smoke test

## Causa Raiz
- As funções `codex_embed_model()` e `claude_embed_model()` em `src/extract/llm_embedding.rs` NÃO tinham default — retornavam string vazia quando `SQLITE_GRAPHRAG_CODEX_EMBED_MODEL`, `SQLITE_GRAPHRAG_LLM_MODEL` e `--llm-model` não estavam definidos

## Correção Aplicada
- `codex_embed_model()` agora retorna `"gpt-5.5"` como default (modelo principal do ChatGPT Pro OAuth)
- `claude_embed_model()` agora retorna `"claude-sonnet-4-6"` como default (modelo principal do Claude Pro/Max)
- Nível de log alterado de `warn` para `info` (não é condição anormal, é resolução de default)

## Arquivos Modificados
- `src/extract/llm_embedding.rs` — defaults adicionados em `codex_embed_model()` e `claude_embed_model()`


## BUG-SKIP-EMBED-INCOMPLETE — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `remember`, `remember-batch`
- Versão afetada: v1.0.89 (pré-correção, após fix parcial BUG-SKIP-EMBED)
- Versão corrigida: v1.0.89

## Problema
- O fix anterior BUG-SKIP-EMBED criou `embed_passage_or_skip()` em `src/embedder.rs` mas a função tinha ZERO chamadores
- O `remember` chamava `embed_passage_with_choice()` diretamente com `?`, propagando o erro sem verificar `should_skip_embedding_on_failure()`
- A flag `--skip-embedding-on-failure` era aceita pelo clap, propagada para env var pelo main.rs, mas NÃO tinha efeito no `remember` — exit 11 em vez de exit 0

## Causa Raiz
- A sessão anterior criou a infraestrutura (`should_skip_embedding_on_failure()` + `embed_passage_or_skip()`) mas NÃO conectou ao caminho de execução do `remember.rs`
- O `remember.rs` usava `?` em 3 pontos de embedding: passagem, chunks paralelos e entidades
- Os 3 pontos propagavam o erro sem verificar a flag de skip

## Correção Aplicada
- `embedding` mudou de `Vec<f32>` para `Option<Vec<f32>>` no `remember.rs`
- 3 pontos de embedding agora verificam `should_skip_embedding_on_failure()` via match/err guard
- `upsert_vec` condicionado a `if let Some(ref emb) = embedding`
- `chunk_embeddings_cache` condicionado a `if let Some(chunk_embeddings) = chunk_embeddings_cache.take()`
- `embed_entity_texts_cached` agora degrada para vetores vazios quando skip está ativo

## Arquivos Modificados
- `src/commands/remember.rs` — 5 pontos de edição para integrar skip-on-failure


## BUG-BUILDER-ENV-VAR — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: MÉDIA
- Afeta: `recall`, `hybrid-search`, `deep-research`, `remember`, `edit` quando `--llm-backend claude` é forçado junto com `--claude-binary`
- Versão afetada: v1.0.89 (pré-correção)
- Versão corrigida: v1.0.89

## Problema
- `LlmEmbeddingBuilder.build()` não lia as env vars `SQLITE_GRAPHRAG_CLAUDE_BINARY` e `SQLITE_GRAPHRAG_CODEX_BINARY`
- O `main.rs` propagava `--claude-binary` para a env var via `set_var`, mas `build()` usava apenas `self.binary_override` ou `which::which`
- O caminho `detect_available()` lia a env var corretamente, mas era codex-first
- Quando `--llm-backend claude` forçava o backend Claude, o builder via `with_claude_builder().build()` ignorava a env var e buscava via `which::which("claude")`
- Resultado: `--claude-binary /caminho/custom --llm-backend claude` ignorava o path customizado

## Correção Aplicada
- `build()` agora lê a env var (`SQLITE_GRAPHRAG_CLAUDE_BINARY` ou `SQLITE_GRAPHRAG_CODEX_BINARY`) antes de cair para `which::which`
- Precedência: `binary_override` (argumento direto) > env var > `which::which`

## Arquivos Modificados
- `src/extract/llm_embedding.rs` — `LlmEmbeddingBuilder::build()` agora lê env var antes de `which::which`


## BUG-BATCH-STATUS — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: BAIXA
- Afeta: `remember-batch`
- Versão afetada: v1.0.89 (pré-correção)
- Versão corrigida: v1.0.89

## Problema
- `remember-batch` retornava `status: "indexed"` para todos os itens, independente de terem sido criados ou atualizados
- O contrato documentado especifica `"created"`, `"updated"`, `"skipped"` e `"failed"`
- O valor `"indexed"` era um resíduo da implementação inicial que não distinguia as operações

## Correção Aplicada
- A variável `memory_id` agora retorna uma tupla `(memory_id, batch_action)` com o status correto
- Caminho de update (force-merge): `"updated"`
- Caminho de criação: `"created"`
- Caminho de falha (já existia): `"failed"` (via `AppError::Duplicate`)

## Arquivos Modificados
- `src/commands/remember_batch.rs` — status dinâmico baseado no caminho de execução


## BUG-BATCH-SKIP-EMBED — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: MÉDIA
- Afeta: `remember-batch` quando `--skip-embedding-on-failure` está ativo
- Versão afetada: v1.0.89 (pré-correção)
- Versão corrigida: v1.0.89

## Problema
- `remember-batch` não integrava `--skip-embedding-on-failure` nos 3 pontos de embedding
- `embed_passage_with_choice` era chamado com `?` direto, propagando erros sem verificar a flag
- `embed_entity_texts_cached` também usava `?` direto
- Quando o LLM falhava, o batch inteiro falhava com exit 11 em vez de persistir sem embedding

## Correção Aplicada
- 3 pontos de embedding envoltos com match guards que verificam `should_skip_embedding_on_failure()`
- `AppError::Validation` permanece fatal mesmo com a flag ativa
- Pattern idêntico ao fix BUG-SKIP-EMBED-INCOMPLETE do `remember`

## Arquivos Modificados
- `src/commands/remember_batch.rs` — 3 pontos de embedding com skip-on-failure guards


## BUG-BOOLISH-ENV — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `--skip-embedding-on-failure`, `--strict-env-clear`, `--dry-run-backend`, `--llm-slot-no-wait`
- Versão afetada: v1.0.82 a v1.0.89
- Versão corrigida: v1.0.89
- Ambiente: Linux Fedora 44

## Problema
- 4 flags booleanas globais com `env = "SQLITE_GRAPHRAG_*"` rejeitam valores Unix padrão (`1`, `yes`, `on`) com exit 2
- Causa raiz: `bool` com `env = "..."` no clap usa `bool::from_str` que aceita APENAS `"true"` e `"false"`
- Qualquer script ou CI que sete `SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE=1` falha antes de executar o comando

## Correção Aplicada
- Adicionado `value_parser = clap::builder::BoolishValueParser::new()` nas 4 flags
- `BoolishValueParser` aceita `true`/`false`/`yes`/`no`/`on`/`off`/`1`/`0`

## Arquivos Modificados
- `src/cli.rs` — 4 campos: `strict_env_clear`, `dry_run_backend`, `skip_embedding_on_failure`, `llm_slot_no_wait`


## BUG-RESTORE-BACKEND — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `restore`
- Versão afetada: v1.0.76 a v1.0.89
- Versão corrigida: v1.0.89

## Problema
- `restore` chama `embed_passage_with_choice` com `None` para `llm_backend`, ignorando `--llm-backend` da CLI
- `restore` NÃO honra `--skip-embedding-on-failure` — falha de embedding causa exit 11
- `restore` NÃO recebe `llm_backend` do main.rs

## Correção Aplicada
- Assinatura alterada para receber `LlmBackendChoice`
- Embedding envolvido com match guard + `should_skip_embedding_on_failure()`
- `upsert_vec` condicionado a `Some(embedding)`
- main.rs propaga `cli.llm_backend` para `restore`

## Arquivos Modificados
- `src/commands/restore.rs` — assinatura + skip-on-failure guard
- `src/main.rs` — propagação de `llm_backend`


## BUG-RENAME-ENTITY-BACKEND — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `rename-entity`
- Versão afetada: v1.0.76 a v1.0.89
- Versão corrigida: v1.0.89

## Problema
- `rename-entity` chama `embed_passage_with_choice` com `None`, ignorando `--llm-backend`
- `rename-entity` NÃO honra `--skip-embedding-on-failure` — falha de embedding causa exit 11
- `rename-entity` NÃO recebe `llm_backend` do main.rs

## Correção Aplicada
- Assinatura alterada para receber `LlmBackendChoice`
- Embedding envolvido com match guard + `should_skip_embedding_on_failure()`
- `upsert_entity_vec` condicionado a `Some(embedding)`
- main.rs propaga `cli.llm_backend` para `rename-entity`

## Arquivos Modificados
- `src/commands/rename_entity.rs` — assinatura + skip-on-failure guard
- `src/main.rs` — propagação de `llm_backend`



## BUG-EDIT-SKIP-EMBED — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `edit`
- Versão afetada: v1.0.76 a v1.0.89
- Versão corrigida: v1.0.89

## Problema
- `edit` chama `embed_passage_with_choice` com `?` direto, sem skip-on-failure guard
- Quando LLM de embedding falha, `edit` retorna exit 11 em vez de exit 0
- Assimetria com `remember`, `remember-batch`, `restore` e `rename-entity` que honram a flag

## Correção Aplicada
- Embedding envolvido com match guard + `should_skip_embedding_on_failure()`
- `upsert_vec` condicionado a `Some(embedding)`
- `backend_invoked` populado apenas quando embedding tem sucesso

## Arquivos Modificados
- `src/commands/edit.rs` — skip-on-failure guard


## BUG-STRICT-ENV-PROPAGATION — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: MÉDIA
- Afeta: `--strict-env-clear` via CLI
- Versão afetada: v1.0.83 a v1.0.89
- Versão corrigida: v1.0.89

## Problema
- `--strict-env-clear` passada via CLI seta `cli.strict_env_clear = true`
- `env_whitelist.rs` lê `std::env::var("SQLITE_GRAPHRAG_STRICT_ENV_CLEAR")` diretamente
- Clap NÃO propaga o valor da flag CLI para a env var (apenas lê como fallback)
- Resultado: `--strict-env-clear` via CLI é silenciosamente ignorada

## Correção Aplicada
- `main.rs` propaga `cli.strict_env_clear` via `std::env::set_var` antes do dispatch

## Arquivos Modificados
- `src/main.rs` — propagação de `--strict-env-clear`


## BUG-BATCH-FTS-DESYNC — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `remember-batch --force-merge`
- Versão afetada: v1.0.89 (pre-fix)
- Versão corrigida: v1.0.89

## Descrição
- `remember-batch --force-merge` atualizava memórias via UPDATE sem chamar `sync_fts_after_update`
- O trigger AFTER UPDATE do FTS5 é intencionalmente ausente (conflito sqlite-vec)
- Todo UPDATE em `memories` DEVE sincronizar FTS manualmente
- `remember` fazia isso corretamente; `remember-batch` omitia a chamada
- Resultado: FTS5 ficava desatualizado, `hybrid-search` retornava conteúdo antigo

## Correção Aplicada
- Captura valores antigos (name, description, body) ANTES do UPDATE
- Chama `memories::sync_fts_after_update` após `memories::update`
- Padrão idêntico ao usado em `remember.rs` linhas 791-827

## Arquivos Modificados
- `src/commands/remember_batch.rs` — adicionada captura de old values + sync_fts_after_update


## BUG-FORGET-DOUBLE-DELETE-VEC — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: BAIXA
- Afeta: `forget`
- Versão afetada: v1.0.69 a v1.0.89 (pre-fix)
- Versão corrigida: v1.0.89

## Descrição
- `forget` chamava `delete_vec` duas vezes para soft-delete bem-sucedido
- Primeira chamada na linha 94 (G39 Passo 4, antes do soft_delete)
- Segunda chamada na linha 135 (dentro de `if forgotten`)
- A segunda chamada era redundante e gerava warnings de log espúrios

## Correção Aplicada
- Removida a segunda chamada redundante a `delete_vec`
- Renomeado `memory_id` para `_memory_id` (variável não mais usada)

## Arquivos Modificados
- `src/commands/forget.rs` — removida chamada duplicada de `delete_vec`


## BUG-ENRICH-DESC-FTS-DESYNC — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `enrich --operation description-enrich`
- Versão afetada: v1.0.69 a v1.0.89 (pre-fix)
- Versão corrigida: v1.0.89

## Descrição
- `call_description_enrich` atualizava `description` via SQL direto sem `sync_fts_after_update`
- O trigger `trg_fts_au` (AFTER UPDATE) no FTS5 é intencionalmente ausente (conflito sqlite-vec)
- Resultado: busca full-text retornava resultado desatualizado após enriquecimento de descrição

## Correção Aplicada
- Adicionada leitura de `old_name` antes do UPDATE
- Adicionada chamada a `sync_fts_after_update` após o UPDATE com valores antigos e novos

## Arquivos Modificados
- `src/commands/enrich.rs` — `call_description_enrich`: adicionado FTS sync


## BUG-ENRICH-BODY-EXTRACT-FTS-DESYNC — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `enrich --operation body-extract`
- Versão afetada: v1.0.69 a v1.0.89 (pre-fix)
- Versão corrigida: v1.0.89

## Descrição
- `call_body_extract` atualizava `body` via SQL direto sem `sync_fts_after_update`
- Mesma causa raiz que BUG-ENRICH-DESC-FTS-DESYNC
- Resultado: busca full-text retornava resultado desatualizado após extração de body

## Correção Aplicada
- Ampliada a query SELECT para incluir `description` (necessário para FTS sync)
- Adicionada leitura de `old_name` antes do UPDATE
- Adicionada chamada a `sync_fts_after_update` após o UPDATE

## Arquivos Modificados
- `src/commands/enrich.rs` — `call_body_extract`: adicionado FTS sync


## GAP-LLM-FALLBACK-DEAD-FLAG — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Severidade: BAIXA
- Status: RESOLVIDO

## Descrição
- Flag `--llm-fallback` (default `codex,claude,none`) aceita pelo clap e exibida no `--dry-run-backend`
- NUNCA propagada para o pipeline real de embedding
- `to_chain()` em `LlmBackendChoice::Auto` usava cadeia hardcoded `[Codex, Claude, None]`
- Resultado: `--llm-fallback claude,none` era silenciosamente ignorado pelo pipeline real

## Causa Raiz
- `to_chain()` era implementada com match hardcoded por variante
- Nenhum módulo interno lia a env var `SQLITE_GRAPHRAG_LLM_FALLBACK`
- `dry_run_backend.rs` lia `cli.llm_fallback` para display mas o pipeline real ignorava

## Correção Aplicada
- `main.rs`: adicionado `set_var("SQLITE_GRAPHRAG_LLM_FALLBACK", &cli.llm_fallback)`
- `cli.rs`: `to_chain()` para `Auto` agora lê `SQLITE_GRAPHRAG_LLM_FALLBACK` via `parse_fallback_chain()`
- `cli.rs`: nova função `parse_fallback_chain()` parseia string CSV em `Vec<LlmBackendKind>`
- Tokens desconhecidos na cadeia emitem `tracing::warn!` e são ignorados
- Cadeia vazia faz fallback para `[Codex, Claude, None]` (default canônico)

## Arquivos Modificados
- `src/main.rs` — adicionado `set_var` para `llm_fallback`
- `src/cli.rs` — `to_chain()` lê env var; nova `parse_fallback_chain()`


## BUG-YES-FLAG-IGNORED — RESOLVIDO em v1.0.89
- Data de identificação: 2026-06-21
- Data de resolução: 2026-06-21
- Severidade: ALTA
- Afeta: `slots release`, `purge`, `cleanup-orphans`
- Versão afetada: v1.0.88
- Versão corrigida: v1.0.89

## Sintoma
- `slots release --slot-id N` deleta o slot SEM exigir `--yes`
- `purge --retention-days 0` executa a purga SEM exigir `--yes`
- `cleanup-orphans` deleta entidades órfãs SEM exigir `--yes`
- Em todos os casos a flag `--yes` existia no clap mas era ignorada no corpo da função
- Padrão do projeto: 5 outros comandos (prune-ner, normalize-entities, vec purge, prune-relations, cache clear) ABORTAM corretamente sem `--yes`

## Causa Raiz
- `slots.rs:run_release()`: imprimia aviso via `eprintln!` mas executava `remove_file` na sequência
- `purge.rs:run()`: campo `args.yes` declarado mas NUNCA lido — `if !args.dry_run` executava incondicionalmente
- `cleanup_orphans.rs:run()`: imprimia aviso via `emit_progress` mas executava `delete_entities_by_ids` na sequência

## Correção Aplicada
- `slots.rs`: substituído `eprintln!` por `return Err(AppError::Validation(...))`
- `purge.rs`: adicionado `if !args.dry_run && !args.yes { return Err(AppError::Validation(...)) }`
- `cleanup_orphans.rs`: substituído `emit_progress` por `return Err(AppError::Validation(...))`
- `tests/slots_no_println_integration.rs`: atualizado para esperar 0 `eprintln!` em vez de 1

## Arquivos Modificados
- `src/commands/slots.rs` — guarda `--yes` ANTES de `remove_file`
- `src/commands/purge.rs` — guarda `--yes` ANTES da transação de purge
- `src/commands/cleanup_orphans.rs` — guarda `--yes` ANTES de `delete_entities_by_ids`
- `tests/slots_no_println_integration.rs` — teste atualizado
