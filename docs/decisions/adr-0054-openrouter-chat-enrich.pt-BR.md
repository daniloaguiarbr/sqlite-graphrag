# ADR-0054 — Transporte de chat OpenRouter para o `enrich`

**Status**: Aceito
**Data**: 2026-06-27
**Contexto**: sqlite-graphrag v1.0.95 — GAP-OR-ENRICH

## Problema

O `enrich` executa um pipeline SCAN→JUDGE→PERSIST onde o JUDGE é um LLM
que retorna JSON estruturado. Até a v1.0.94 o JUDGE tinha só três
transportes — `claude-code`, `codex`, `opencode` — e cada um resolve em
um `Command::new` que spawna uma CLI instalada e autenticada localmente.
Não existia nenhum transporte REST para o JUDGE.

Enquanto isso, os embeddings já haviam migrado para um cliente REST
(`src/embedding_api.rs`, ADR-0052/0053): `remember`/`recall`/`ingest`
embedam contra o `/embeddings` do OpenRouter sem subprocesso local. Isso
deixou uma assimetria documentada em `gaps.md` — embedding tem caminho
REST, enrichment não tem. Concretamente:

1. **Dependência de CLI** — toda execução de `enrich` exigia uma das três
   CLIs instalada e autenticada via OAuth no host, bloqueando ambientes
   headless ou containers que têm chave de API mas não têm CLI.

2. **Sem escolha de modelo** — o modelo do JUDGE era o default da CLI
   spawnada; o usuário não podia escolher um modelo de texto específico.

3. **Saída frágil** — a resposta do JUDGE era parseada do stdout do
   subprocesso, e o cold-start por item somava latência.

O único cliente HTTP OpenRouter (`OPENROUTER_EMBEDDINGS_URL` em
`src/embedding_api.rs`) aponta exclusivamente para `/embeddings`; não
existia cliente para `/chat/completions`.

## Decisão

Adicionar um quarto transporte ao `enrich`, `--mode openrouter`, que
roteia o JUDGE para o endpoint REST `/chat/completions` do OpenRouter. A
lógica SCAN→JUDGE→PERSIST fica intacta; só o transporte do JUDGE muda.

### Novo módulo `src/chat_api.rs`

`OpenRouterChatClient` espelha `src/embedding_api.rs`: guarda um
`reqwest::Client`, uma chave de API `secrecy::SecretBox<String>` (zeroize
no drop, nunca logada) e o nome do modelo vinculado.
`complete(system_prompt, input_text, schema_str, max_tokens)` roda uma
completion de saída estruturada e retorna
`(serde_json::Value, cost_usd, is_oauth)`.

- `response_format` é `json_schema` com `strict: true`. Os schemas por
  operação reusados (`BINDINGS_SCHEMA`, `ENTITY_DESCRIPTION_SCHEMA`,
  etc.) são agnósticos ao runner e carregam o contrato.
- `provider.require_parameters: true` roteia apenas providers que honram
  o schema, então um provider que descarta silenciosamente o
  `response_format` é excluído em vez de devolver texto irrestrito.
- `reasoning.enabled: false` desabilita reasoning na extração para
  reduzir tokens pagos e latência. Como o suporte a reasoning-mandatory
  varia por modelo, um fallback gracioso envolve isso: `complete()` tenta
  `enabled: false` primeiro e, se o provider rejeitar (HTTP 400
  mencionando `reasoning`, detectado pelo helper `reasoning_disable_rejected`),
  faz UM retry omitindo o campo `reasoning` para o modelo usar seu default
  obrigatório. 9 dos 13 modelos testados aceitam `enabled: false`; 4 exigem
  o fallback. O parâmetro depreciado `usage: {include:true}` NÃO é enviado —
  o objeto `usage` (com `cost`) já volta automaticamente.
- Acontecem dois parses: o corpo HTTP em `ChatResponse`, depois a string
  `choices[0].message.content` no valor JSON final. Conteúdo vazio ou
  corpo não-JSON sob schema rígido é reportado como erro explícito
  "modelo incompatível com structured outputs", nomeando o modelo.
- `cost_usd` é lido de `usage.cost` (ou `0.0` quando ausente) e somado ao
  total da execução. `is_oauth` é sempre `false` porque o OpenRouter usa
  chave de API, não OAuth.
- O retry/backoff é idêntico ao cliente de embeddings: aborto imediato em
  401/400/404, `retry-after` em 429, backoff exponencial + jitter em 5xx
  e em 200-com-falha-de-parse. Os headers são mínimos — só
  `Authorization: Bearer`, sem `HTTP-Referer`/`X-Title`.

### Integração no `enrich` (`src/commands/enrich.rs`)

`EnrichMode` ganha a variante `OpenRouter` (Display `"openrouter"`).
Novos flags: `--openrouter-model` (OBRIGATÓRIO neste modo),
`--openrouter-api-key` (env `OPENROUTER_API_KEY`), `--openrouter-timeout`,
`--openrouter-base-url`. `validate_mode_flags` rejeita flags de modo
cruzado (flags claude/codex/opencode sob `--mode openrouter`). O preflight
probe valida apenas a chave de API neste modo — sem spawn de subprocesso.
Cada braço de dispatch do JUDGE ganha um ramo
`OpenRouter => call_openrouter(...)`; `call_openrouter` é um wrapper sync
que dirige `client.complete(...)` via `shared_runtime()?.block_on(...)` e
retorna a mesma tupla `(Value, f64, bool)` dos outros três runners.

