---
name: sqlite-graphrag
description:Para memória persistente, GraphRAG, ou contexto de longo prazo em Claude Code, Codex, OpenCode, Cursor, Windsurf, agentes AI. Em: lembrar, salvar, recuperar, busca híbrida, grafo entidades, memória SQLite, RAG local, embedding LLM-only, OAuth, BLOB-backed, migrate, embedding-dim, llm-parallelism, re-embed, force-reembed, OAuth-only, endurecimento codex claude opencode, mock LLM CI, ADR-0041 ADR-0051, OpenRouter Bedrock, --dry-run-backend, backend_invoked, preflight exit 16, llm-backend codex claude opencode none auto, llm-model gpt-5.4-mini claude-sonnet-4-6 opencode/big-pickle, headless fallback chain, v1.0.86-v1.0.90. KW: memória RAG GraphRAG SQLite one-shot OAuth offline persistente grafo entidade opencode codex claude.
---


## Versão Atual (v1.0.90)
- Versão atual do binário: v1.0.90 (lançada em 2026-06-22)
- Versão atual do schema: v15 (após init ou migrate em banco fresco)
- Esta skill documenta features de v1.0.86 até v1.0.90
- Versões anteriores (v1.0.85.2 e abaixo) estão fora do escopo
- Para versões mais antigas, consulte o histórico git desta skill


## Quando Esta Skill Ativa
- USE quando o usuário pede para lembrar, salvar, recordar, recuperar, buscar, ou persistir algo entre sessões
- USE para contexto de longo prazo, grafo de conhecimento, GraphRAG, RAG, ligação de entidades, gestão de memória
- USE quando sqlite, sqlite-graphrag, embedding, FTS5, hybrid-search, ou memória LLM for mencionado
- NÃO USE para dados efêmeros pontuais, I/O simples de arquivo, ou tarefas sem relação a contexto persistente


## Regras de Instrução para LLMs (Prompt Rules)
- QUANDO usuário disser "lembre disso", "salve isso", "persista isso" → EXECUTE `remember --force-merge` com `--graph-stdin` incluindo entidades curadas e relações canônicas
- QUANDO usuário perguntar "o que você sabe sobre X", "recall X", "busque X" → EXECUTE `hybrid-search "X" --k 10 --json` PRIMEIRO, depois EXPANDA resultados top com `read --name <nome> --json`
- QUANDO usuário perguntar "como X se relaciona com Y" → EXECUTE `graph traverse --from X --depth 2 --json` ou `related X --hops 2 --json`
- QUANDO usuário pedir "pesquise profundamente sobre X" → EXECUTE `deep-research "X" --k 20 --max-hops 3 --json`
- ANTES de criar QUALQUER memória → EXECUTE `hybrid-search "<nome ou descrição>" --k 5 --json` para VERIFICAR duplicatas; se encontrar, USE `--force-merge` para ATUALIZAR em vez de criar nova
- APÓS criar ou atualizar memória → VERIFIQUE com `read --name <nome> --json | jaq '{name, description, body_length}'`
- APÓS CADA turno com achados novos → AVALIE se deve persistir via `remember --force-merge`; se nada novo, DECLARE "Nenhum achado novo para persistir neste turno"
- QUANDO exit code for não-zero → LEIA envelope JSON de erro do stdout (`jaq '{code, message, error_class}'`), REPORTE ao usuário com passos de remediação
- QUANDO exit code 9 (duplicada) → RETENTE com flag `--force-merge`
- QUANDO exit code 19 (SHUTDOWN) → RETRY OBRIGATÓRIO; trabalho parcial foi descartado
- QUANDO exit code 75 (singleton locked) → AGUARDE e retente; NÃO aumente concorrência
- QUANDO exit code 16 (preflight) → CORRIJA o problema de config MCP; NÃO bypass com `SKIP_PREFLIGHT`
- SEMPRE parseie saída JSON com `jaq` (NUNCA `jq`)
- SEMPRE passe flag `--json` em toda invocação de `sqlite-graphrag`
- SEMPRE use `--llm-backend codex --llm-model gpt-5.4-mini` OU `--llm-backend claude --llm-model claude-sonnet-4-6` OU `--llm-backend opencode --llm-model opencode/big-pickle` para comandos de embedding
- SEMPRE use APENAS relações canônicas: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- SEMPRE mapeie relações não-canônicas ANTES de persistir: `adds|creates → causes`, `implements → supports`, `blocks → contradicts`, `tested-by → related`, `part-of → applies-to`
- SEMPRE normalize nomes de entidade para kebab-case ASCII lowercase ANTES de passar à CLI
- NUNCA use MCP Serena ou arquivos `.md` de memória para persistência
- NUNCA escreva MEMORY.md ou qualquer memória baseada em arquivo
- NUNCA inicie ou referencie daemon (removido na v1.0.76)
- NUNCA passe `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` no ambiente
- PREFIRA `remember --force-merge` sobre `edit` para atualizações para garantir re-indexação
- PREFIRA `--graph-stdin` com entidades curadas sobre `--enable-ner` para qualidade de extração
- LIMITE entidades do grafo a conceitos específicos do domínio: projetos, ferramentas, pessoas, decisões, arquivos, incidentes
- REJEITE palavras genéricas, pronomes, UUIDs, hashes, timestamps como nomes de entidade
```bash
# REGRA: SEMPRE verifique duplicatas ANTES de criar memória
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  hybrid-search "auth JWT design" --k 5 --json | jaq '.results[].name'
# Se encontrar: ATUALIZE com --force-merge
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name decisao-auth --type decision --force-merge \
  --description "Estrategia JWT atualizada" --body-file auth.md

# REGRA: SEMPRE use --graph-stdin com entidades curadas e relações canônicas
echo '{"body":"JWT com 15 min de expiracao","entities":[{"name":"jwt","entity_type":"concept"},{"name":"servico-auth","entity_type":"tool"}],"relationships":[{"source":"servico-auth","target":"jwt","relation":"uses","strength":0.9}]}' \
  | sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
    remember --name decisao-auth --type decision --description "Rotacao JWT" --graph-stdin --force-merge

# REGRA: SEMPRE verifique após escrita
sqlite-graphrag read --name decisao-auth --json | jaq '{name, description, body_length}'

# REGRA: QUANDO exit 9 (duplicada) → retente com --force-merge
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name mem-existente --type note --description "x" --body "y" --force-merge
```


## Princípios Fundamentais
- INVOKE sempre como subprocesso via `std::process::Command`
- READ stdout para dados estruturados JSON ou NDJSON
- READ stderr para logs de tracing e mensagens humanas
- CHECK exit code ANTES de parsear stdout
- TRUST em contratos JSON como API versionada por SemVer
- SAIBA que BUILD é LLM-only e one-shot; binário tem 14.6 MiB stripped ELF (NÃO 6 MB como em docs antigos)
- SAIBA que BUILD NÃO tem daemon, NÃO tem ONNX runtime, NÃO tem cache de modelo
- SAIBA que OAUTH-ONLY: spawn ABORTA exit 1 se `ANTHROPIC_API_KEY` estiver setada
- SAIBA que OAUTH-ONLY: spawn ABORTA exit 1 se `OPENAI_API_KEY` estiver setada
- PASSE NAMESPACE por projeto via `--namespace <ns>` ou env
- SAIBA que NAMESPACE default é `global` quando omitido
- NUNCA exponha o binário como servidor MCP ou serviço HTTP
- NUNCA escreva arquivo `.sqlite` em paralelo ao binário
- NUNCA edite o arquivo `.sqlite` a partir de outra ferramenta
```bash
# INVOQUE sempre como subprocesso e verifique o exit code ANTES de parsear stdout
sqlite-graphrag health --json | jaq '.integrity_ok'
echo "exit=$?"
```


## Cartão de Referência Rápida
- INIT primeira vez: `sqlite-graphrag init --namespace <ns>`
- VERIFIQUE saúde: `sqlite-graphrag health --json | jaq '.integrity_ok'`
- ARMAZENE (Codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini remember --name <kebab> --type note --description "x" --body "y"`
- ARMAZENE (Claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 remember --name <kebab> --type note --description "x" --body "y"`
- ARMAZENE (OpenCode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle remember --name <kebab> --type note --description "x" --body "y"`
- INGIRA pasta (Codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini ingest ./docs --mode codex --recursive --pattern "*.md" --type document --json`
- INGIRA pasta (Claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 ingest ./docs --mode claude-code --recursive --pattern "*.md" --type document --json`
- INGIRA pasta (OpenCode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle ingest ./docs --mode opencode --recursive --pattern "*.md" --type document --json`
- BUSQUE semântica (Codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini recall "query" --k 5 --json`
- BUSQUE semântica (Claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 recall "query" --k 5 --json`
- BUSQUE semântica (OpenCode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle recall "query" --k 5 --json`
- BUSQUE híbrida (Codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini hybrid-search "query" --k 10 --rrf-k 60 --json`
- BUSQUE híbrida (Claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 hybrid-search "query" --k 10 --rrf-k 60 --json`
- BUSQUE híbrida (OpenCode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle hybrid-search "query" --k 10 --rrf-k 60 --json`
- PESQUISE profunda (Codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini deep-research "question" --k 20 --max-hops 3 --json`
- PESQUISE profunda (Claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 deep-research "question" --k 20 --max-hops 3 --json`
- PESQUISE profunda (OpenCode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle deep-research "question" --k 20 --max-hops 3 --json`
- ENRIQUEÇA (Codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation re-embed --limit 100 --resume --json`
- ENRIQUEÇA (Claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --json`
- ENRIQUEÇA (OpenCode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation re-embed --limit 100 --resume --json`
- LEIA (sem embedding): `sqlite-graphrag read --name <kebab> --json | jaq '{name, description, body_length}'`
- LISTE (sem embedding): `sqlite-graphrag list --type decision --limit 50 --json | jaq '.items[].name'`
- TRAVESSE grafo (sem embedding): `sqlite-graphrag graph traverse --from <entity> --depth 2 --json`
- VEJA stats do grafo (sem embedding): `sqlite-graphrag graph stats --json | jaq '{node_count, edge_count, avg_degree}'`
- LIGUE entidades (sem embedding): `sqlite-graphrag link --from jwt --to servico-auth --relation uses --weight 0.8 --create-missing --json`
- DELEÇÃO física (sem embedding): `sqlite-graphrag forget --name <n>` depois `purge --retention-days 30 --yes`


## Inicialização, Saúde e Config Global
- EXECUTE `sqlite-graphrag init --namespace <ns>` no primeiro uso
- EXECUTE `health --json` para verificar `integrity_ok` e `schema_ok`
- VERIFIQUE `schema_version >= 15` após `init` ou `migrate`
- EXECUTE `migrate --json` após cada upgrade do binário
- USE `migrate --to-llm-only --drop-vec-tables --json` para bancos v1.0.74 ou v1.0.75
- USE `migrate --rehash --json` para reparar drift de checksum SipHasher13 V002
- USE `migrate --dry-run --json` para PREVIEW de migrações pendentes sem aplicar
- TRATE exit code 10 como erro de banco; execute `vacuum` e `health`
- TRATE exit code 15 como ocupado; amplie `--wait-lock`
- TRATE exit code 16 como falha preflight (v1.0.87+); corrija config MCP ou defina `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1`
- ABORTE pipeline quando `integrity_ok` retornar `false`
- EXECUTE `optimize --json` para refrescar estatísticas do planner; resposta inclui `fts_rebuilt`
- USE `optimize --skip-fts --json` quando FTS5 foi reconstruído recentemente
- EXECUTE `fts rebuild --json` quando `health.fts_degraded` for true
- INSPECIONE `wal_size_mb` em `health` para fragmentação
- VERIFIQUE `journal_mode` igual a `wal` em produção
- USE `debug-schema --json` para troubleshooting de drift de schema
- PASSE `--db <PATH>` para sobrescrever localização do banco (agora aceito em `embedding status/list/abandon`, `pending list/show` desde v1.0.89, ADR-0049)
- PASSE `--namespace <NS>` em `health` desde v1.0.89 para filtrar contagens para um namespace
- DEFINA env `SQLITE_GRAPHRAG_DB_PATH` para configuração persistente
- DEFINA env `SQLITE_GRAPHRAG_NAMESPACE` para namespace persistente
- PASSE `--lang en` ou `--lang pt` para forçar idioma do stderr
- PASSE `--tz America/Sao_Paulo` para localizar timestamps
- DEFINA env `SQLITE_GRAPHRAG_DISPLAY_TZ` para timezone persistente
- DEFINA `SQLITE_GRAPHRAG_LOG_FORMAT=json` para agregadores de log
- USE `-v` para info, `-vv` para debug, `-vvv` para trace
- ATIVE `SQLITE_GRAPHRAG_LOW_MEMORY=1` em containers restritos
- DEFINA env `SQLITE_GRAPHRAG_EMBEDDING_DIM` na faixa `[8, 4096]` (default 64 MRL)
- DEFINA `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` para modo compliance (ADR-0041)
- DEFINA `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` APENAS para harnesses de teste CI
- USE valores válidos de `--type`: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- CONHEÇA as FLAGS globais: `--db`, `--namespace`, `--lang`, `--tz`, `--json`, `--low-memory`, `--max-concurrency N`, `--wait-lock SECS`, `--llm-parallelism N`, `--llm-backend claude|codex|opencode|none|auto[,fallback...]`, `--llm-model <MODEL>`, `--dry-run-backend`, `--llm-fallback-mode <claude|codex|opencode>`, `--graceful-shutdown-secs N`, `--claude-binary <PATH>`, `--codex-binary <PATH>`, `--opencode-binary <PATH>`, `--opencode-model <MODEL>`, `--opencode-timeout <SECS>`, `--skip-embedding-on-failure`
```bash
# INICIALIZE namespace, VERIFIQUE saúde e CONFIRME schema_version
sqlite-graphrag init --namespace meuprojeto
sqlite-graphrag health --json | jaq '{integrity_ok, schema_ok, schema_version}'

# PREVIEW de migrações pendentes ANTES de aplicar
sqlite-graphrag migrate --dry-run --json | jaq '.would_apply[]? | {name, version}'

# APLIQUE migração após upgrade do binário
sqlite-graphrag migrate --json | jaq '{applied, schema_version}'

# FORCE idioma do stderr e timezone de exibição
sqlite-graphrag --lang pt --tz America/Sao_Paulo health --json | jaq '.integrity_ok'

# DEFINA config persistente via env vars
export SQLITE_GRAPHRAG_DB_PATH=/data/projeto.sqlite
export SQLITE_GRAPHRAG_NAMESPACE=meuprojeto
sqlite-graphrag health --json | jaq '.db_path'
```


## Contrato de Arquitetura (OAuth/LLM/One-Shot)
- SAIBA que BUILD é LLM-only; build padrão NÃO tem `fastembed`, `ort`, `ndarray`, `tokenizers`, `huggingface-hub`, `sqlite-vec`, `GLiNER`
- SAIBA que BUILD removeu subcomando `daemon` inteiramente (ADR-0021)
- SAIBA que COSINE similarity é pure Rust em `src/similarity.rs`
- SAIBA que COSINE roda sobre `memory_embeddings`, `entity_embeddings`, `chunk_embeddings` com BLOB
- SAIBA que SCHEMA é v15 após `init` ou `migrate` em banco fresco
- SAIBA que MIGRAÇÃO V013 dropa virtual tables `vec_memories`, `vec_entities`, `vec_chunks`
- SAIBA que MIGRAÇÃO V014 cria tabela de checkpoint `pending_memories`
- SAIBA que MIGRAÇÃO V015 cria fila de retry `pending_embeddings`
- SAIBA que OAUTH-ONLY: `ANTHROPIC_API_KEY` ABORTA spawn com `AppError::Validation` (ADR-0011)
- SAIBA que OAUTH-ONLY: `OPENAI_API_KEY` ABORTA spawn com `AppError::Validation` (ADR-0011)
- SAIBA que OAUTH-ONLY: ambas API keys EXCLUÍDAS do whitelist de env-clear
- SAIBA que OAUTH-ONLY: flag `--bare` REMOVIDA de todos os caminhos executáveis
- SAIBA que OAUTH-ONLY: 7 flags de endurecimento SEMPRE passadas para `claude -p`
- CONHEÇA as FLAGS de endurecimento para claude: `--model claude-sonnet-4-6 --strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions --output-schema`
- CONHEÇA as FLAGS de endurecimento para codex: `--model gpt-5.5 --json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules -c mcp_servers='{}' --ask-for-approval never`
- SAIBA que ADR-0041 v1.0.83: `ANTHROPIC_AUTH_TOKEN` PRESERVADA para providers Anthropic-compatíveis
- SAIBA que ADR-0041 v1.0.83: `ANTHROPIC_BASE_URL` PRESERVADA para endpoints customizados
- SAIBA que ADR-0041 v1.0.83: `OPENAI_BASE_URL` PRESERVADA para endpoints OpenAI-compatíveis
- SAIBA que ADR-0041 v1.0.83: `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT` PRESERVADAS
- SAIBA que ADR-0041 v1.0.83: providers suportados incluem OpenRouter, AWS Bedrock, gateways corporativos
- SAIBA a PRECEDÊNCIA de DIM de embedding: env `SQLITE_GRAPHRAG_EMBEDDING_DIM` depois `schema_meta.dim` depois default 64 MRL
- SAIBA que DIM de embedding adapta tamanho de lote: base 8 chunks / 25 nomes de entidade em dim 64
- USE MOCK LLM CLI para CI: prefixe `tests/mock-llm` ao PATH
- USE a RECEITA de bypass de SHUTDOWN: `PATH=tests/mock-llm:$PATH SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1 setsid -w timeout 120 sqlite-graphrag …`
- NUNCA instale com `--features embedding-legacy` ou `--features ner-legacy`
- NUNCA dependa do daemon ou flag `--bare` (REMOVIDOS em v1.0.76 e v1.0.79)
- NUNCA misture queries em `vec_memories` (REMOVIDO em v1.0.76)
- NUNCA chame `migrate --to-llm-only` sem guarda de segurança `--drop-vec-tables`
```bash
# CONFIRME que o build é OAuth-only: ANTHROPIC_API_KEY aborta com exit 1
ANTHROPIC_API_KEY=sk-test sqlite-graphrag init --namespace x 2>&1 || echo "exit=$?"

# CONFIRME que OPENAI_API_KEY também aborta com exit 1
OPENAI_API_KEY=sk-test sqlite-graphrag init --namespace x 2>&1 || echo "exit=$?"

# PRESERVE provider customizado via ANTHROPIC_BASE_URL (ADR-0041, OpenRouter/Bedrock)
export ANTHROPIC_BASE_URL=https://openrouter.ai/api/v1
export ANTHROPIC_AUTH_TOKEN=sk-or-...
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name nota --type note --description x --body y
```


## Seleção de Backend LLM — Codex, Claude e OpenCode Headless
- NOTA: exemplos dedicados do OpenCode estão na seção "Seleção de Backend LLM — OpenCode Headless (v1.0.90)" abaixo
- PASSE `--llm-backend codex` para FORÇAR backend Codex em comandos de embedding
- PASSE `--llm-backend claude` para FORÇAR backend Claude Code headless em comandos de embedding
- PASSE `--llm-model <MODEL>` para selecionar o modelo de embedding em QUALQUER backend (v1.0.89, ADR-0050)
- SAIBA que o DEFAULT de `--llm-backend` é `codex`
- SAIBA que o MODELO padrão para backend codex é `gpt-5.5` e para backend claude é `claude-sonnet-4-6`
- PASSE `--llm-backend codex,claude` para codex-PRIMEIRO com fallback claude (ADR-0038)
- PASSE `--llm-backend claude,codex` para claude-PRIMEIRO com fallback codex
- PASSE `--llm-backend codex,claude,none` para cair em embedding null quando ambos falharem
- PASSE `--llm-fallback-mode <claude|codex>` para trocar backend mid-job em rate-limit
- PASSE `--skip-embedding-on-failure` APENAS quando `--llm-backend …,none` está ativo
- PASSE `--dry-run-backend` para PLANEJAR a operação de backend sem executá-la (preview idempotente)
- PARSEE o campo `backend_invoked` no envelope para CONFIRMAR o backend efetivo
- DEFINA env `SQLITE_GRAPHRAG_LLM_BACKEND` para fixar backend de forma persistente
- DEFINA env `SQLITE_GRAPHRAG_LLM_MODEL` para fixar modelo de forma persistente
- DEFINA env `SQLITE_GRAPHRAG_CODEX_BINARY` para sobrescrever o path do binário codex
- PASSE `--codex-binary <PATH>` para sobrescrever o path do binário codex inline
- PASSE `--claude-binary <PATH>` para sobrescrever o path do binário claude inline
- SAIBA que backend claude divide-se em embedder local via `embed_via_claude_local` (zero-token, compatível com OAuth)
- USE `LlmEmbeddingBuilder` para compor pipeline: `with_backend(Codex).or_fallback(Claude).or_skip()`
- EXECUTE `codex login` após upgrade para refrescar o refresh token OAuth do codex
- ATUALIZE OAuth do claude quando erro de OAuth expirado for reportado
- NUNCA passe `--llm-backend codex` esperando que ele use credenciais de API key (OAuth-only, exit 1)
```bash
# BACKEND Codex headless — gpt-5.4-mini explícito
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name decisao-auth --type decision \
  --description "Estrategia JWT com rotacao" --body "15 min de expiracao com refresh"

# BACKEND Claude Code headless — claude-sonnet-4-6 explícito
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name decisao-auth --type decision \
  --description "Estrategia JWT com rotacao" --body "15 min de expiracao com refresh"

# CODEX exclusivo — FALHA em erro, sem fallback
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  recall "autenticacao JWT" --k 5 --json | jaq '.backend_invoked'

# CODEX-PRIMEIRO com fallback claude — cadeia de fallback
sqlite-graphrag --llm-backend codex,claude --llm-model gpt-5.5 \
  hybrid-search "fluxo de auth" --k 10 --json | jaq '.backend_invoked'

# CADEIA tripla — codex, depois claude, depois embedding null
sqlite-graphrag --llm-backend codex,claude,none --skip-embedding-on-failure \
  remember --name nota --type note --description x --body y | jaq '.backend_invoked'

# DRY-RUN do backend — PLANEJE sem executar (preview idempotente)
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini --dry-run-backend \
  recall "query" --k 5 --json | jaq '{backend_invoked, planned: true}'

# CONFIGURE backend codex persistente via env vars
export SQLITE_GRAPHRAG_LLM_BACKEND=codex
export SQLITE_GRAPHRAG_LLM_MODEL=gpt-5.4-mini
export SQLITE_GRAPHRAG_CODEX_EMBED_MODEL=gpt-5.4-mini
sqlite-graphrag recall "query" --k 5 --json | jaq '.backend_invoked'

# CONFIGURE backend claude persistente via env vars
export SQLITE_GRAPHRAG_LLM_BACKEND=claude
export SQLITE_GRAPHRAG_LLM_MODEL=claude-sonnet-4-6
sqlite-graphrag hybrid-search "query" --k 10 --json | jaq '.backend_invoked'

# SOBRESCREVA path do binario codex
export SQLITE_GRAPHRAG_CODEX_BINARY=/usr/local/bin/codex
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini recall "query" --k 5 --json

# SOBRESCREVA path do binario claude inline
sqlite-graphrag --claude-binary /usr/local/bin/claude --llm-backend claude \
  remember --name x --type note --description "x" --body "y"

# REFRESQUE OAuth do codex quando token expirar
codex login
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini recall "query" --k 5 --json
```


## Seleção de Backend LLM — OpenCode Headless (v1.0.90)
- PASSE `--llm-backend opencode` para spawnar OpenCode CLI headless para embedding e extração
- PASSE `--llm-backend codex,claude,opencode,none` para fallback chain completo
- SAIBA que opencode é a TERCEIRA prioridade no auto-detect: codex > claude > opencode > none
- SAIBA que opencode usa seu PRÓPRIO sistema de auth (NÃO OAuth); `ANTHROPIC_API_KEY` e `OPENAI_API_KEY` NÃO são necessárias
- SAIBA que opencode NÃO tem `--output-schema` ou `--json-schema`; structured output via role-setting prompts + parsing JSON
- SAIBA que output NDJSON do opencode tem 3 tipos de evento: `step_start`, `text` (`.part.text`), `step_finish`
- DEFINA env `SQLITE_GRAPHRAG_OPENCODE_BINARY` para path persistente do binário
- DEFINA env `SQLITE_GRAPHRAG_OPENCODE_MODEL` para seleção persistente de modelo (default: `opencode/big-pickle`)
- DEFINA env `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL` para modelo de embedding persistente
- DEFINA env `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT` para timeout persistente (default: 300s)
- PASSE `--opencode-binary <PATH>` para override de binário por invocação
- PASSE `--opencode-model <MODEL>` para seleção de modelo no ingest/enrich
- PASSE `--opencode-timeout <SECONDS>` para timeout no ingest/enrich
- PASSE `--mode opencode` para pipelines de ingest e enrich
- SAIBA que embedding via opencode usa role-setting prompt "You are an embedding function" para produzir vetores numéricos reais
- SAIBA que `SQLITE_GRAPHRAG_OPENCODE_MODEL` NÃO faz fallback para `SQLITE_GRAPHRAG_LLM_MODEL` (fix de cross-contamination v1.0.90)
- SAIBA que `propagate_opencode_env()` propaga OPENCODE_*, OPENROUTER_*, XDG_*, LANG, TERM, USER, LOGNAME, TMPDIR para subprocesso
- SAIBA modelos gratuitos: `opencode/big-pickle`, `opencode/deepseek-v4-flash-free`, `opencode/mimo-v2.5-free`, `opencode/nemotron-3-ultra-free`, `opencode/north-mini-code-free`
- SAIBA versão mínima: 1.17.0

```bash
# OPENCODE HEADLESS — backend explícito e modelo gratuito
sqlite-graphrag --llm-backend opencode \
  remember --name auth-design --type decision \
  --description "JWT rotation strategy" --body "15-min expiry with refresh"

# OPENCODE COM MODELO ESPECÍFICO
sqlite-graphrag --llm-backend opencode --llm-model opencode/deepseek-v4-flash-free \
  remember --name auth-design --type decision \
  --description "JWT rotation strategy" --body-file auth.md

# FALLBACK CHAIN COMPLETO: codex primeiro, depois claude, depois opencode, depois none
sqlite-graphrag --llm-backend codex,claude,opencode,none --skip-embedding-on-failure \
  remember --name auth-design --type decision \
  --description "JWT rotation strategy" --body-file auth.md

# INGEST COM EXTRAÇÃO OPENCODE
sqlite-graphrag ingest ./docs --mode opencode --recursive --json

# INGEST COM MODELO E TIMEOUT ESPECÍFICOS
sqlite-graphrag ingest ./docs --mode opencode --opencode-model opencode/mimo-v2.5-free \
  --opencode-timeout 600 --recursive --json

# ENRICH COM OPENCODE
sqlite-graphrag enrich --operation memory-bindings --mode opencode --json

# DRY-RUN COM BACKEND OPENCODE
sqlite-graphrag --llm-backend opencode --dry-run-backend \
  remember --name preview --type note --description x --body y | jaq '.backend_invoked'
```

```bash
# BACKEND opencode persistente via env vars
export SQLITE_GRAPHRAG_LLM_BACKEND=opencode
export SQLITE_GRAPHRAG_OPENCODE_MODEL=opencode/big-pickle
export SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL=opencode/big-pickle

# OVERRIDE de path do binário opencode
export SQLITE_GRAPHRAG_OPENCODE_BINARY=~/.opencode/bin/opencode
```


## CRUD — Caminho de Escrita (remember, remember-batch, ingest)
- INVOKE `remember --name <kebab> --type <kind> --description <text> --body-stdin` para corpos longos
- INVOKE `remember --name <kebab> --body-file <path>` para evitar escape shell
- INVOKE `remember --name <kebab> --body <text>` para corpos curtos
- PASSE `--force-merge` para updates idempotentes e restauração de soft-deleted
- PASSE `--clear-body` para limpar corpo durante update com `--force-merge`
- PASSE `--dry-run` para validar inputs sem persistir
- PASSE `--max-rss-mb <MiB>` para abortar quando RSS exceder threshold (default 8192)
- RESPEITE limite de 512000 bytes e 512 chunks por corpo
- INVOKE `remember --graph-stdin` para anexar `{body, entities, relationships}` em único JSON
- PASSE entities como `[{name, entity_type}]` com kebab-case ASCII
- PASSE relationships como `[{source, target, relation, strength}]` onde `strength ∈ [0.0, 1.0]`
- USE `--enable-ner` para extração de entidades URL-regex (URL-regex APENAS desde v1.0.79)
- NUNCA envie `entity_type` e `type` juntos no mesmo objeto JSON
- NUNCA use `--gliner-variant` (no-op desde v1.0.79)
- INVOKE `remember-batch` para 10+ memórias via NDJSON stdin
- ESPERE evento por item: `name`, `status ∈ {created, updated, skipped, failed}`, `memory_id?`, `error?`, `elapsed_ms`
- ESPERE linha de summary: `total`, `created`, `updated`, `skipped`, `failed`, `elapsed_ms`
- INVOKE `ingest <DIR> --recursive --pattern "*.md"` para importar diretório
- PASSE `--type <kind>` para aplicar mesmo tipo a todos arquivos ingeridos
- RESPEITE cap `--max-files 10000` como validação all-or-nothing
- USE `--fail-fast` para parar na primeira falha por arquivo
- USE `--max-name-length N` para sobrescrever truncamento de nomes em 60 chars
- ESPERE linha NDJSON por arquivo: `file`, `name`, `status`, `truncated`, `original_name?`, `memory_id?`, `action?`, `error?`
- ESPERE linha de summary: `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- USE `--llm-parallelism N` em `ingest` (default 2, clamp [1, 32])
- DISTINGA `--max-concurrency N` (fan-out CLI) de `--ingest-parallelism N` (per-file extract+embed)
- USE `--auto-describe` (default true desde v1.0.89) para extrair descrição da primeira linha significativa do corpo; opt-out via `--no-auto-describe`
- INVOKE `ingest --mode claude-code` para extração curada por LLM
- INVOKE `ingest --mode codex` para extração curada por OpenAI Codex
- ESPERE eventos claude-code: contagem `entities`, contagem `rels`, `cost_usd` (Omite cost para OAuth)
- USE `--resume` para continuar do queue DB após interrupção
- USE `--retry-failed` para retentar apenas arquivos que falharam
- NUNCA use `fd | xargs remember`; use `ingest` em vez disso
- NUNCA misture `--body`, `--body-file`, `--body-stdin`, `--graph-stdin` em única invocação
- NUNCA passe corpo vazio sem entities via `--graph-stdin` (exit 1 desde v1.0.54)
- NUNCA use `--force-merge` em `ingest` (exclusivo de `remember`)
- NUNCA misture tipos diferentes de memória em mesma invocação de `ingest`
```bash
# REMEMBER com backend Codex headless — gpt-5.4-mini
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name decisao-auth --type decision \
  --description "Estrategia JWT com rotacao" --body "15 min de expiracao com refresh"

# REMEMBER com backend Claude Code headless — claude-sonnet-4-6
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name decisao-auth --type decision \
  --description "Estrategia JWT com rotacao" --body "15 min de expiracao com refresh"

# REMEMBER com backend OpenCode headless — opencode/big-pickle
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  remember --name decisao-auth --type decision \
  --description "Estrategia JWT com rotacao" --body "15 min de expiracao com refresh"

# REMEMBER Codex exclusivo — corpo de arquivo, falha em erro sem fallback
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name decisao-auth --type decision \
  --description "Estrategia JWT" --body-file auth.md

# REMEMBER Codex-primeiro com fallback claude
sqlite-graphrag --llm-backend codex,claude --llm-model gpt-5.5 \
  remember --name decisao-auth --type decision \
  --description "Estrategia JWT" --body-file auth.md

# REMEMBER corpo longo via stdin (Claude headless)
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name design-cache --type decision \
  --description "Cache LRU com TTL 5 min" --body-stdin <<'EOF'
Escolhemos cache LRU porque:
- TTL de 5 min equilibra frescor e custo
- Eviction LRU evita estouro de memoria
EOF

# REMEMBER com grafo anexado via --graph-stdin (Codex headless)
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name nota-grafo --type note \
  --graph-stdin <<'EOF'
{"body":"JWT usa servico de auth","entities":[{"name":"jwt","entity_type":"concept"},{"name":"servico-auth","entity_type":"tool"}],"relationships":[{"source":"jwt","target":"servico-auth","relation":"uses","strength":0.8}]}
EOF

# REMEMBER-BATCH 10+ memorias via NDJSON (Codex headless)
printf '%s\n' \
  '{"name":"nota-a","type":"note","description":"a","body":"corpo a"}' \
  '{"name":"nota-b","type":"note","description":"b","body":"corpo b"}' \
  | sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
    remember-batch --json | jaq -c 'select(.status) // {summary: .}'

# REMEMBER-BATCH com backend Claude headless
printf '%s\n' \
  '{"name":"nota-c","type":"note","description":"c","body":"corpo c"}' \
  | sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
    remember-batch --json

# REMEMBER-BATCH com backend OpenCode headless
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  remember-batch --json < batch.ndjson | jaq -c 'select(.summary != true)'

# INGEST com extracao Codex
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  ingest ./docs --mode codex --recursive --pattern "*.md" --json

# INGEST com extracao Claude Code
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  ingest ./docs --mode claude-code --recursive --pattern "*.md" --json

# INGEST com extração OpenCode
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  ingest ./docs --mode opencode --recursive --pattern "*.md" --json

# INGEST com auto-describe e retomada de fila
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  ingest ./corpus --mode codex --recursive --auto-describe --resume --json \
  | jaq -c 'select(.status == "done") | {file, entities, rels}'
```


## CRUD — Leitura, Histórico, Atualização
- INVOKE `read --name <kebab>` para fetch O(1) por nome
- INVOKE `read --id <N>` para lookup direto por memory_id
- INVOKE `read --with-graph` para incluir entidades e relacionamentos vinculados
- PARSEE campos `body`, `description`, `created_at_iso`, `updated_at_iso`
- TRATE exit code 4 como memória não encontrada no namespace
- ESPERE v1.0.85 G55 mensagem bilíngue: `--lang en` emite `Memory not found`, `--lang pt` emite `Memória não encontrada`
- INVOKE `list --type <kind> --limit N` para filtrar por tipo de memória
- USE `--offset N` para paginar datasets grandes
- USE `--include-deleted` para incluir memórias soft-deleted
- ESPERE resposta de `list`: `items[]`, `total_count`, `truncated`, `body_length`, `elapsed_ms`
- INVOKE `history --name <n>` para listar versões em ordem cronológica reversa
- USE `--diff` para incluir estatísticas de diff de caracteres entre versões
- ESPERE `versions[]`: `version`, `created_at_iso`, `body_length`, `deleted?`, `changes?`
- INVOKE `edit --name <n> --body-file <path>` para atualizar corpo de arquivo
- USE `--description <text>` para atualizar apenas descrição
- USE `--type <kind>` para mudar tipo de memória sem recriar (v1.0.66)
- USE `--force-reembed` para regenerar embedding sem mudança de corpo (v1.0.79)
- USE `--llm-parallelism N` em `edit` (default 4, clamp [1, 32])
- USE `--expected-updated-at <ts>` para optimistic locking
- TRATE exit code 3 como conflito de optimistic lock; recarregue `read --json` e retente
- INVOKE `rename --from <old> --to <new>` para renomear preservando histórico
- TRATE exit 1 quando novo nome for igual ao antigo (v1.0.64)
- INVOKE `restore --name <n> --version <N>` para restaurar versão antiga
- OMITA `--version` para selecionar última versão não-restore automaticamente
- ESPERE que cada `edit` ou `restore` crie nova versão imutável
- ESPERE correção de desync do FTS5 aplicada (v1.0.56); memórias editadas ficam imediatamente localizáveis
- NUNCA pule optimistic locking em pipelines concorrentes
```bash
# READ por nome — fetch O(1), SEM embedding (leitura pura)
sqlite-graphrag read --name decisao-auth --json | jaq '{name, description, body_length}'

# READ por id com grafo vinculado
sqlite-graphrag read --id 42 --with-graph --json | jaq '{name, entities: (.entities | length)}'

# LIST por tipo com paginacao, SEM embedding
sqlite-graphrag list --type decision --limit 50 --offset 0 --json | jaq '.items[].name'

# HISTORY com diff de caracteres entre versoes
sqlite-graphrag history --name decisao-auth --diff --json | jaq '.versions[] | {version, body_length, changes}'

# EDIT do corpo via arquivo — RE-EMBEDA, use backend Codex
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  edit --name decisao-auth --body-file revisado.md

# EDIT do corpo via arquivo — RE-EMBEDA, use backend Claude
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  edit --name decisao-auth --body-file revisado.md

# EDIT do corpo via arquivo — RE-EMBEDA, use backend OpenCode
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  edit --name decisao-auth --body-file revisado.md

# EDIT apenas descricao — NÃO re-embeda, SEM backend
sqlite-graphrag edit --name decisao-auth --description "JWT 15min com refresh HTTP-only"

# EDIT com force-reembed — regenera embedding sem mudar corpo (Codex)
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  edit --name decisao-auth --force-reembed

# EDIT com force-reembed — regenera embedding sem mudar corpo (OpenCode)
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  edit --name decisao-auth --force-reembed

# EDIT com optimistic locking — TRATE exit 3 como conflito
TS=$(sqlite-graphrag read --name decisao-auth --json | jaq -r '.updated_at_iso')
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  edit --name decisao-auth --description "novo" --expected-updated-at "$TS" || echo "exit=$?"

# RENAME preservando historico, SEM embedding
sqlite-graphrag rename --from decisao-auth --to decisao-autenticacao --json

# RESTORE versao antiga, SEM embedding
sqlite-graphrag restore --name decisao-autenticacao --version 2 --json
```


## CRUD — Deleção (forget, purge, unlink, prune, cleanup)
- INVOKE `forget --name <n>` para soft-delete reversível
- ESPERE que `forget` desapareça das saídas de `recall` e `list`
- TRATE exit 4 como memória ausente (desde v1.0.52)
- INVOKE `restore` para reverter soft-delete antes de qualquer purge
- INVOKE `purge --retention-days <N> --yes` para deleção física
- USE `--dry-run` primeiro para auditar contagem
- ESPERE retenção default de 90 dias para memórias soft-deleted
- INVOKE `unlink --from <a> --to <b> --relation <type>` para remoção direcionada de aresta
- OMITA `--relation` para remover todas arestas entre `--from` e `--to`
- USE `--entity <name> --all` para remover em massa todos relacionamentos de uma entidade
- TRATE exit code 4 como aresta inexistente
- INVOKE `prune-relations --relation <type> --yes` para deleção em massa de relacionamentos
- USE `--show-entities` com `--dry-run` para listar nomes de entidades afetadas
- INVOKE `cleanup-orphans --dry-run` para auditar entidades órfãs
- APLIQUE `--yes` em pipelines automatizados para `cleanup-orphans`
- INVOKE `prune-ner --entity <n>` para remover bindings NER de entidade específica
- INVOKE `prune-ner --all --yes` para remover todos bindings NER no namespace
- USE pipeline padrão: bulk `forget` depois `cleanup-orphans --yes` depois `vacuum --json`
- NUNCA delete manualmente via shell `sqlite3`; use apenas comandos do binário
```bash
# FORGET soft-delete reversivel, SEM embedding
sqlite-graphrag forget --name design-antigo --json

# PURGE deleção física — SEMPRE faça dry-run primeiro
sqlite-graphrag purge --retention-days 30 --yes --dry-run
sqlite-graphrag purge --retention-days 30 --yes --json

# UNLINK remoção direcionada de aresta
sqlite-graphrag unlink --from jwt --to servico-auth --relation uses --json

# UNLINK em massa de todos relacionamentos de uma entidade
sqlite-graphrag unlink --entity jwt --all --json

# PRUNE-RELATIONS em massa por tipo — dry-run lista entidades afetadas
sqlite-graphrag prune-relations --relation mentions --yes --dry-run --show-entities
sqlite-graphrag prune-relations --relation mentions --yes --json

# CLEANUP-ORPHANS auditoria depois execução
sqlite-graphrag cleanup-orphans --dry-run --json
sqlite-graphrag cleanup-orphans --yes --json

# PIPELINE padrão de limpeza
sqlite-graphrag forget --name obsoleto
sqlite-graphrag cleanup-orphans --yes --json
sqlite-graphrag vacuum --json
```


## Grafo de Entidades (link, graph, memory-entities, rename, delete, merge, reclassify, normalize)
- INVOKE `link --from <a> --to <b> --relation <type>` para criar aresta
- PASSE `--create-missing` para auto-criar entidades inexistentes durante link
- PASSE `--entity-type <kind>` para entidades auto-criadas (default `concept`)
- PASSE `--weight <float>` para peso da aresta (default 0.5)
- USE `--strict-relations` para falhar em tipos de relação não-canônicos
- USE `--max-entity-degree N` para avisar quando entidade excede N conexões
- INVOKE `graph entities --json` para listar todas entidades
- ACESSE via `.entities[]` (campo é `entities` NÃO `items`)
- FILTRE via `--entity-type <kind>`
- ORDENE via `--sort-by degree|name|created_at` (default `name`)
- DEFINA direção via `--order asc|desc` (default `asc`)
- PAGINE via `--limit N --offset N`
- INVOKE `graph stats --json` para inspecionar `node_count`, `edge_count`, `avg_degree`, `max_degree`
- INVOKE `graph traverse --from <root> --depth <N>` para travessia de subgrafo
- ESPERE `hops[]`: `entity`, `relation`, `direction`, `weight`, `depth`
- TRATE exit 4 como entidade raiz inexistente
- USE `--format json|dot|mermaid` com `--output <path>` para exportar grafo
- INVOKE `memory-entities --name <memory>` para lookup forward de entidades
- INVOKE `memory-entities --entity <name>` para lookup reverso de memórias
- INVOKE `rename-entity --name <old> --new-name <new>` para renomear entidade
- TRATE exit 4 como entidade não encontrada
- TRATE exit 1 se novo nome falhar validação
- INVOKE `delete-entity --name <n> --cascade` para remover entidade e todos bindings
- PASSE `--cascade` que é OBRIGATÓRIO quando entidade tem relacionamentos (senão exit 1)
- INVOKE `merge-entities --names "a,b,c" --into <target>` para mesclar entidades
- INVOKE `reclassify --name <n> --new-type <kind>` para reclassificação individual
- INVOKE `reclassify --from-type <old> --to-type <new> --batch` para reclassificação em massa
- INVOKE `reclassify-relation --from-relation <old> --to-relation <new> --batch`
- INVOKE `normalize-entities --yes` para normalizar todos nomes para kebab-case ASCII
- VALIDE nomes: mínimo 2 chars, sem newlines, sem ALL_CAPS curtos (4 chars ou menos rejeitados desde fix BUG-13 v1.0.88)
- NORMALIZE nomes via NFKD depois ASCII depois lowercase depois hífens
- CONHEÇA as RELAÇÕES canônicas: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- CONHEÇA o MAPEAMENTO não-canônico: `adds|creates → causes`, `implements → supports`, `blocks → contradicts`, `tested-by → related`, `part-of → applies-to`
- CONHEÇA os TIPOS canônicos de entidade: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- NUNCA use `mentions` como relação padrão (adiciona ruído)
- NUNCA persista estado efêmero em entidades
```bash
# LINK cria aresta com auto-criacao, SEM embedding
sqlite-graphrag link --from jwt --to servico-auth --relation uses \
  --weight 0.8 --create-missing --entity-type tool --json

# GRAPH ENTITIES listagem ordenada por grau, SEM embedding
sqlite-graphrag graph entities --sort-by degree --order desc --limit 10 --json \
  | jaq -r '.entities[] | "\(.name) grau=\(.degree)"'

# GRAPH STATS inspeção de densidade ANTES de travessia
sqlite-graphrag graph stats --json | jaq '{node_count, edge_count, avg_degree, max_degree}'

# GRAPH TRAVERSE subgrafo de 2 hops
sqlite-graphrag graph traverse --from jwt --depth 2 --json \
  | jaq -r '.hops[] | "\(.entity) \(.relation) (\(.depth))"'

# GRAPH exportacao para mermaid
sqlite-graphrag graph --format mermaid --output grafo.mmd

# MEMORY-ENTITIES lookup forward e reverso
sqlite-graphrag memory-entities --name decisao-auth --json | jaq '.entities[].name'
sqlite-graphrag memory-entities --entity jwt --json | jaq '.memories[].name'

# RENAME-ENTITY preservando relacionamentos
sqlite-graphrag rename-entity --name jwt --new-name json-web-token --json

# DELETE-ENTITY com cascade OBRIGATORIO se tem relacionamentos
sqlite-graphrag delete-entity --name obsoleto --cascade --json

# MERGE-ENTITIES mescla duplicatas
sqlite-graphrag merge-entities --names "auth,authentication,autenticacao" --into auth --json

# RECLASSIFY individual e em massa
sqlite-graphrag reclassify --name jwt --new-type concept --json
sqlite-graphrag reclassify --from-type tool --to-type concept --batch --json

# NORMALIZE-ENTITIES para kebab-case ASCII
sqlite-graphrag normalize-entities --yes --json
```


## Busca GraphRAG (recall, hybrid-search, related, deep-research, enrich)
- USE o padrão canônico de três camadas: `hybrid-search` depois `read --name` depois `related|graph traverse`
- INVOKE `recall <query> --k N` para busca semântica pura KNN
- PASSE `--no-graph` para desabilitar expansão automática de grafo
- INTERPRETE `distance` crescente como similaridade decrescente
- INTERPRETE `score` como `1.0 - distance` clamped em `[0.0, 1.0]`
- ESPERE `source ∈ {direct, graph}` e `graph_depth` para resultados de grafo
- ESPERE resposta: `direct_matches[]`, `graph_matches[]`, `results[]`, `elapsed_ms`
- INVOKE `hybrid-search <query> --k N` para fusão FTS5 e KNN via RRF
- PASSE `--rrf-k 60` para constante RRF padrão
- PASSE `--weight-vec 1.0` e `--weight-fts 1.0` para fusão balanceada
- USE `--with-graph --max-hops 2 --min-weight 0.3` para expansão de grafo
- ESPERE resposta `hybrid-search`: `results[]`, `graph_matches[]`, `fts_degraded`, `vec_degraded_reason?`, `backend_invoked`, `elapsed_ms`
- LEIA TANTO `results[]` QUANTO `graph_matches[]` quando `--with-graph` ativo
- INVOKE `related <name> --hops N` para travessia multi-hop a partir de memória
- PASSE `--relation <type>` para filtrar travessia por relação
- ESPERE `hop_distance` explícito por hop
- INVOKE `deep-research "<query>" --k 20` para pesquisa paralela multi-hop
- PASSE `--max-sub-queries 7` para cap de decomposição de query
- PASSE `--max-hops 3 --min-weight 0.3 --max-results 50` para travessia de grafo
- PASSE `--with-bodies` para incluir corpos completos de memórias nos resultados
- ESPERE resposta: `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context?`, `stats`
- INVOKE `enrich --operation <op> --mode claude-code` para qualidade de grafo via LLM
- CONHEÇA as OPERAÇÕES: `memory-bindings`, `entity-descriptions`, `body-enrich` (Jaccard >=0.7), `re-embed --limit N --resume`
- PASSE `--llm-parallelism N` para controlar subprocessos LLM concorrentes
- PASSE `--max-cost-usd N` para cap de gasto acumulado (ignorado para usuários OAuth)
- PASSE `--resume` e `--retry-failed` para resiliência a crash
- USE `--dry-run` para preview sem spawnar LLM
- USE query AMPLA para `recall --k 5`
- USE query MISTA de tokens para `hybrid-search --k 10`
- USE query MISTA com grafo para `hybrid-search --with-graph --max-hops 2`
- USE query EXPLORATÓRIA de memória para `related --hops 2`
- USE query EXPLORATÓRIA de entidade para `graph traverse --depth 2`
- NUNCA confunda `distance` com `combined_score` em ranking
- NUNCA aumente `--hops` sem inspecionar `graph stats` antes
- NUNCA pule camada 2 quando snippet for insuficiente
- NUNCA leia apenas `.results[]` quando `--with-graph` estiver ativo
```bash
# RECALL com embedding Codex — gpt-5.4-mini
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  recall "autenticacao JWT" --k 5 --json | jaq '.results[] | {name, score, source}'

# RECALL com embedding Claude — claude-sonnet-4-6
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  recall "autenticacao JWT" --k 5 --json | jaq '.results[] | {name, score, source}'

# RECALL com embedding OpenCode — opencode/big-pickle
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  recall "autenticacao JWT" --k 5 --json | jaq '.results[] | {name, score, source}'

# RECALL KNN puro sem expansao de grafo (Codex)
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  recall "fluxo de refresh" --k 5 --no-graph --json | jaq '.direct_matches[].name'

# HYBRID-SEARCH com embedding Codex e expansao de grafo
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  hybrid-search "fluxo de auth" --k 10 --with-graph --max-hops 2 --json \
  | jaq -r '(.results[].name), (.graph_matches[].name)' | sort -u

# HYBRID-SEARCH com embedding Claude e fusao balanceada
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  hybrid-search "fluxo de auth" --k 10 --rrf-k 60 --weight-vec 1.0 --weight-fts 1.0 --json \
  | jaq '{backend_invoked, fts_degraded, results: (.results | length)}'

# HYBRID-SEARCH com embedding OpenCode e expansão de grafo
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  hybrid-search "fluxo de auth" --k 10 --with-graph --json \
  | jaq -r '(.results[].name), (.graph_matches[].name)' | sort -u

# RELATED travessia multi-hop a partir de memoria, SEM embedding
sqlite-graphrag related decisao-auth --hops 2 --relation uses --json \
  | jaq '.results[] | {name, hop_distance}'

# DEEP-RESEARCH com backend Codex — pesquisa paralela multi-hop
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  deep-research "Como o binario se autentica em providers OAuth?" \
  --k 20 --max-hops 3 --max-sub-queries 5 --with-bodies --json \
  | jaq '{stats, evidence_chains: (.evidence_chains | length)}'

# DEEP-RESEARCH com backend Claude
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  deep-research "estrategia de cache e TTL" --k 20 --max-hops 3 --json \
  | jaq '.sub_queries[]'

# DEEP-RESEARCH com backend OpenCode
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  deep-research "estrategia de cache e TTL" --k 20 --max-hops 3 --json \
  | jaq '.sub_queries[]'

# ENRICH re-embed com backend Codex
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  enrich --operation re-embed --limit 100 --resume --json

# ENRICH memory-bindings com backend Claude
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  enrich --operation memory-bindings --mode claude-code --json

# ENRICH memory-bindings com backend OpenCode
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  enrich --operation memory-bindings --mode opencode --json

# ENRICH dry-run para preview sem spawnar LLM
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  enrich --operation entity-descriptions --mode claude-code --dry-run --json
```


## Superfície v1.0.86+ (pending, slots, embedding, llm-backend, shutdown)
- INVOKE `pending list --filter-status queued` para inspecionar fila de checkpoint de três estágios do remember
- INVOKE `pending show <id>` para inspecionar linha única de checkpoint
- INVOKE `pending cleanup --yes` para remover linhas em estado terminal
- SAIBA que é RESPALDADO pela tabela `pending_memories` criada pela migração V014 (ADR-0036)
- PASSE `--db <PATH>` em `pending list`/`pending show` (v1.0.89, ADR-0049)
- INVOKE `pending-embeddings list` para inspecionar fila de retry de embeddings que falharam
- INVOKE `pending-embeddings process` para reprocessar com próximo backend
- SAIBA que é RESPALDADO pela tabela `pending_embeddings` criada pela migração V015 (ADR-0040)
- INVOKE `slots status` para inspecionar semáforo de slots host-wide
- INVOKE `slots release --slot-id <N> --yes` para colher slots órfãos
- SAIBA que LOCK usa `fs4 = "0.9"` com `fcntl(F_SETLK)` em Unix e `LockFileEx` em Windows (ADR-0039)
- INVOKE `embedding status` para contagens agregadas por status
- INVOKE `embedding list` para inspeção por entrada
- PASSE `--db <PATH>` em `embedding status`/`embedding list`/`embedding abandon` (v1.0.89, ADR-0049)
- PASSE `--llm-backend codex,claude` para codex-primeiro com fallback claude (ADR-0038)
- PASSE `--llm-backend codex,claude,none` para fallback de embedding null
- SAIBA que o DEFAULT de `--llm-backend` é `codex`
- PASSE `--llm-fallback-mode <claude|codex>` para trocar backend mid-job em rate-limit
- PASSE `--max-concurrency N` flag global para limitar invocações CLI pesadas concorrentes
- PASSE `--wait-lock SECS` flag global para ampliar janela de aquisição de lock
- PASSE `--llm-parallelism N` flag global para cap de fan-out de subprocessos de embedding (default 4, clamp [1, 32])
- PASSE `--ingest-parallelism N` para controlar paralelismo extract+embed por arquivo em `ingest`
- PASSE `--graceful-shutdown-secs N` para reservar orçamento de cleanup antes de SIGKILL
- PASSE `--skip-embedding-on-failure` APENAS quando `--llm-backend …,none`
- PASSE ADR-0041 `--strict-env-clear` para descartar credenciais de provider customizado em subprocesso
- PASSE `--dry-run-backend` para planejar operação de backend sem executá-la (preview idempotente)
- PARSEE campo `backend_invoked` nos envelopes de recall, hybrid-search, remember, edit, ingest, enrich, read para confirmar backend efetivo
- LEIA `vec_degraded_reason` nos envelopes de recall/hybrid-search quando caminho vec estiver degradado
- SAIBA que backend claude divide-se em embedder local via `embed_via_claude_local` (zero-token, compatível com OAuth)
- USE `LlmEmbeddingBuilder` para compor pipeline de embedding: `with_backend(Codex).or_fallback(Claude).or_skip()`
- INVOKE `codex-models --json` desde v1.0.89 para emitir envelope JSON `{"action":"codex_models","count":N,"default":"...","models":[...]}` (alias no-op)
- EXECUTE `codex login` após upgrade para refrescar refresh token OAuth (incidente 2026-06-14)
- AÇÃO do operador para OAuth stale: `codex login` depois retry
```bash
# PENDING list e show da fila de checkpoint, com --db explicito
sqlite-graphrag --db /data/projeto.sqlite pending list --filter-status queued --json
sqlite-graphrag --db /data/projeto.sqlite pending show 7 --json
sqlite-graphrag pending cleanup --yes --json

# PENDING-EMBEDDINGS inspecao e reprocessamento com proximo backend
sqlite-graphrag pending-embeddings list --json | jaq '.[] | {id, status}'
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  pending-embeddings process --json

# SLOTS status e colheita de slot orfao
sqlite-graphrag slots status --json | jaq '{max_concurrency, acquired, waiting}'
sqlite-graphrag slots release --slot-id 3 --yes --json

# EMBEDDING status e listagem com --db explicito
sqlite-graphrag --db /data/projeto.sqlite embedding status --json | jaq '{pending, done, failed}'
sqlite-graphrag --db /data/projeto.sqlite embedding list --json | jaq '.[] | {id, status}'

# CODEX-MODELS whitelist de modelos (alias no-op)
sqlite-graphrag codex-models --json | jaq '{count, default, models: .models[:3]}'

# REFRESQUE OAuth do codex em caso de token expirado
codex login
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini recall "query" --k 5 --json
```


## Camada de Validação Pre-Flight v1.0.87+ (ADR-0045, GAP-META-005)
- SAIBA que `src/spawn/preflight.rs` porta todo spawn de subprocesso LLM através de 7 guards ANTES do fork
- SAIBA que exit code 16 (`EX_CONFIG`) é o código universal de falha preflight (adicionado v1.0.87)
- SAIBA que 7 guards rodam em ordem: `check_argv_size`, `check_binary_exists`, `check_mcp_config_inline`, `check_mcp_config_path`, `check_walkup_mcp_json`, `check_output_buffer`, `check_claude_config_dir`
- SAIBA que `check_argv_size` rejeita argv excedendo `ARG_MAX - 4096` bytes (margem para env vars do kernel)
- SAIBA que `check_binary_exists` aborta quando `claude` ou `codex` não está em PATH
- SAIBA que `check_mcp_config_inline` reescreve `--mcp-config '{}'` literal para tempfile com `{"mcpServers":{}}` (Claude Code 2.1.177 rejeita a forma literal)
- SAIBA que `check_mcp_config_path` valida conteúdo JSON de arquivos `--mcp-config <PATH>`
- SAIBA que `check_walkup_mcp_json` rejeita `.mcp.json` inválido na cadeia ancestral do CWD (até 16 níveis via `Path::ancestors()`)
- SAIBA que `check_output_buffer` dobra buffer do parser acima de 64 KB para lidar com saídas grandes
- SAIBA que `check_claude_config_dir` evita vazamento MCP de `~/.claude/` user-level
- DEFINA `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` APENAS em emergências; bypass reverte para `Command::spawn()` direto e herda todas as 5 classes de bug GAP-META-005
- LEIA envelope JSON `AppError::PreFlightFailed(PreFlightError)` para remediação específica por variante
- SAIBA que fix BUG-11 v1.0.88 garante que falha preflight propaga via `embed_via_backend_strict`; NUNCA espere sucesso silencioso quando preflight falha
- NUNCA prossiga após exit code 16 sem resolver a variante específica reportada
```bash
# DIAGNOSTIQUE falha preflight — config MCP ruim aborta com exit 16
CLAUDE_CONFIG_DIR=/tmp/bad-mcp sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name teste --type note --description x --body y 2>&1 || echo "exit=$?"

# BYPASS de emergencia do preflight — APENAS em emergencias
SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1 sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name teste --type note --description x --body y

# LEIA envelope de erro preflight para remediacao por variante
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name teste --type note --description x --body y --json 2>/dev/null \
  | jaq 'select(.error == true) | {code, message}'
```


## Hotfixes v1.0.88+ (BUG-11, BUG-12, BUG-13)
- SAIBA que BUG-11 (CRÍTICO) foi CORRIGIDO: falha preflight em `extract/llm_embedding.rs:563-565` agora propaga para `remember` via `embed_via_backend_strict` em vez de persistência silenciosa com `backend_invoked: "none"` e zero chunks
- REPRODUZA o fix BUG-11: `CLAUDE_CONFIG_DIR=/tmp/bad-config-with-mcp sqlite-graphrag remember --name X --type note --description x --body y` retorna exit 11 com envelope JSON de erro
- SAIBA que BUG-12 (MÉDIO) foi CORRIGIDO: enforço OAuth-only emite exatamente 1 linha stderr (eram 2 — `eprintln!` duplicado removido em `src/output.rs`)
- VERIFIQUE o fix BUG-12: `ANTHROPIC_API_KEY=sk-test sqlite-graphrag init` emite 1 linha stderr
- SAIBA que BUG-13 (MÉDIO) foi CORRIGIDO: `link --create-missing` valida nomes de entidade ANTES de normalizar (estava bypassando validação; abreviações ALL_CAPS de 3-4 chars como `API`, `WAL`, `RUST` agora corretamente rejeitadas via CLI casando com o caminho `remember --graph-stdin`)
- VERIFIQUE o fix BUG-13: `sqlite-graphrag link --from api --to service --create-missing --relation uses` retorna exit 1 com erro de validação
- INVOKE a variante `AppError::PreFlightFailed(PreFlightError)` no tratamento de erros; exit code 16, `is_permanent() == true`
```bash
# REPRODUZA fix BUG-11 — preflight ruim propaga exit 11 (Codex headless)
CLAUDE_CONFIG_DIR=/tmp/bad-config-with-mcp sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name X --type note --description x --body y --json 2>/dev/null \
  | jaq 'select(.error == true)'; echo "exit=$?"

# VERIFIQUE fix BUG-12 — OAuth-only emite exatamente 1 linha stderr
ANTHROPIC_API_KEY=sk-test sqlite-graphrag init 2>&1 1>/dev/null | wc -l

# VERIFIQUE fix BUG-13 — ALL_CAPS curto rejeitado em link, SEM embedding
sqlite-graphrag link --from api --to service --create-missing --relation uses 2>&1 || echo "exit=$?"
```


## Remediação de Deadlock de Embedding v1.0.89+ (ADR-0050)
- PASSE `--llm-model <MODEL>` como flag global para selecionar modelo de embedding para TODOS os backends (v1.0.89, ADR-0050)
- SAIBA que o MODELO padrão para backend codex é `gpt-5.5`; para backend claude é `claude-sonnet-4-6`
- DEFINA env `SQLITE_GRAPHRAG_LLM_MODEL` como override persistente para `--llm-model`
- PASSE `--codex-binary <PATH>` para sobrescrever localização do binário codex (v1.0.89, ADR-0050)
- DEFINA env `SQLITE_GRAPHRAG_CODEX_BINARY` como override persistente para `--codex-binary`
- PASSE `--claude-binary <PATH>` para sobrescrever localização do binário claude (propagado via set_var desde v1.0.89)
- PASSE `--skip-embedding-on-failure` para retornar exit 0 quando embedding LLM falha (cabeado end-to-end desde v1.0.89, ADR-0050)
- SAIBA que 7 flags CLI mortas foram corrigidas na v1.0.89 via propagação `set_var` em `main.rs`: `--llm-model`, `--llm-fallback`, `--skip-embedding-on-failure`, `--claude-binary`, `--codex-binary`, `--llm-max-host-concurrency`, `--llm-slot-wait-secs`
- SAIBA que `deep-research` e `remember-batch` agora recebem `llm_backend` do main.rs (v1.0.89, ADR-0050)
- SAIBA que timeout adaptativo escala com tamanho do batch: `base + 15s × (batch_size - 1)` (v1.0.89, ADR-0050)
- SAIBA que erros de OAuth expirado agora incluem hint acionável: "execute codex login" ou "atualize OAuth do claude" (v1.0.89)
- SAIBA que `BoolishValueParser` aceita `1/yes/on/true` e `0/no/off/false` para env vars booleanas (v1.0.89, ADR-0050)
- SAIBA que flag `--yes` em `slots release`, `purge`, `cleanup-orphans` foi cabeada end-to-end (v1.0.89, BUG-YES-FLAG-IGNORED)
```bash
# SELECIONE modelo via flag global para QUALQUER backend (Codex)
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  recall "query" --k 5 --json | jaq '.backend_invoked'

# SELECIONE modelo via flag global para QUALQUER backend (Claude)
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  hybrid-search "query" --k 10 --json | jaq '.backend_invoked'

# DEFINA modelo persistente via env var
export SQLITE_GRAPHRAG_LLM_MODEL=gpt-5.4-mini
sqlite-graphrag --llm-backend codex remember --name nota --type note --description x --body y

# SOBRESCREVA path do binario codex via flag
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  --codex-binary /usr/local/bin/codex recall "query" --k 5 --json

# SKIP-EMBEDDING-ON-FAILURE retorna exit 0 quando embedding falha
sqlite-graphrag --llm-backend codex,claude,none --skip-embedding-on-failure \
  remember --name nota --type note --description x --body y; echo "exit=$?"
```


## Drift de Schema e Paridade de Flag v1.0.89+ (ADR-0048, ADR-0049)
- SAIBA que `health.schema.json` foi regenerado via macro derive `schemars` (ADR-0048); `additionalProperties: true` conforme política Must-Ignore (RFC 7493 I-JSON)
- SAIBA que 17 novos campos foram adicionados ao envelope `health` desde v1.0.88: `fts_query_ok`, `vec_memories_missing`, `vec_memories_orphaned`, `sqlite_version`, `mentions_ratio`, `mentions_warning`, `top_relation`, `top_relation_ratio`, `applies_to_ratio`, `relation_concentration_warning`, `super_hub_count`, `super_hub_warning`, `top_hub_entity`, `top_hub_degree`, `hub_warning`, `non_normalized_count`, `normalization_warning`
- REGENERE schemas via `cargo run --bin dump-schema` (ordenamento BTreeMap idempotente)
- PASSE `--namespace <NS>` em `health` para filtrar contagens para um namespace
- USE `migrate --dry-run --json` para PREVIEW de migrações pendentes sem aplicar; lista nomes+versões, valida checksums, verifica pré-condições
- USE `codex-models --json` como alias no-op retornando envelope JSON
- USE `--auto-describe` (default true) em `ingest` para extrair descrição da primeira linha significativa do corpo; opt-out via `--no-auto-describe`
- PASSE `--db <PATH>` em `embedding status`/`embedding list`/`embedding abandon`/`pending list`/`pending show` (ADR-0049)
- SAIBA que `--db <PATH>` NÃO é global; cada subcomando aceita independentemente (`clap::Arg::global = true` foi REJEITADO como invasivo)
- TRATE o tamanho do binário como 14.6 MiB stripped ELF (NÃO 6 MB como em docs antigos); veja descrição em `Cargo.toml:6`
```bash
# REGENERE schemas e CONFIRME idempotencia
cargo run --bin dump-schema -- --check
git diff --stat docs/schemas/
cargo run --bin dump-schema  # se --check falhou

# HEALTH escopado para namespace (paridade de flag --db e --namespace)
sqlite-graphrag --db /data/projeto.sqlite health --namespace prod --json \
  | jaq '{integrity_ok, schema_version, counts}'

# MIGRATE dry-run preview SEM aplicar
sqlite-graphrag migrate --dry-run --json | jaq '.would_apply[]? | {name, version}'
```


## Contratos JSON (Top-5 Campos por Comando)
- PARSEE TOP campos `recall`: `results[].name`, `snippet`, `distance`, `score`, `source`
- PARSEE TOP campos `hybrid-search`: `results[].name`, `combined_score`, `vec_rank`, `fts_rank`, `source`
- PARSEE TOP campos `health`: `integrity_ok`, `schema_ok`, `counts`, `wal_size_mb`, `schema_version`
- PARSEE TOP campos `list`: `items[].name`, `type`, `description`, `updated_at_iso`, `deleted_at_iso?`
- PARSEE TOP campos `edit`: `memory_id`, `name`, `action`, `version`, `elapsed_ms`
- PARSEE TOP campos `read`: `name`, `body`, `description`, `created_at_iso`, `updated_at_iso`
- PARSEE TOP campos `forget`: `action`, `forgotten`, `name`, `namespace`, `elapsed_ms`
- PARSEE TOP campos `link`: `action`, `from`, `to`, `relation`, `weight`
- PARSEE TOP campos `graph entities`: `entities[].id`, `name`, `entity_type`, `degree`, `description?`
- PARSEE TOP campos `deep-research`: `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context`, `stats`
- PARSEE EVENTOS NDJSON de `enrich`: `phase`, `name`, `status`, `entities?`, `rels?`, `cost_usd?`, `elapsed_ms?`
- PARSEE TOP campos `pending list`: `id`, `name`, `status`, `created_at`, `namespace`
- PARSEE TOP campos `slots status`: `max_concurrency`, `acquired`, `waiting`, `held_by_pid[]`
- PARSEE TOP campos `embedding status`: `pending`, `processing`, `done`, `failed`, `skipped`
- PARSEE envelopes `remember`/`edit`/`ingest`/`enrich`/`read`: incluem `backend_invoked` e `vec_degraded_reason?`
- SAIBA que `health.schema.json` usa `"additionalProperties": true` conforme política Must-Ignore (RFC 7493 I-JSON) desde v1.0.89 (ADR-0048); os outros 49 schemas em `docs/schemas/` ainda usam `"additionalProperties": false` (Must-Validate) pendentes de regeneração em v1.0.90+
- CONSULTE SCHEMAS completos em `docs/schemas/*.schema.json` (nunca inline schema completo em skill)
```bash
# PARSEE backend_invoked no envelope de recall (Codex)
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  recall "query" --k 5 --json | jaq '{backend_invoked, vec_degraded_reason}'

# PARSEE top-5 campos de hybrid-search
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  hybrid-search "query" --k 10 --json \
  | jaq '.results[] | {name, combined_score, vec_rank, fts_rank, source}'

# PARSEE envelope de health
sqlite-graphrag health --json | jaq '{integrity_ok, schema_ok, counts, wal_size_mb, schema_version}'
```


## Códigos de Saída e Retry
- TRATE EXIT 0 como sucesso; parsee stdout
- TRATE EXIT 1 como erro de validação (peso inválido, self-link, max-files excedido, bypass ALL_CAPS em link)
- TRATE EXIT 2 como erro de parsing de argumento Clap
- TRATE EXIT 3 como conflito de optimistic lock; recarregue `read --json` e retente
- TRATE EXIT 4 como entidade, memória ou versão não encontrada
- TRATE EXIT 5 como erro de namespace
- TRATE EXIT 6 como payload acima do limite de tamanho
- TRATE EXIT 9 como memória duplicada (use `--force-merge` para update ou restore)
- TRATE EXIT 10 como erro de banco; execute `vacuum` e `health`
- TRATE EXIT 11 como falha de embedding (erro de subprocesso LLM, incluindo falha preflight desde fix BUG-11)
- TRATE EXIT 13 como falha parcial de batch; reprocesse apenas os que falharam
- TRATE EXIT 14 como erro de I/O (permissão, disco cheio)
- TRATE EXIT 15 como banco ocupado; amplie `--wait-lock`
- TRATE EXIT 16 como falha de validação preflight (v1.0.87+, ADR-0045); cheque envelope JSON para variante
- TRATE EXIT 19 como SHUTDOWN_EXIT_CODE (ADR-0037); trabalho parcial descartado; RETRY OBRIGATÓRIO
- PARSEE EXIT 19 envelope: `{error:true, code:19, signal, graceful, message}`
- TRATE EXIT 20 como erro interno ou falha de serialização JSON
- TRATE EXIT 75 como slots esgotados OU `JobSingletonLocked`
- PARSEE EXIT 75 de `enrich`/`ingest --mode claude-code|codex|opencode`: `job '(\w+)'.*namespace '(\w+)'`
- RESPEITE EXIT 75 circuit breaker: janela de cooldown por namespace; NÃO retente imediatamente
- TRATE EXIT 77 como pressão de RAM; aguarde memória livre
- NUNCA ignore exit code não-zero como sucesso
- NUNCA reprocesse batch inteiro após exit 13
- NUNCA aumente concorrência após exit 75 ou 77
- NUNCA confunda exit 1 (validação) com exit 9 (duplicada)
- NUNCA trate exit 16 como transitório; corrija o problema preflight subjacente
```bash
# VERIFIQUE exit code ANTES de parsear stdout
sqlite-graphrag read --name inexistente --json; echo "exit=$?"  # exit 4

# TRATE exit 9 (duplicada) com --force-merge para update
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name nota --type note --description x --body y --force-merge

# TRATE exit 11 (falha de embedding) com fallback de backend
sqlite-graphrag --llm-backend codex,claude --llm-model gpt-5.5 \
  remember --name nota --type note --description x --body y; echo "exit=$?"
```


## Concorrência, RAM, Paralelismo, Slots
- RESPEITE teto rígido `2 × nCPUs` para comandos pesados
- TRATE como pesados: `init`, `remember`, `ingest`, `recall`, `hybrid-search`
- DISTINGA `--max-concurrency` (fan-out CLI) de `--ingest-parallelism` (per-file)
- DEFINA `--llm-parallelism N` default 4 em `remember`/`edit`, default 2 em `ingest`
- CLAMP `--llm-parallelism` na faixa `[1, 32]`
- USE `--llm-max-host-concurrency N` para cap de subprocessos LLM cross-process
- USE `--llm-slot-wait-secs N` para esperar slot ou `--llm-slot-no-wait` para abortar
- AMPLIE `--wait-lock SECS` quando contenção for esperada
- ATIVE `SQLITE_GRAPHRAG_LOW_MEMORY=1` para paralelismo unitário (3-4x mais lento)
- USE `--strict-env-clear` (ADR-0041) para preservar apenas `PATH` em compliance
- USE a RECEITA de bypass de SHUTDOWN: prefixe `tests/mock-llm` ao PATH depois sete `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` depois envolva com `setsid -w timeout`
- SAIBA que JOB SINGLETON: `enrich`, `ingest --mode claude-code`, `ingest --mode codex`, `ingest --mode opencode` adquirem singleton por namespace
- USE `--wait-job-singleton SECS` para esperar lock ou `--force-job-singleton` para quebrar lock stale
- LIMITE ingestão paralela em CI para evitar rate limits da LLM
- NUNCA rode `enrich` em paralelo contra mesmo banco
```bash
# LIMITE fan-out de embedding com --llm-parallelism (Codex)
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini --llm-parallelism 8 \
  ingest ./docs --mode codex --recursive --json

# LIMITE concorrencia de invocacoes CLI pesadas
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 --max-concurrency 2 \
  recall "query" --k 5 --json

# AMPLIE wait-lock sob contencao esperada
sqlite-graphrag --wait-lock 30 remember --name nota --type note --description x --body y

# ATIVE low-memory para paralelismo unitario em container restrito
SQLITE_GRAPHRAG_LOW_MEMORY=1 sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  ingest ./docs --mode codex --recursive --json
```


## Manutenção (fts, backup, vacuum, optimize, migrate, export, debug-schema, vec, completions)
- INVOKE `fts rebuild --json` para reconstruir totalmente o índice full-text FTS5
- INVOKE `fts check --json` para executar verificação de integridade FTS5
- INVOKE `fts stats --json` para inspecionar saúde FTS5 (`total_rows`, `fts_functional`)
- INVOKE `optimize --fts-dry-run` para preview de rebuild FTS5
- INVOKE `optimize --fts-progress N` para imprimir progresso a cada N segundos
- PASSE `--no-fts-skip-when-functional` para forçar rebuild FTS5 mesmo quando saudável
- INVOKE `backup --output <path> --json` para backup online seguro via API SQLite
- INVOKE `sync-safe-copy --dest <path>` para snapshot atômico antes de operações críticas
- INVOKE `export --namespace <ns> --type <kind> --json` para exportar memórias como NDJSON
- INVOKE `vacuum --json` após purge grande para reclamar espaço
- INVOKE `migrate --rehash --json` para reparar drift de checksum V002
- INVOKE `migrate --to-llm-only --drop-vec-tables --json` para upgrades v1.0.74/75
- INVOKE `migrate --dry-run --json` para preview de migrações (v1.0.89)
- INVOKE `debug-schema --json` (oculto do `--help`) para inspecionar estado de schema
- INVOKE `completions <bash|zsh|fish|elvish|powershell>` para gerar completions de shell
- INVOKE `vec orphan-list --json` para listar vetores órfãos de memória
- INVOKE `vec purge-orphan --yes --dry-run` para PREVIEW de purge
- INVOKE `vec purge-orphan --yes` para purgar PERMANENTEMENTE órfãos
- INVOKE `vec stats --json` para inspecionar saúde das tabelas vec
- REGENERE schemas via `cargo run --bin dump-schema` (v1.0.89, ADR-0048)
- AGENDE semanal: `purge --retention-days 30 --yes` depois `cleanup-orphans --yes` depois `prune-relations --relation mentions --yes` depois `vacuum --json` depois `optimize --json` depois `sync-safe-copy --dest ~/backups/`
- SAIBA que DESDE v1.0.53 toda escrita executa `PRAGMA wal_checkpoint(TRUNCATE)` após commit
- SE corrupção ocorrer apesar do checkpoint: `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`
```bash
# FTS rebuild, check e stats — todos SEM embedding
sqlite-graphrag fts rebuild --json
sqlite-graphrag fts check --json | jaq '.integrity_ok'
sqlite-graphrag fts stats --json | jaq '{total_rows, fts_functional}'

# BACKUP online seguro e snapshot atomico
sqlite-graphrag backup --output ~/backups/graphrag-$(date +%Y%m%d).sqlite --json
sqlite-graphrag sync-safe-copy --dest ~/backups/snap.sqlite

# EXPORT memorias como NDJSON
sqlite-graphrag export --namespace prod --type decision --json | jaq -c '{name, description}'

# VACUUM e OPTIMIZE apos purge grande
sqlite-graphrag vacuum --json
sqlite-graphrag optimize --json | jaq '.fts_rebuilt'

# VEC orphan-list e purge com dry-run
sqlite-graphrag vec orphan-list --json | jaq 'length'
sqlite-graphrag vec purge-orphan --yes --dry-run --json
sqlite-graphrag vec stats --json

# COMPLETIONS de shell
sqlite-graphrag completions bash > ~/.local/share/bash-completion/completions/sqlite-graphrag
```


## Exemplos Prontos

### Exemplo 1 — Bootstrap de namespace de projeto
```bash
sqlite-graphrag init --namespace meuprojeto
sqlite-graphrag health --json | jaq '.integrity_ok'
sqlite-graphrag health --json | jaq '{schema_version, counts}'
```
- ESPERE: exit 0, `integrity_ok: true`, `schema_version >= 15`

### Exemplo 2 — Armazenar e recuperar memória (backend Codex)
```bash
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name decisao-auth --type decision \
  --description "JWT 15 min de expiração com fluxo de refresh" \
  --body-stdin <<'EOF'
Escolhemos JWT com 15 minutos de expiração porque:
- Refresh tokens são cookies HTTP-only
- 15min reduz blast radius de XSS
- Fluxo de refresh reemite tokens em atividade do usuário
EOF

sqlite-graphrag read --name decisao-auth --json | jaq '{description, body_length}'
```
- ESPERE: memória persistida, body contém texto completo, `body_length` > 100

### Exemplo 3 — Busca híbrida com expansão de grafo (backend Claude)
```bash
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  hybrid-search "autenticação JWT" --k 5 --with-graph --max-hops 2 --json \
  | jaq -r '(.results[] | .name), (.graph_matches[] | .name)' | sort -u
```
- ESPERE: top 5 resultados KNN+FTS5 fundidos mais 0-N vizinhos multi-hop

### Exemplo 3b — Busca híbrida com expansão de grafo (backend OpenCode)
```bash
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  hybrid-search "autenticação JWT" --k 5 --with-graph --max-hops 2 --json \
  | jaq -r '(.results[] | .name), (.graph_matches[] | .name)' | sort -u
```
- ESPERE: top 5 resultados KNN+FTS5 fundidos via OpenCode embedding

### Exemplo 4 — Ingest em massa de pasta de documentação (backend Codex)
```bash
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  ingest ./docs --mode codex --recursive --type document \
  --pattern "*.md" --max-files 1000 --auto-describe --json \
  | jaq -c 'select(.status)' | jaq -s 'group_by(.status) | map({status: .[0].status, count: length})'
```
- ESPERE: progresso NDJSON; summary mostra `files_total`, `files_succeeded`, `files_failed`

### Exemplo 4b — Ingest em massa (backend OpenCode)
```bash
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  ingest ./docs --mode opencode --recursive --type document \
  --pattern "*.md" --auto-describe --json \
  | jaq -c 'select(.status)' | jaq -s 'group_by(.status) | map({status: .[0].status, count: length})'
```
- ESPERE: progresso NDJSON; OpenCode extrai entidades e relações por arquivo

### Exemplo 5 — Travessia de grafo a partir de entidade conhecida
```bash
sqlite-graphrag graph entities --json | jaq -r '.entities[].name' | head -10
sqlite-graphrag graph traverse --from jwt --depth 2 --json | jaq -r '.hops[] | "\(.entity) \(.relation)"'
```
- ESPERE: lista de entidades; travessia mostra vizinhança de 2 hops via relações canônicas

### Exemplo 6 — Pergunta de pesquisa profunda (backend Claude)
```bash
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  deep-research "Como o binário se autentica em providers OAuth?" \
  --k 20 --max-hops 3 --max-sub-queries 5 --json \
  | jaq '{stats, evidence_chains: (.evidence_chains | length)}'
```
- ESPERE: sub-queries decompostas, cadeias de evidência ligando seed ao alvo, graph_context populado

### Exemplo 7 — Extração de entidades curada por LLM (backend Codex)
```bash
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  ingest ./corpus --mode codex --recursive --resume --json \
  | jaq -c 'select(.status == "done") | {file, entities, rels}'
```
- ESPERE: NDJSON por arquivo com `entities` count, `rels` count; `--resume` continua após interrupção

### Exemplo 8 — Diagnosticar falha preflight (exit 16, backend Claude)
```bash
CLAUDE_CONFIG_DIR=/tmp/bad-mcp sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name teste --type note --description x --body y 2>&1
echo "exit=$?"
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name teste --type note --description x --body y 2>&1 || echo "exit=$?"
```
- ESPERE: primeira invocação retorna exit 16 com envelope `AppError::PreFlightFailed`
- ESPERE: segunda invocação sem diretório MCP ruim retorna exit 0

### Exemplo 9 — Recuperação de soft-delete (backend Codex)
```bash
sqlite-graphrag forget --name decisao-auth
sqlite-graphrag history --name decisao-auth --json | jaq '.versions[0].deleted'
sqlite-graphrag restore --name decisao-auth
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  recall "JWT" --k 3 --json | jaq '.results[].name'
```
- ESPERE: soft-delete esconde de recall; restore traz de volta; recall mostra novamente

### Exemplo 10 — Health check com filtro de namespace e tabelas vec
```bash
sqlite-graphrag health --namespace prod --json | jaq '{integrity_ok, schema_version, counts}'
sqlite-graphrag vec stats --json | jaq '.'
sqlite-graphrag embedding status --json | jaq '{pending, done, failed}'
```
- ESPERE: contagens escopadas para o namespace `prod`; saúde de tabelas vec; status da fila de embedding

### Exemplo 11 — Regenerar schemas JSON após mudanças de tipo
```bash
cargo run --bin dump-schema -- --check
git diff --stat docs/schemas/
cargo run --bin dump-schema  # se --check falhou
```
- ESPERE: `--check` sai com 0 quando schemas estão sincronizados; regeneração produz output idempotente

### Exemplo 12 — Pipeline de manutenção (semanal)
```bash
sqlite-graphrag purge --retention-days 30 --yes --dry-run
sqlite-graphrag cleanup-orphans --yes --dry-run
sqlite-graphrag prune-relations --relation mentions --yes --dry-run
sqlite-graphrag vacuum --json
sqlite-graphrag optimize --json
sqlite-graphrag sync-safe-copy --dest ~/backups/graphrag-$(date +%Y%m%d).sqlite
```
- ESPERE: cada dry-run reporta contagens; pipeline completo reclama espaço e gera snapshot seguro

### Exemplo 13 — Inspecionar whitelist de modelos Codex (v1.0.89, alias no-op, GAP-E2E-010a)
```bash
sqlite-graphrag codex-models --json | jaq '{count, default, models: .models[:3]}'
sqlite-graphrag codex-models  # modo texto para humanos
sqlite-graphrag codex-models --json | jaq '.models | length'
```
- ESPERE: envelope JSON `{"action":"codex_models","count":N,"default":"gpt-5.5","models":[...]}`
- ESPERE: modo texto emite lista legível de modelos suportados
- USE ao validar que o escopo OAuth atual inclui os nomes de modelo codex necessários

### Exemplo 14 — Health check escopado para um namespace (v1.0.89, GAP-E2E-002)
```bash
sqlite-graphrag health --namespace prod --json | jaq '{integrity_ok, schema_version, counts}'
sqlite-graphrag health --namespace dev --json | jaq '.counts'  # contagens diferentes
sqlite-graphrag health --json | jaq '.counts'  # contagens globais
```
- ESPERE: contagens filtradas para o namespace especificado; campos integrity e schema_version inalterados
- USE em ambientes multi-tenant para verificar isolamento por namespace
- REGRA DE OMISSÃO: quando `--namespace` é omitido, contagens agregam entre todos namespaces (visão global)

### Exemplo 15 — Preview de migração em dry-run (v1.0.89, GAP-E2E-009)
```bash
sqlite-graphrag migrate --dry-run --json | jaq '.would_apply[]? | {name, version}'
sqlite-graphrag migrate --to-llm-only --drop-vec-tables --dry-run --json | jaq '.'
sqlite-graphrag migrate --dry-run --json  # sempre faça PREVIEW antes de migrações destrutivas
```
- ESPERE: lista de migrações pendentes com nome+versão sem aplicá-las; banco permanece inalterado
- ESPERE: `--to-llm-only --dry-run` reporta plano de drop de tabelas vec sem executar
- USE em pipelines CI e antes de qualquer passo de migração irreversível

### Exemplo 16 — Cadeia de fallback de backend (Codex primeiro, Claude reserva)
```bash
sqlite-graphrag --llm-backend codex,claude --llm-model gpt-5.5 \
  remember --name decisao-fallback --type decision \
  --description "Cadeia de backend" --body "Codex primeiro, Claude reserva" --json \
  | jaq '{backend_invoked, vec_degraded_reason}'

sqlite-graphrag --llm-backend codex,claude,none --skip-embedding-on-failure \
  remember --name decisao-null --type note --description x --body y --json \
  | jaq '.backend_invoked'
```
- ESPERE: `backend_invoked` reporta qual backend efetivamente gerou o embedding
- ESPERE: cadeia tripla cai em embedding null e retorna exit 0 com `--skip-embedding-on-failure`

### Exemplo 16b — Cadeia de fallback completa: codex → claude → opencode → none
```bash
sqlite-graphrag --llm-backend codex,claude,opencode,none --skip-embedding-on-failure \
  remember --name max-resiliencia --type note \
  --description "sobrevive a falha de todos os backends" --body-file nota.md

sqlite-graphrag read --name max-resiliencia --json | jaq '.backend_invoked'
```
- ESPERE: tenta codex, depois claude, depois opencode, depois degrada para embedding null; `backend_invoked` confirma qual rodou

### Exemplo 17 — Seleção de modelo por backend e env var persistente
```bash
# Modelo explicito por backend via flag
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini recall "query" --k 5 --json | jaq '.backend_invoked'
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 recall "query" --k 5 --json | jaq '.backend_invoked'

# Backend e modelo persistentes via env vars
export SQLITE_GRAPHRAG_LLM_BACKEND=codex
export SQLITE_GRAPHRAG_LLM_MODEL=gpt-5.4-mini
sqlite-graphrag recall "query" --k 5 --json | jaq '.backend_invoked'
```
- ESPERE: `--llm-model` seleciona o modelo de embedding para o backend ativo
- ESPERE: env vars fixam backend e modelo sem repetir flags em cada invocação


## Referências para Documentação Estendida

Para detalhes além do escopo de uso diário desta skill, os seguintes documentos do projeto estendem a cobertura:

- `docs/HOW_TO_USE.md` — quickstart, instalação, workflows comuns
- `docs/COOKBOOK.md` — 50+ receitas para padrões avançados (diagnóstico preflight, recovery de drift de schema, etc.)
- `docs/MIGRATION.md` — caminhos de upgrade entre versões
- `docs/CROSS_PLATFORM.md` — comportamento em Linux, macOS, Windows ARM64
- `docs/AGENTS.pt-BR.md` — documentação PT-BR estendida para agentes de IA
- `docs/schemas/*.schema.json` — contratos JSON Schema completos (versionados por SemVer)
- `docs/decisions/adr-*.md` — Architecture Decision Records (justificativas para cada escolha de design)
- `llms-full.txt` — dump completo de contexto LLM com todas as regras
- `gaps.md` — gaps abertos e fechados atualmente
- `CHANGELOG.md` — release notes versão por versão
- `Cargo.toml` — metadados do pacote e documentação de tamanho do binário (14.6 MiB)


## Resumo de Regras Ativas e Anti-padrões
- NUNCA passe `ANTHROPIC_API_KEY` ou `OPENAI_API_KEY` (OAuth-only, exit 1)
- NUNCA dependa do daemon ou use flag `--bare` (REMOVIDOS v1.0.76 e v1.0.79)
- NUNCA instale com `--features embedding-legacy` ou `--features ner-legacy` (REMOVIDOS)
- NUNCA use crates `fastembed`, `tokenizers`, `sqlite-vec`, ou `GLiNER`
- NUNCA espere KNN sqlite-vec; cosine é pure Rust em `src/similarity.rs`
- NUNCA rode `enrich` em paralelo contra mesmo banco (job singleton via `lock::acquire_job_singleton`)
- NUNCA escreva no arquivo `.sqlite` fora do binário
- NUNCA ignore exit 19 (envelope SHUTDOWN_EXIT_CODE); trabalho parcial descartado, RETRY OBRIGATÓRIO
- NUNCA ignore exit 16 (falha preflight); corrija config MCP ou `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1`
- NUNCA duplique conteúdo já existente em `CHANGELOG.md`
- NUNCA use `mentions` como relação padrão de grafo
- NUNCA passe corpo vazio via `--graph-stdin` (exit 1 desde v1.0.54)
- NUNCA use `--gliner-variant` (no-op desde v1.0.79)
- NUNCA chame `migrate --to-llm-only` sem guarda de segurança `--drop-vec-tables`
- NUNCA ignore flag `--wait-lock` quando contenção for esperada
- NUNCA assuma exit 1 igual a exit 9 (validação vs duplicada)
- NUNCA assuma que o tamanho do binário é 6 MB; o real é 14.6 MiB stripped ELF
- NUNCA passe `--llm-backend codex` esperando uso de API key (OAuth-only, exit 1)
- SEMPRE passe `--llm-backend` e `--llm-model` em comandos de embedding para backend determinístico
- SEMPRE parsee `backend_invoked` para confirmar qual backend gerou o embedding