### Infraestrutura reusada

`resolve_api_key("openrouter", cli)` (precedência env > config > CLI),
`shared_runtime()` (ponte sync→async) e o padrão de singleton `OnceLock`
do `OPENROUTER_CLIENT` são reusados verbatim. Um novo singleton
`OPENROUTER_CHAT_CLIENT` em `src/embedder.rs` espelha
`get_openrouter_embedder`. Nenhuma dependência nova — `reqwest`,
`secrecy`, `tokio`, `serde` já estão presentes.

## Alternativas Consideradas

### A. Adicionar um modo JUDGE HTTP genérico (qualquer endpoint OpenAI-compatible)

Rejeitada (YAGNI). O gap é especificamente a paridade do OpenRouter com o
caminho de embeddings. Um flag de endpoint genérico duplicaria a
superfície de auth/retry sem um segundo backend atual que o justifique; o
OpenRouter já serve todos os modelos que o usuário listou.

### B. Definir um default para `--openrouter-model` num modelo de texto conhecido

Rejeitada por restrição explícita do usuário. `--openrouter-model` é
OBRIGATÓRIO; a ausência retorna `AppError::Validation` (exit 1) antes de
qualquer chamada de rede. Um default escolheria silenciosamente um modelo
cujo suporte a structured outputs e custo o usuário não escolheu.

### C. Parsear `usage` via uma segunda requisição de usage-accounting

Rejeitada. `usage: {include:true}` está depreciado e o objeto `usage` com
`cost` já chega na resposta de chat; uma segunda chamada somaria latência
e custo por dados já em mãos.

### D. Habilitar reasoning e excluí-lo da saída

Rejeitada para extração. `reasoning.enabled: false` é mais barato e mais
rápido; `{exclude:true}` ainda cobraria tokens de reasoning. Modelos com
`reasoning.mandatory: true` rejeitam o disable; em vez de erro,
`complete()` faz um retry omitindo `reasoning` (ver Decisão) para o modelo
usar seu default obrigatório — transformando os 9/13 que aceitam
`enabled: false` em 13/13 compatíveis.

## Consequências

- Positiva: o `enrich` roda sem CLI local instalada ou autenticada — uma
  chave de API basta, desbloqueando uso headless e em container.
- Positiva: o usuário escolhe o modelo de texto exato do JUDGE via
  `--openrouter-model`, com uma matriz de compatibilidade de 13 modelos
  exercitada E2E.
- Positiva: o Structured Outputs `strict` produz JSON confiável sem
  parsing frágil de stdout, e `usage.cost` dá o custo real por item em uma
  única requisição.
- Negativa: os tokens são pagos contra a `OPENROUTER_API_KEY` do usuário,
  ao contrário dos modos de CLI local sem OAuth — o trade-off é
  conveniência e alcance headless versus cobrança por token.
- Negativa: o suporte a `json_schema` varia por provider; um modelo sem
  structured outputs falha com erro explícito do OpenRouter.
  `reasoning.mandatory` NÃO é falha — o fallback da Decisão o absorve. O
  teste real dos 13 modelos (13/13 passam: 9 com `enabled: false`, 4 via o
  fallback) é a única prova confiável de quais modelos são seguros para
  produção.
- Os três modos existentes ficam inalterados; `--mode` continua
  obrigatório (ADR-0053), agora com `openrouter` como quarto valor
  válido.

## Validação

- Build: `cargo build --release` 0 erros; `cargo clippy --all-targets
  --all-features -- -D warnings` 0 warnings; `cargo fmt --all --check`
  0 diferenças; `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`
  0 warnings.
- Unit: testes com `wiremock::MockServer` para montagem do request
  (`response_format`, `provider.require_parameters`, `reasoning`), parse
  da resposta + segundo parse JSON, leitura de `usage.cost`, retry (429
  `retry-after`, 5xx backoff, 401 permanente), 400/404 sem retry,
  conteúdo vazio como incompatível; rejeição de flags cruzadas em
  `validate_mode_flags`; `--openrouter-model` obrigatório (exit 1).
- API real: `tests/openrouter_chat_real.rs` (`#[ignore]`) itera os 13
  modelos de texto listados contra o `BINDINGS_SCHEMA` rígido. Matriz:
  13/13 passam — 9 aceitam `reasoning.enabled: false`, 4
  (`minimax/minimax-m2.7[:nitro]`, `openai/gpt-oss-120b[:nitro]`) exigem o
  fallback reasoning-mandatory.

## Cross-references

- `gaps.md` — GAP-OR-ENRICH marcado como RESOLVIDO em v1.0.95
- ADR-0053 (remediação de quatro gaps da v1.0.94) — tornou `enrich
  --mode` obrigatório
- ADR-0052 (backend de embedding OpenRouter) — o cliente REST de
  embeddings que este módulo espelha
- `src/chat_api.rs` (`OpenRouterChatClient`), `src/commands/enrich.rs`
  (`EnrichMode::OpenRouter`, `call_openrouter`, validação de flags),
  `src/embedder.rs` (singleton `OPENROUTER_CHAT_CLIENT`,
  `resolve_api_key`, `shared_runtime`), `src/embedding_api.rs`
  (retry/backoff espelhado)
