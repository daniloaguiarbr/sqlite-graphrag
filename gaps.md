# Gaps — sqlite-graphrag CLI


## GAP-OPENCODE-001 — FECHADO (v1.0.90) — Backend OpenCode Headless na Pipeline de Embedding e Extração

## Problema
- O sistema suporta APENAS `codex` headless e `claude` headless como backends LLM
- O `opencode` headless (v1.17.7, instalado no PATH) NÃO é utilizado em NENHUMA função de embedding ou extração
- A camada de spawn (`src/spawn/opencode_adapter.rs`) já implementa o trait `VersionAdapter` para o opencode
- A camada de embedding (`src/extract/llm_embedding.rs`) ignora completamente o opencode
- A camada de extração (`src/extract/llm_backend.rs`) ignora completamente o opencode
- A camada de fallback chain (`src/embedder.rs`) ignora completamente o opencode
- O enum `EmbeddingFlavour` em `llm_embedding.rs:101` possui APENAS duas variantes: `Claude` e `Codex`
- O enum `LlmBackendKindFactory` em `llm_backend.rs:200` possui APENAS quatro variantes: `Auto`, `Codex`, `Claude` e `None`
- O enum `LlmBackendKind` em `embedder.rs:791` possui APENAS três variantes: `Codex`, `Claude` e `None`
- O `detect_available_backend()` em `llm_backend.rs:386` PROBES APENAS `codex` e `claude` no PATH
- O `LlmEmbedding::detect_available()` em `llm_embedding.rs:294` PROBES APENAS `codex` e `claude` no PATH
- O modelo do opencode NÃO é selecionável via CLI flag `--llm-model` ou env var
- Existe hardcode de CLI binária (apenas `codex` e `claude`) em 6+ módulos
- Existe hardcode de modelo default (apenas `gpt-5.5` e `claude-sonnet-4-6`) sem extensibilidade


## Consequências do Problema
- Usuários com `opencode` instalado NÃO podem usá-lo como backend de embedding
- Usuários com `opencode` instalado NÃO podem usá-lo como backend de extração de entidades/relações
- A fallback chain `codex → claude → none` NUNCA considera o opencode como candidato
- Modelos disponíveis via opencode (ex.: `opencode/big-pickle`, `opencode/deepseek-v4-flash-free`, `opencode/mimo-v2.5-free`, `opencode/nemotron-3-ultra-free`, `opencode/north-mini-code-free`) são INACESSÍVEIS
- O auto-detect (`--llm-backend auto`) IGNORA o opencode mesmo quando está no PATH
- Código morto: o `opencode_adapter.rs` existe e funciona mas NUNCA é invocado pela pipeline de produção
- A arquitetura do factory pattern (`LlmBackendFactory` trait) foi PROJETADA para extensibilidade (comentário em `llm_backend.rs:223`: "New backends (ollama, opencode, lm-studio) can be added") mas o opencode NUNCA foi conectado
- Violação do princípio de design do próprio sistema: a trait `VersionAdapter` documenta "opencode" como executor suportado mas a integração é incompleta


## Causa Raiz do Problema
- O `opencode_adapter.rs` foi adicionado na v1.0.75 (G22) como parte da abstração `VersionAdapter` mas o trabalho parou na camada de spawn
- As 3 camadas superiores (embedding, extração, fallback chain) NUNCA foram atualizadas para reconhecer o opencode
- Os 3 enums centrais (`EmbeddingFlavour`, `LlmBackendKindFactory`, `LlmBackendKind`) foram escritos com variantes hardcoded em vez de extensíveis
- A função `detect_available_backend()` usa probes hardcoded `has_in_path("codex")` e `has_in_path("claude")` sem probe para `has_in_path("opencode")`
- A função `LlmEmbedding::detect_available()` usa `which::which("codex")` e `which::which("claude")` sem `which::which("opencode")`
- O builder `LlmEmbeddingBuilder` possui apenas `codex_default()` e `claude_default()` sem `opencode_default()`
- As env vars de configuração (`SQLITE_GRAPHRAG_CODEX_BINARY`, `SQLITE_GRAPHRAG_CLAUDE_BINARY`, etc.) NÃO têm equivalentes para opencode
- A resolução de modelo (`codex_embed_model()`, `claude_embed_model()`) NÃO tem equivalente `opencode_embed_model()`
- O `build_codex_embedding_command()` e `invoke_claude()` são funções separadas por backend sem abstração genérica para um terceiro backend
- A CLI do opencode usa `opencode run --format json -m <provider/model> <mensagem>` que é DIFERENTE de `codex exec --json` e `claude -p --output-format json`, exigindo um `invoke_opencode()` dedicado


## Solução Proposta
- Adicionar variante `Opencode` ao enum `EmbeddingFlavour` em `llm_embedding.rs`
- Adicionar variante `Opencode` ao enum `LlmBackendKindFactory` em `llm_backend.rs`
- Adicionar variante `Opencode` ao enum `LlmBackendKind` em `embedder.rs`
- Criar `OpencodeFactory` que implementa `LlmBackendFactory` em `llm_backend.rs`
- Atualizar `detect_available_backend()` para incluir probe `has_in_path("opencode")` com precedência configurável
- Atualizar `LlmEmbedding::detect_available()` para incluir `which::which("opencode")` como terceiro candidato
- Criar `LlmEmbeddingBuilder::opencode_default()` simétrico aos builders existentes
- Criar `invoke_opencode()` em `llm_embedding.rs` que constrói o comando `opencode run --format json -m <modelo> --dangerously-skip-permissions`
- Criar `build_opencode_embedding_command()` simétrica a `build_codex_embedding_command()`
- Criar `opencode_embed_model()` com precedência: `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL` > `SQLITE_GRAPHRAG_LLM_MODEL` > default
- Adicionar env vars: `SQLITE_GRAPHRAG_OPENCODE_BINARY`, `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL`
- Atualizar o match em `factory_for_choice()` para incluir `LlmBackendKindFactory::Opencode`
- Atualizar a fallback chain padrão em `embedder.rs:737` para `[Codex, Claude, Opencode, None]`
- Atualizar `EmbeddingFlavour::as_str()` para incluir `"opencode"`
- Atualizar `LlmBackendKind::as_str()` para incluir `"opencode"`
- Aceitar `--llm-backend opencode` na CLI (parsing do clap)
- Permitir seleção de modelo via `--llm-model provider/modelo` (ex.: `--llm-model opencode/big-pickle`)
- NÃO fazer hardcode de nenhuma CLI binária nem modelo default no opencode
- PROIBIR hardcode: toda referência a binário e modelo DEVE ser resolvida via env var ou flag CLI


## Benefícios da Solução
- Usuários com opencode headless podem usá-lo como backend de embedding e extração
- Modelos gratuitos do opencode (`deepseek-v4-flash-free`, `mimo-v2.5-free`, etc.) ficam acessíveis para embedding
- A fallback chain ganha resiliência: `codex → claude → opencode → none`
- O auto-detect passa a considerar 3 backends ao invés de 2
- O `opencode_adapter.rs` existente deixa de ser código morto
- A arquitetura respeita o princípio de extensibilidade documentado no factory pattern
- Operadores com múltiplos backends instalados podem escolher livremente via `--llm-backend opencode --llm-model opencode/big-pickle`
- Zero hardcode de CLI e zero hardcode de modelo: toda resolução via env var ou flag


## Como Solucionar — Etapas Incrementais
- Etapa 1 — Enums: adicionar variante `Opencode` nos 3 enums (`EmbeddingFlavour`, `LlmBackendKindFactory`, `LlmBackendKind`) e atualizar os `as_str()` e `match` correspondentes
- Etapa 2 — Env vars e resolução de modelo: criar `opencode_embed_model()` e env vars `SQLITE_GRAPHRAG_OPENCODE_BINARY`, `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL`
- Etapa 3 — Builder: criar `LlmEmbeddingBuilder::opencode_default()` e `build_opencode_embedding_command()` que constrói `opencode run --format json -m <modelo> --dangerously-skip-permissions`
- Etapa 4 — Invoke: criar `invoke_opencode()` em `llm_embedding.rs` com parsing de output JSON do `opencode run --format json`
- Etapa 5 — Factory: criar `OpencodeFactory` implementando `LlmBackendFactory` e atualizar `factory_for_choice()`
- Etapa 6 — Auto-detect: atualizar `detect_available_backend()` e `LlmEmbedding::detect_available()` para incluir probe do opencode no PATH
- Etapa 7 — Fallback chain: atualizar a chain padrão em `embedder.rs` para `[Codex, Claude, Opencode, None]`
- Etapa 8 — CLI: aceitar `--llm-backend opencode` no parsing do clap e mapear para `LlmBackendKindFactory::Opencode`
- Etapa 9 — Testes: criar testes unitários para `opencode_embed_model()`, `OpencodeFactory`, `invoke_opencode()`, `build_opencode_embedding_command()`, e teste de integração com mock-opencode script
- Etapa 10 — Documentação: atualizar CLAUDE.md, skills, ADRs e schemas para refletir o terceiro backend


## Relações Causa x Efeito
- CAUSA: adapter existe APENAS na camada spawn → EFEITO: pipeline de produção ignora opencode
- CAUSA: enums hardcoded com 2-3 variantes → EFEITO: impossível selecionar opencode via CLI
- CAUSA: `detect_available_backend()` probes apenas codex/claude → EFEITO: auto-detect ignora opencode no PATH
- CAUSA: ausência de `invoke_opencode()` → EFEITO: nenhum subprocess opencode é spawnado para embedding
- CAUSA: ausência de `OpencodeFactory` → EFEITO: factory pattern projetado para extensibilidade permanece incompleto
- CAUSA: ausência de env vars `SQLITE_GRAPHRAG_OPENCODE_*` → EFEITO: operador não pode configurar binário/modelo do opencode
- CAUSA: fallback chain hardcoded `[Codex, Claude, None]` → EFEITO: sistema degrada para none sem tentar opencode
- CAUSA: `opencode run --format json` usa sintaxe diferente de codex/claude → EFEITO: requer implementação de `invoke_opencode()` dedicada (não reutilizável dos existentes)


## Referências no Código-Fonte
- `src/spawn/opencode_adapter.rs` — adapter existente (v1.0.75 G22), FUNCIONAL mas NÃO integrado
- `src/spawn/mod.rs:13` — `pub mod opencode_adapter` (declarado, importável)
- `src/spawn/compat_matrix.rs:55` — `opencode_capabilities()` (implementada)
- `src/extract/llm_backend.rs:17` — docstring menciona opencode como opção válida
- `src/extract/llm_backend.rs:200-210` — `LlmBackendKindFactory` enum (SEM variante Opencode)
- `src/extract/llm_backend.rs:221-223` — comentário que opencode será adicionado em v1.0.83+
- `src/extract/llm_backend.rs:386-411` — `detect_available_backend()` (SEM probe opencode)
- `src/extract/llm_backend.rs:417-426` — `factory_for_choice()` (SEM match Opencode)
- `src/extract/llm_embedding.rs:100-104` — `EmbeddingFlavour` enum (SEM variante Opencode)
- `src/extract/llm_embedding.rs:294-328` — `detect_available()` (SEM probe opencode)
- `src/embedder.rs:735-739` — fallback chain hardcoded `[Codex, Claude, None]`
- `src/embedder.rs:791` — `LlmBackendKind` enum (SEM variante Opencode)


## Informações do OpenCode CLI (v1.17.7)
- Binário: `~/.opencode/bin/opencode`
- Versão instalada: 1.17.7
- Modo headless: `opencode run --format json -m <provider/modelo> <mensagem>`
- Seleção de modelo: `-m provider/modelo` (ex.: `-m opencode/big-pickle`)
- Modelos disponíveis: `opencode/big-pickle`, `opencode/deepseek-v4-flash-free`, `opencode/mimo-v2.5-free`, `opencode/nemotron-3-ultra-free`, `opencode/north-mini-code-free`
- Output JSON: `--format json` (eventos JSON por linha, similar ao codex JSONL)
- Auto-approve: `--dangerously-skip-permissions`
- Servidor headless: `opencode serve`


## GAP-OPENCODE-002 — FECHADO (v1.0.90) — Backend OpenCode Headless na Pipeline de Ingestão, Enriquecimento e Fallback Chain

## Problema
- O `ingest --mode` aceita APENAS `none`, `gliner`, `claude-code` e `codex` como modos de extração curada por LLM
- O `enrich --mode` aceita APENAS `claude-code` e `codex` como provedores LLM
- O enum `LlmBackendChoice` no `src/cli.rs:28` aceita APENAS `Auto`, `Claude`, `Codex` e `None`
- A função `parse_fallback_chain()` em `src/cli.rs:60` reconhece APENAS tokens `codex`, `claude`, `claude-code` e `none`
- O `--llm-fallback` default é `codex,claude,none` sem possibilidade de incluir `opencode`
- O `dry_run_backend.rs` NÃO considera opencode na resolução de backend
- NÃO existe `ingest_opencode.rs` simétrico a `ingest_claude.rs` e `ingest_codex.rs`
- NÃO existe `opencode_runner.rs` simétrico a `claude_runner.rs` e `codex_spawn.rs`
- O `opencode_adapter.rs` existe na camada spawn mas NUNCA é conectado às pipelines de ingestão e enriquecimento
- O modelo do opencode NÃO é selecionável via `--opencode-model` nem via env var `SQLITE_GRAPHRAG_OPENCODE_MODEL`
- O binário do opencode NÃO é selecionável via `--opencode-binary` nem via env var `SQLITE_GRAPHRAG_OPENCODE_BINARY`
- O timeout do opencode NÃO é configurável via `--opencode-timeout` nem via env var


## Consequências do Problema
- NENHUM arquivo pode ser ingerido com extração curada pelo opencode headless
- NENHUMA memória pode ser enriquecida via opencode headless
- A fallback chain `codex → claude → none` NUNCA tenta opencode antes de degradar para `none`
- Modelos gratuitos do opencode (`deepseek-v4-flash-free`, `mimo-v2.5-free`, `nemotron-3-ultra-free`) são INACESSÍVEIS para extração de entidades e relações
- Operadores que possuem APENAS opencode instalado (sem codex e sem claude) recebem erro "no LLM CLI found" mesmo com o opencode funcional no PATH
- O dry-run de backend (`--dry-run-backend`) JAMAIS reporta opencode como backend disponível
- A validação cruzada de flags (`mode-conditional flag validation` em `src/commands/ingest.rs:852`) NÃO valida flags `--opencode-*` porque elas NÃO existem
- O job singleton de `enrich` e `ingest --mode` NÃO governa invocações opencode porque o modo NÃO existe
- O mecanismo de `--fallback-mode` no `enrich` aceita APENAS `ClaudeCode` e `Codex`, impedindo fallback para opencode em rate-limit
- A feature de `--preflight-check` no `enrich` NÃO pode validar disponibilidade do opencode antes de escanear candidatos
- Seis módulos de comando (`ingest.rs`, `ingest_claude.rs`, `ingest_codex.rs`, `enrich.rs`, `claude_runner.rs`, `codex_spawn.rs`) contêm lógica duplicada entre codex e claude que DEVERIA ser triplicada para opencode mas NÃO é
- Violação da regra `rules-rust-proibicao-hardcode` Seção 1 (separação código/configuração): o binário e o modelo do backend são resolvidos por strings hardcoded sem extensibilidade


## Causa Raiz do Problema
- O enum `IngestMode` em `src/commands/ingest.rs:301` foi escrito com variantes hardcoded (`None`, `Gliner`, `ClaudeCode`, `Codex`) sem prever um terceiro backend LLM
- O enum `EnrichMode` em `src/commands/enrich.rs:331` foi escrito com APENAS duas variantes (`ClaudeCode`, `Codex`) sem extensibilidade
- O enum `LlmBackendChoice` em `src/cli.rs:28` foi escrito com APENAS quatro variantes (`Auto`, `Claude`, `Codex`, `None`) sem variante `Opencode`
- A função `parse_fallback_chain()` em `src/cli.rs:60` usa match hardcoded com APENAS 3 tokens reconhecidos (`codex`, `claude`/`claude-code`, `none`)
- O `LlmBackendChoice::to_chain()` em `src/cli.rs:45` produz chains que JAMAIS incluem opencode
- A resolução de backend no `dry_run_backend.rs` probes APENAS `codex` e `claude` via `which::which`
- NÃO existe um módulo `ingest_opencode.rs` com `run_opencode_ingest()` simétrico a `run_claude_ingest()` e `run_codex_ingest()`
- NÃO existe um módulo `opencode_runner.rs` com `build_opencode_command()`, `parse_opencode_output()` e `validate_opencode_version()`
- A lógica de spawn, parsing de output e validação de modelo é ESPECÍFICA por backend (cada um tem formato JSON diferente) e o opencode usa `opencode run --format json -m <provider/model>` que é DIFERENTE de `codex exec --json` e `claude -p --output-format json`
- O flag `--dangerously-skip-permissions` do opencode é DIFERENTE dos 7 flags de endurecimento do claude e dos 9 flags do codex
- As env vars de configuração seguem padrão `SQLITE_GRAPHRAG_{BACKEND}_*` mas as do opencode (`SQLITE_GRAPHRAG_OPENCODE_BINARY`, `SQLITE_GRAPHRAG_OPENCODE_MODEL`, `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT`) NÃO foram criadas
- O adapter `opencode_adapter.rs` foi adicionado na v1.0.75 APENAS na camada de spawn (trait `VersionAdapter`) sem integração com as camadas superiores de ingestão e enriquecimento


## Solução Proposta
- Adicionar variante `Opencode` ao enum `IngestMode` em `src/commands/ingest.rs`
- Adicionar variante `Opencode` ao enum `EnrichMode` em `src/commands/enrich.rs`
- Adicionar variante `Opencode` ao enum `LlmBackendChoice` em `src/cli.rs`
- Atualizar `parse_fallback_chain()` para reconhecer token `"opencode"` como `LlmBackendKind::Opencode`
- Atualizar `LlmBackendChoice::to_chain()` para incluir opencode nas chains quando selecionado
- Criar `src/commands/ingest_opencode.rs` com `run_opencode_ingest()` que spawna `opencode run --format json -m <modelo> --dangerously-skip-permissions` por arquivo
- Criar `src/commands/opencode_runner.rs` com `build_opencode_command()`, `parse_opencode_output()` e `validate_opencode_version()`
- Adicionar flags CLI: `--opencode-binary`, `--opencode-model`, `--opencode-timeout`
- Adicionar env vars: `SQLITE_GRAPHRAG_OPENCODE_BINARY`, `SQLITE_GRAPHRAG_OPENCODE_MODEL`, `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT`
- Atualizar `dry_run_backend.rs` para considerar opencode na resolução de backend e no probe de PATH
- Atualizar a validação cruzada de flags em `ingest.rs:852` para rejeitar flags `--claude-*` e `--codex-*` quando `--mode opencode` e vice-versa
- Atualizar `--fallback-mode` no `enrich` para aceitar `Opencode` como modo de fallback
- Atualizar `--preflight-check` no `enrich` para validar disponibilidade do opencode antes de escanear candidatos
- O modelo do opencode DEVE ser selecionável via `--opencode-model provider/modelo` (ex.: `--opencode-model opencode/big-pickle`)
- PROIBIDO hardcode de CLI binária: toda referência ao binário DEVE ser resolvida via `SQLITE_GRAPHRAG_OPENCODE_BINARY` ou `--opencode-binary` ou `which::which("opencode")`
- PROIBIDO hardcode de modelo default: o modelo DEVE ser resolvido via `SQLITE_GRAPHRAG_OPENCODE_MODEL` ou `--opencode-model` ou `--llm-model`
- O parsing de output do opencode (`opencode run --format json`) emite eventos JSON por linha (NDJSON) similar ao codex; `parse_opencode_output()` DEVE iterar linhas e extrair o resultado final


## Benefícios da Solução
- Operadores com opencode instalado podem usá-lo para ingestão curada (`--mode opencode`)
- Operadores com opencode instalado podem usá-lo para enriquecimento (`--mode opencode`)
- A fallback chain ganha resiliência: `codex → claude → opencode → none`
- Modelos gratuitos do opencode ficam acessíveis para extração de entidades/relações (custo zero)
- O dry-run de backend passa a reportar opencode como backend disponível quando instalado
- O sistema respeita o princípio de extensibilidade documentado no factory pattern (`src/extract/llm_backend.rs:221`)
- Operadores com APENAS opencode instalado (sem codex e sem claude) podem operar o sistema completo
- Zero hardcode de CLI e zero hardcode de modelo: toda resolução via env var ou flag CLI
- A validação cruzada de flags previne erros silenciosos ao misturar flags de backends diferentes
- O mecanismo de fallback no enrich pode degradar para opencode antes de abortar em rate-limit


## Como Solucionar — Etapas Incrementais
- Etapa 1 — Enums de modo: adicionar variante `Opencode` em `IngestMode`, `EnrichMode` e `LlmBackendChoice`, atualizar todos os `match` e `Display` correspondentes
- Etapa 2 — Fallback chain: atualizar `parse_fallback_chain()` para reconhecer `"opencode"` e atualizar `LlmBackendChoice::to_chain()` para produzir chains com opencode
- Etapa 3 — Flags e env vars: adicionar `--opencode-binary`, `--opencode-model`, `--opencode-timeout` na struct `IngestArgs` e `EnrichArgs`; criar env vars `SQLITE_GRAPHRAG_OPENCODE_*`
- Etapa 4 — Runner: criar `src/commands/opencode_runner.rs` com `build_opencode_command()` (constrói `opencode run --format json -m <modelo> --dangerously-skip-permissions`), `parse_opencode_output()` (itera NDJSON e extrai resultado final) e `validate_opencode_version()` (verifica `opencode --version >= 1.17.0`)
- Etapa 5 — Ingestão: criar `src/commands/ingest_opencode.rs` com `run_opencode_ingest()` simétrico a `run_claude_ingest()` e `run_codex_ingest()`; incluir queue DB, resume, retry-failed, rate-limit backoff e dry-run
- Etapa 6 — Enriquecimento: atualizar `src/commands/enrich.rs` para despachar `EnrichMode::Opencode` usando `opencode_runner.rs`, incluir preflight-check e fallback-mode
- Etapa 7 — Dry-run: atualizar `dry_run_backend.rs` para considerar opencode na resolução e no probe de PATH
- Etapa 8 — Validação cruzada: atualizar a validação de flags em `ingest.rs` para rejeitar flags `--claude-*` e `--codex-*` quando `--mode opencode` e rejeitar `--opencode-*` quando `--mode claude-code` ou `--mode codex`
- Etapa 9 — Testes: criar testes unitários para `opencode_runner.rs` (build_command, parse_output, validate_version), `ingest_opencode.rs` (run_opencode_ingest com mock), e teste de integração com mock-opencode script
- Etapa 10 — Documentação: atualizar CLAUDE.md, skills EN/PT, schemas NDJSON, ADRs e help text do clap para refletir o terceiro backend de ingestão/enriquecimento


## Relações Causa x Efeito
- CAUSA: `IngestMode` enum com 4 variantes hardcoded → EFEITO: `--mode opencode` é rejeitado pelo clap com "invalid value"
- CAUSA: `EnrichMode` enum com 2 variantes hardcoded → EFEITO: `--mode opencode` é rejeitado pelo clap no enrich
- CAUSA: `LlmBackendChoice` sem variante Opencode → EFEITO: `--llm-backend opencode` é rejeitado pelo clap
- CAUSA: `parse_fallback_chain()` não reconhece token "opencode" → EFEITO: `--llm-fallback codex,opencode,none` emite warning e ignora opencode
- CAUSA: `LlmBackendChoice::to_chain()` sem opencode → EFEITO: nenhuma chain inclui opencode como candidato
- CAUSA: ausência de `ingest_opencode.rs` → EFEITO: nenhum subprocess opencode é spawnado para extração curada de entidades/relações
- CAUSA: ausência de `opencode_runner.rs` → EFEITO: nenhum comando opencode é construído, nenhum output é parseado, nenhuma versão é validada
- CAUSA: ausência de flags `--opencode-*` → EFEITO: operador não pode configurar binário, modelo ou timeout do opencode
- CAUSA: ausência de env vars `SQLITE_GRAPHRAG_OPENCODE_*` → EFEITO: operador não pode configurar opencode via ambiente (12-Factor App violado)
- CAUSA: `dry_run_backend.rs` probes apenas codex/claude → EFEITO: dry-run NUNCA reporta opencode como disponível
- CAUSA: validação cruzada de flags ignora opencode → EFEITO: mistura silenciosa de flags de backends diferentes
- CAUSA: `--fallback-mode` no enrich aceita apenas ClaudeCode/Codex → EFEITO: rate-limit não pode degradar para opencode
- CAUSA: output format diferente (`opencode run --format json` vs `codex exec --json` vs `claude -p --output-format json`) → EFEITO: requer implementação dedicada de parser, não reutilizável dos existentes


## Referências no Código-Fonte
- `src/commands/ingest.rs:301` — `IngestMode` enum (SEM variante Opencode)
- `src/commands/ingest.rs:221` — `--mode` aceita apenas `none`, `gliner`, `claude-code`, `codex`
- `src/commands/ingest.rs:852` — validação cruzada de flags (SEM validação para opencode)
- `src/commands/ingest_claude.rs` — runner claude para ingestão (SEM equivalente opencode)
- `src/commands/ingest_codex.rs` — runner codex para ingestão (SEM equivalente opencode)
- `src/commands/enrich.rs:331` — `EnrichMode` enum (SEM variante Opencode)
- `src/commands/enrich.rs:376` — `--mode` aceita apenas `claude-code`, `codex`
- `src/commands/enrich.rs:495` — `--fallback-mode` aceita apenas `ClaudeCode`, `Codex`
- `src/commands/claude_runner.rs` — runner claude headless (SEM equivalente opencode)
- `src/commands/codex_spawn.rs` — runner codex headless (SEM equivalente opencode)
- `src/commands/dry_run_backend.rs:75-113` — resolução de backend (SEM probe opencode)
- `src/cli.rs:28` — `LlmBackendChoice` enum (SEM variante Opencode)
- `src/cli.rs:60` — `parse_fallback_chain()` (SEM token "opencode")
- `src/cli.rs:188` — `--llm-backend` (SEM valor `opencode`)
- `src/cli.rs:228` — `--llm-fallback` default `codex,claude,none` (SEM opencode)
- `src/spawn/opencode_adapter.rs` — adapter existente na camada spawn (FUNCIONAL mas NÃO integrado com ingestão/enriquecimento)


## Diferenças de Interface entre os 3 Backends
- codex: `codex exec --json --output-schema <path> --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules -c mcp_servers='{}' --ask-for-approval never "<prompt>"`
- claude: `claude -p --output-format json --strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions "<prompt>"`
- opencode: `opencode run --format json -m <provider/modelo> --dangerously-skip-permissions "<prompt>"`
- codex retorna JSONL (`{type: "item", item: {type: "agent_message", content: [{type: "output_text", text: "..."}]}}`)
- claude retorna JSON array (`[{type: "result", result: "..."}]`)
- opencode retorna NDJSON (eventos JSON por linha, formato a ser validado com output real)
- codex usa `--output-schema` para structured output (tempfile com JSON Schema)
- claude usa `--output-format json` para structured output
- opencode NÃO possui flag de structured output equivalente (requer extração via parsing do texto de resposta)


## Dependência com GAP-OPENCODE-001
- GAP-OPENCODE-001 cobre a pipeline de EMBEDDING (enums `EmbeddingFlavour`, `LlmBackendKindFactory`, `LlmBackendKind`; funções `detect_available_backend()`, `detect_available()`, `embed_with_fallback()`)
- GAP-OPENCODE-002 cobre as pipelines de INGESTÃO e ENRIQUECIMENTO (enums `IngestMode`, `EnrichMode`, `LlmBackendChoice`; módulos `ingest_opencode.rs`, `opencode_runner.rs`, `dry_run_backend.rs`)
- Ambos os GAPs compartilham: o adapter `opencode_adapter.rs` (camada spawn), as env vars `SQLITE_GRAPHRAG_OPENCODE_*` e a regra "PROIBIDO hardcode de CLI e modelo"
- GAP-OPENCODE-001 DEVE ser resolvido ANTES do GAP-OPENCODE-002 porque a ingestão e o enriquecimento dependem da pipeline de embedding para persistir memórias pesquisáveis
- A Etapa 8 do GAP-OPENCODE-001 (CLI: aceitar `--llm-backend opencode`) e a Etapa 1 do GAP-OPENCODE-002 (enums de modo) podem ser feitas na MESMA PR para consistência


## BUG-AUDIT-001 — FECHADO (v1.0.90 auditoria) — Cross-contamination de modelo opencode
- CAUSA: `opencode_embed_model()` e `resolve_opencode_model()` faziam fallback para `SQLITE_GRAPHRAG_LLM_MODEL`
- EFEITO: quando `LLM_MODEL=gpt-5.4-mini` (modelo codex), o opencode falhava com `ProviderModelNotFoundError`
- CORREÇÃO: removido fallback para `LLM_MODEL` em ambas as funções; precedência agora é `OPENCODE_EMBED_MODEL > OPENCODE_MODEL > default opencode/big-pickle`
- ARQUIVOS: `src/extract/llm_embedding.rs`, `src/commands/opencode_runner.rs`
- TESTE: `opencode_embed_model_ignores_llm_model` (novo, 875 total)


## BUG-AUDIT-002 — FECHADO (v1.0.90 auditoria) — Prompt de embedding genérico causava recusa do modelo
- CAUSA: prompt "Generate a 64-dimensional semantic embedding vector..." era interpretado como pedido de API real
- EFEITO: modelo opencode recusava gerar o vetor numérico, explicando que não tinha acesso a API de embedding
- CORREÇÃO: prompt reescrito com role-setting "You are an embedding function" que produz vetores reais de 64 dimensões
- ARQUIVOS: `src/extract/llm_embedding.rs` (invoke_single_async e embed_batch_async)
- VALIDAÇÃO: teste e2e confirmou `backend_invoked: "opencode"` com recall score 0.357


## BUG-AUDIT-003 — FECHADO (v1.0.90 auditoria) — env_clear() removia variáveis do provider
- CAUSA: `invoke_opencode()` e `build_opencode_command()` faziam `env_clear()` preservando apenas PATH e HOME
- EFEITO: credenciais de provider (OPENROUTER_API_KEY, etc.) e config (XDG_CONFIG_HOME) eram perdidas
- CORREÇÃO: criada `propagate_opencode_env()` que preserva OPENCODE_*, OPENROUTER_*, XDG_*, LANG, TERM, USER, LOGNAME, TMPDIR
- ARQUIVOS: `src/commands/opencode_runner.rs`, `src/extract/llm_embedding.rs`


## BUG-AUDIT-004 — FECHADO (v1.0.90 auditoria) — ingest_opencode retornava Err(Validation) em vez de executar
- CAUSA: `run_opencode_ingest()` era um stub que retornava `Err(AppError::Validation("under development"))`
- EFEITO: `--mode opencode` no ingest falhava sempre com mensagem de "under development"
- CORREÇÃO: implementado loop completo de extração por arquivo com persist de entidades/relações no SQLite
- ARQUIVOS: `src/commands/ingest_opencode.rs` (reescrito de 171 para ~310 linhas)
- VALIDAÇÃO: teste e2e com 2 arquivos markdown extraiu 10 entidades e 8 relações via opencode/big-pickle


## BUG-AUDIT-005 — FECHADO (v1.0.90 auditoria) — Schema do DB incorreto no persist_memory_with_graph
- CAUSA: INSERT usava `entity_type` (campo da struct Rust) ao invés de `type` (coluna SQLite); faltava `body_hash` NOT NULL
- EFEITO: `NOT NULL constraint failed: memories.body_hash` e `table entities has no column named entity_type`
- CORREÇÃO: INSERT corrigido para `type` e `body_hash` (BLAKE3); removidos `created_at` de entities (tem DEFAULT); removido `created_at` de relationships
- ARQUIVOS: `src/commands/ingest_opencode.rs`


## GAP-ENRICH-OPENCODE-001 — FECHADO (v1.0.90 auditoria-2) — enrich `--mode opencode` delega silenciosamente para codex headless
- CAUSA: 13 match arms `EnrichMode::Codex | EnrichMode::Opencode => call_codex(...)` usavam codex headless em vez de opencode headless
- EFEITO: operador que seleciona `--mode opencode` no enrich usa codex silenciosamente sem saber
- DETALHES: `find_codex_binary()` chamado na linha 1610 em vez de `find_opencode_binary()`; `validate_codex_model()` chamado em vez de validar modelo opencode; `build_codex_command()` chamado em vez de `build_opencode_command()`; preflight probe (linha 793) usa `find_codex_binary` para opencode
- CORREÇÃO: criada `call_opencode()` sync no `enrich.rs` que usa `opencode_runner`; separados os 13 match arms para `EnrichMode::Opencode => call_opencode(...)` dedicado; preflight probe e binary resolution agora usam `find_opencode_binary()` e `build_opencode_command()`
- ARQUIVOS: `src/commands/enrich.rs`, `src/commands/opencode_runner.rs`


## BUG-AUDIT-006 — FECHADO (v1.0.90 auditoria-3) — --opencode-binary flag CLI dead flag (declarada no clap mas ignorada)
- CAUSA: `find_opencode_binary()` aceita ZERO parâmetros; `find_codex_binary(explicit: Option<&Path>)` aceita path explícito
- EFEITO: `--opencode-binary /caminho/custom` é aceito pelo clap mas IGNORADO; binary é resolvido apenas via env var ou PATH
- DETALHES: enrich.rs linha 1660 e ingest_opencode.rs linha 88 chamam `find_opencode_binary()` sem `args.opencode_binary`; preflight probe (enrich.rs linha 847) também ignora
- CORREÇÃO: criada `find_opencode_binary_with_override(explicit: Option<&Path>)` que prioriza path explícito; atualizado enrich.rs (linhas 847 e 1660) e ingest_opencode.rs (linha 88) para passar `args.opencode_binary.as_deref()`
- ARQUIVOS: `src/commands/opencode_runner.rs`, `src/commands/enrich.rs`, `src/commands/ingest_opencode.rs`
- VALIDAÇÃO: `--opencode-binary /nonexistent/opencode` agora retorna erro "binary not found at explicit path"


## BUG-AUDIT-007 — FECHADO (v1.0.90 auditoria-3) — spawn_with_memory_limit (RLIMIT_AS 4GB) crashava opencode (Bun runtime)
- CAUSA: `call_opencode()` no enrich.rs usava `claude_runner::spawn_with_memory_limit()` que aplica `RLIMIT_AS = 4 GB`
- EFEITO: Bun runtime do opencode usa mmap agressivo para virtual address space e crashava com "failed to spawn thread: Resource temporarily unavailable" e "memory allocation of 14 bytes failed"
- DETALHES: glibc/Bun tentava alocar TLS (Thread-Local Storage) e falhava com ENOMEM dentro do limite de 4 GB de espaço de endereço virtual; codex/claude (Node.js) funcionam dentro de 4 GB mas Bun não
- CORREÇÃO: criada `spawn_opencode()` em opencode_runner.rs que aplica setsid para isolamento de process group mas SEM RLIMIT_AS; substituída chamada em enrich.rs
- ARQUIVOS: `src/commands/opencode_runner.rs`, `src/commands/enrich.rs`
- VALIDAÇÃO: enrich entity-descriptions com opencode real retorna status "done" em vez de crashar


## BUG-AUDIT-008 — FECHADO (v1.0.90 auditoria-3) — call_opencode() no enrich ignora json_schema (structured output impossível)
- CAUSA: parâmetro `_json_schema` (com underscore) em `call_opencode()` era declarado mas ignorado
- EFEITO: prompts de entity-descriptions e memory-bindings retornavam texto puro em vez de JSON; opencode não tem flag `--output-schema` (codex) nem `--json-schema` (claude)
- DETALHES: sem o schema no prompt, o modelo opencode respondia com prosa descritiva; `parse_json_from_opencode_text()` falhava com "could not extract valid JSON"
- CORREÇÃO: renomeado `_json_schema` para `json_schema`; quando schema não é vazio, injeta instrução "You MUST respond with ONLY valid JSON matching this schema:" no prompt antes de passar para `build_opencode_command_sync()`
- ARQUIVOS: `src/commands/enrich.rs`
- VALIDAÇÃO: enrich entity-descriptions com opencode real retorna status "done" com JSON parseável


## BUG-AUDIT-009 — FECHADO (v1.0.90 auditoria-4) — preflight probe do opencode usa spawn_with_memory_limit (RLIMIT_AS crasha Bun)
- CAUSA: `run_preflight_probe()` para `EnrichMode::Opencode` (linha 859) chamava `spawn_with_memory_limit()` que aplica RLIMIT_AS=4GB; idêntico ao BUG-AUDIT-007 mas num caminho de código separado (preflight probe vs call_opencode)
- EFEITO: `--preflight-check` com `--mode opencode` crashava o Bun runtime com "Fatal glibc error: failed to register TLS destructor: out of memory"
- DETALHES: o BUG-007 corrigiu apenas `call_opencode()` (linha 3975) mas a preflight probe é um caminho de código separado que também spawna opencode
- CORREÇÃO: substituído `claude_runner::spawn_with_memory_limit(&mut cmd)` por `opencode_runner::spawn_opencode(&mut cmd)` na preflight probe (linha 859)
- ARQUIVOS: `src/commands/enrich.rs`
- VALIDAÇÃO: `--preflight-check --mode opencode` real processa entidades com status "done" sem crash


## BUG-AUDIT-010 — FECHADO (v1.0.90 auditoria-4) — dry_run_backend mensagem de erro enganosa quando opencode eclipsado por codex
- CAUSA: `dry_run_backend` para `LlmBackendChoice::Opencode` chamava `detect_available()` que retorna o primeiro backend encontrado (codex > claude > opencode); quando codex está no PATH, `detect_available()` retorna codex e a guard `!flavour.starts_with("opencode:")` reporta "opencode not found on PATH" mesmo quando opencode ESTÁ instalado
- EFEITO: operador recebe instrução para instalar opencode quando o problema real é prioridade de detecção
- CORREÇÃO: diferenciada mensagem de erro: se codex ou claude eclipsa opencode, menciona que outro backend tem prioridade e sugere `SQLITE_GRAPHRAG_OPENCODE_BINARY`; se opencode realmente ausente, mantém mensagem original
- ARQUIVOS: `src/commands/dry_run_backend.rs`
- VALIDAÇÃO: `cargo test --lib` 875 passando, 0 falhas


## BUG-AUDIT-011 — FECHADO (v1.0.90 auditoria-4) — --names ignorado silenciosamente em entity-descriptions e body-enrich
- CAUSA: `scan_entities_without_description()` e `scan_short_body_memories()` não aceitavam `name_filter`; `scan_operation()` resolvia `name_filter` via `resolve_name_filter(args)` mas NÃO o passava para EntityDescriptions nem BodyEnrich (apenas MemoryBindings e ReEmbed recebiam)
- EFEITO: `--names "minha-entidade"` com `--operation entity-descriptions` processava TODOS os 4790 itens em vez do subconjunto solicitado; desperdício de tokens LLM e tempo
- CORREÇÃO: adicionado parâmetro `name_filter: &[String]` a `scan_entities_without_description()` e `scan_short_body_memories()`; implementada cláusula SQL `WHERE name IN (...)` parametrizada quando filtro não-vazio; atualizado call sites e 4 testes unitários
- ARQUIVOS: `src/commands/enrich.rs`
- VALIDAÇÃO: `cargo test --lib` 875 passando, `cargo clippy` ZERO warnings


## GAP-SKILL-OPENCODE-001 — FECHADO (v1.0.90 auditoria-2) — Skills EN/PT nao mencionam backend opencode
- CAUSA: skills foram reescritas na v1.0.89 com exemplos codex/claude mas opencode foi adicionado na v1.0.90 sem atualizar skills
- EFEITO: operadores nao sabem que o backend opencode existe; env vars `SQLITE_GRAPHRAG_OPENCODE_*` nao documentadas; flags CLI `--mode opencode`, `--opencode-model`, `--opencode-timeout` nao documentadas
- CORREÇÃO: adicionada seção OpenCode Backend nas skills EN e PT com env vars, flags CLI, exemplos de uso e limitações
- ARQUIVOS: `skill/sqlite-graphrag-en/SKILL.md`, `skill/sqlite-graphrag-pt/SKILL.md`


## BUG-SLOT-TEST-001 — FECHADO (v1.0.90 auditoria-5) — Teste `slot_enforces_max_concurrency` falhava por leak de `XDG_RUNTIME_DIR`
- CAUSA: `slots_dir()` prioriza `XDG_RUNTIME_DIR` sobre `SQLITE_GRAPHRAG_CACHE_DIR`; testes setavam apenas `SQLITE_GRAPHRAG_CACHE_DIR` para diretorio isolado mas `XDG_RUNTIME_DIR=/run/user/1000` prevalecia, direcionando o teste para o diretorio real de slots onde outro PID segurava `slot-0.lock`
- EFEITO: `slot_enforces_max_concurrency` falhava com "first slot" por colisao com slots reais do host
- CORREÇÃO: criada `isolate_slots_env()` que remove `XDG_RUNTIME_DIR` E seta `SQLITE_GRAPHRAG_CACHE_DIR` para temp unico; criada `restore_slots_env()` para cleanup; aplicada em `slot_enforces_max_concurrency` e `slot_releases_on_drop`
- ARQUIVOS: `src/llm_slots.rs`
- VALIDAÇÃO: 5/5 testes llm_slots passando, 875 total


## DOC-WARNING-001 — FECHADO (v1.0.90 auditoria-5) — `cargo doc` warning "unresolved link to 0" em `preflight.rs:84`
- CAUSA: `argv[0]` em doc comment era parseado pelo rustdoc como intra-doc link `[0]`
- EFEITO: `cargo doc` emitia warning "unresolved link to `0`"
- CORREÇÃO: escapado colchetes: `argv\[0\]`
- ARQUIVOS: `src/spawn/preflight.rs`


## DOC-WARNING-002 — FECHADO (v1.0.90 auditoria-5) — `cargo doc` warning "unclosed HTML tag path" em `ingest.rs:122`
- CAUSA: `<path>` em doc comment era parseado pelo rustdoc como tag HTML
- EFEITO: `cargo doc` emitia warning "unclosed HTML tag `path`"
- CORREÇÃO: convertido para code inline: `` `<path>` ``
- ARQUIVOS: `src/commands/ingest.rs`


## FMT-001 — FECHADO (v1.0.90 auditoria-5) — `cargo fmt --check` diferença em `cli.rs:74`
- CAUSA: macro `tracing::warn!` com formatação inconsistente
- EFEITO: `cargo fmt --check` reportava diferença
- CORREÇÃO: `cargo fmt` aplicado
- ARQUIVOS: `src/cli.rs`


## BUG-TIMEOUT-HARDCODE-001 — FECHADO (v1.0.90) — Timeout de embedding hardcoded em 60s causa exit 11 em corpos grandes
- CORREÇÃO: adicionado campo `timeout_override: Option<Duration>` ao `LlmEmbedding` e `LlmEmbeddingBuilder`; criados métodos `instance_embed_timeout()` e `instance_embed_timeout_for_batch()` com precedência campo > env var > default; removido `std::env::set_var` unsafe de `embed_batch_async()`; 3 funções `invoke_*_async` agora usam `self.instance_embed_timeout()` thread-safe
- ARQUIVOS: `src/extract/llm_embedding.rs`
- VALIDAÇÃO: 875 testes passando, clippy ZERO warnings, fmt ZERO diferenças

## Problema
- O timeout interno de embedding por chamada LLM é hardcoded em `const DEFAULT_EMBED_TIMEOUT_SECS: u64 = 60` em `src/extract/llm_embedding.rs:43`
- Corpos de memória grandes (15+ KB) geram múltiplos chunks que precisam ser embeddados
- Cada chamada individual de embedding (`invoke_claude_async`, `invoke_codex_async`, `invoke_opencode_async`) usa `embed_timeout()` que retorna 60s por padrão
- O `embed_timeout_for_batch()` escala o timeout com 15s por item extra mas APENAS para o wrapper `embed_batch_async` (linhas 499-507) que manipula a env var `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` temporariamente
- A env var `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` é a ÚNICA forma de sobrescrever o default de 60s
- NÃO existe flag CLI `--embed-timeout` para o operador ajustar o timeout sem usar env var
- O valor de 60s é insuficiente quando o endpoint OAuth do modelo LLM tem latência elevada (OAuth cold-start, rate-limit backoff, modelo grande)
- O valor de 15s por item extra no batch scaling é TAMBÉM hardcoded sem possibilidade de configuração


## Consequências do Problema
- `remember` com corpo grande falha com exit 11 (`erro de embedding: no LLM backends available; fallback chain exhausted`) quando o timeout de 60s é atingido
- O operador recebe mensagem de erro enganosa sobre "fallback chain exhausted" quando o problema real é timeout
- O operador precisa descobrir a env var `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` por leitura de código para contornar o problema
- A env var NÃO é documentada em `--help` do comando `remember`
- O `embed_timeout_for_batch()` manipula a env var de processo em runtime (`std::env::set_var`) o que é `unsafe` em contexto multi-thread desde Rust 1.83+ e viola a regra `rules-rust` de thread-safety
- O workaround por env var é frágil: se dois threads de embedding rodam simultaneamente, a env var é compartilhada e um thread pode ler o timeout do outro
- Modelos LLM com latência variável (OAuth cold-start, rate-limit, endpoints remotos) precisam de timeout adaptativo, não fixo
- O valor de 60s foi calibrado para `gpt-5.5` local mas NÃO é adequado para modelos menores ou endpoints remotos com maior latência


## Causa Raiz do Problema
- CAUSA PRIMÁRIA: `DEFAULT_EMBED_TIMEOUT_SECS = 60` é uma constante hardcoded no código-fonte sem possibilidade de configuração via CLI
- CAUSA SECUNDÁRIA: a env var `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` é o ÚNICO mecanismo de override mas NÃO é exposta como flag CLI `--embed-timeout`
- CAUSA TERCIÁRIA: `embed_timeout_for_batch()` (linha 58) usa `std::env::set_var` para manipular a env var em runtime, violando thread-safety
- CAUSA QUATERNÁRIA: o valor de 15s por item extra no batch scaling (linha 60) é TAMBÉM hardcoded sem configuração
- VIOLAÇÃO: `rules-rust` Seção "PROIBIDO — Hardcode": "NUNCA hardcode caminhos, URLs ou configurações no código; DEVE usar variáveis de ambiente para configurações externas"
- VIOLAÇÃO PARCIAL: a env var existe mas NÃO está exposta na interface CLI, forçando o operador a ler o código-fonte para descobrir o mecanismo de override


## Solução Proposta
- Adicionar flag CLI `--embed-timeout <SECONDS>` nos comandos `remember`, `edit`, `ingest` e `enrich`
- Estabelecer precedência: `--embed-timeout` CLI > `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` env var > default 60s
- Documentar a flag `--embed-timeout` no `--help` de cada comando que usa embedding
- Remover a manipulação de env var em runtime (`std::env::set_var`) de `embed_timeout_for_batch()`
- Passar o timeout como parâmetro explícito para as funções `invoke_*_async()` em vez de ler env var global
- Externalizar o valor de 15s por item extra como constante documentada ou flag opcional
- Considerar timeout adaptativo baseado no tamanho do corpo (bytes) em vez de contagem de chunks
- Validar range da flag: mínimo 10s, máximo 3600s (consistente com o clamp existente na linha 49)


## Benefícios da Solução
- Operador pode ajustar o timeout de embedding sem ler código-fonte
- Operador pode usar `--embed-timeout 300` para corpos grandes sem env var
- Thread-safety restaurada: sem `set_var` em runtime multi-thread
- Eliminação de hardcode: valor de timeout vem da CLI ou env var, NUNCA do código
- Mensagem de erro mais precisa: "embedding timed out after 60s" em vez de "fallback chain exhausted"
- Timeout adaptativo permite lidar com latência variável de endpoints OAuth


## Como Solucionar — Etapas Incrementais
- Etapa 1 — Flag CLI: adicionar `--embed-timeout <SECONDS>` na struct `Args` dos comandos `remember`, `edit`, `ingest` e `enrich` com `#[arg(long, default_value_t = 60)]`
- Etapa 2 — Propagação: passar o timeout como parâmetro explícito para `LlmEmbedding::new()` ou `LlmEmbeddingBuilder` em vez de ler env var
- Etapa 3 — Remover set_var: eliminar `std::env::set_var("SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS", ...)` de `embed_batch_async()` e substituir por parâmetro de timeout no call
- Etapa 4 — Batch scaling: mover o cálculo `base + 15s * (n-1)` para dentro do builder como campo configurável
- Etapa 5 — Documentação: atualizar skills EN/PT, CLAUDE.md e `--help` text do clap
- Etapa 6 — Testes: validar que `--embed-timeout 120` propaga corretamente para cada backend


## Relações Causa x Efeito
- CAUSA: `DEFAULT_EMBED_TIMEOUT_SECS = 60` hardcoded → EFEITO: corpos de 15+ KB falham com exit 11 quando LLM responde em >60s
- CAUSA: ausência de flag `--embed-timeout` → EFEITO: operador precisa ler código-fonte para descobrir `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS`
- CAUSA: `embed_timeout_for_batch()` usa `std::env::set_var` → EFEITO: race condition em multi-thread (Rust 1.83+ considera `set_var` unsafe)
- CAUSA: valor de 15s/item hardcoded sem configuração → EFEITO: operador não pode ajustar scaling para modelos mais lentos
- CAUSA: mensagem de erro reporta "fallback chain exhausted" → EFEITO: operador investiga backends quando o problema real é timeout
- CAUSA: clamp `10..=3600` na env var (linha 49) → EFEITO: regra de validação está correta mas não está exposta na interface CLI


## Referências no Código-Fonte
- `src/extract/llm_embedding.rs:43` — `const DEFAULT_EMBED_TIMEOUT_SECS: u64 = 60` (hardcode)
- `src/extract/llm_embedding.rs:45-52` — `fn embed_timeout()` lê env var com fallback para 60s
- `src/extract/llm_embedding.rs:58-62` — `fn embed_timeout_for_batch()` escala timeout com 15s/item
- `src/extract/llm_embedding.rs:499-507` — `embed_batch_async()` manipula env var com `set_var`
- `src/extract/llm_embedding.rs:729` — `invoke_claude_async()` usa `embed_timeout()`
- `src/extract/llm_embedding.rs:873` — `invoke_codex_async()` usa `embed_timeout()`
- `src/extract/llm_embedding.rs:952` — `invoke_opencode_async()` usa `embed_timeout()`
- `src/llm/exit_code_hints.rs:133` — mensagem de hint menciona `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS`
- `src/extract/llm_embedding.rs:1195` — teste `embed_timeout_default_is_60` afirma o hardcode


## BUG-WINDOWS-001 — FECHADO (v1.0.90) — Compilação falha no Windows por uso de `std::os::unix` sem guard `#[cfg(unix)]`
- CORREÇÃO: criado helper `extract_exit_info()` com branches `#[cfg(unix)]` e `#[cfg(not(unix))]`; substituídos 3 blocos inline idênticos (invoke_claude, invoke_codex, invoke_opencode) por chamada ao helper; DRY + cross-platform
- ARQUIVOS: `src/extract/llm_embedding.rs`
- VALIDAÇÃO: 875 testes passando, clippy ZERO warnings, doc ZERO warnings

## Problema
- 3 usos de `std::os::unix::process::ExitStatusExt` em `src/extract/llm_embedding.rs` (linhas 786, 896, 975) NÃO possuem guard `#[cfg(unix)]`
- O módulo `std::os::unix` NÃO existe no target `x86_64-pc-windows-msvc`
- O método `.signal()` de `ExitStatusExt` NÃO existe em `std::process::ExitStatus` no Windows
- O `cargo install sqlite-graphrag` no Windows falha com 4 erros de compilação:
  - `error[E0433]: failed to resolve: could not find 'unix' in 'os'` (linhas 786 e 896)
  - `error[E0599]: no method named 'signal' found for struct 'ExitStatus'` (linhas 787 e 897)
- Outros 8 usos de `std::os::unix` no codebase POSSUEM `#[cfg(unix)]` corretamente (ex.: `claude_runner.rs:47`, `opencode_runner.rs:383`, `connection.rs:218`, etc.)
- Os 3 usos problemáticos estão dentro de blocos `else` em `if let Some(code) = output.status.code() { ... } else { ... }` onde o `else` tenta extrair o signal Unix do processo


## Consequências do Problema
- `cargo install sqlite-graphrag` no Windows FALHA com erro de compilação
- Usuários Windows NÃO podem instalar a CLI via crates.io
- O CI `windows-build-check` (documentado na v1.0.68) deveria ter capturado este problema mas os erros sugerem que a verificação não cobria `llm_embedding.rs`
- A promessa de cross-platform do Cargo.toml (metadata `windows-sys = "0.59.0"`) é VIOLADA
- 3 dos 13 usos de `std::os::unix` no codebase estão desprotegidos — inconsistência com os outros 10 que seguem o padrão correto


## Causa Raiz do Problema
- CAUSA PRIMÁRIA: os 3 blocos `else` nas linhas 786, 896 e 975 de `llm_embedding.rs` usam `use std::os::unix::process::ExitStatusExt` e `.signal()` sem `#[cfg(unix)]` guard
- CAUSA SECUNDÁRIA: no Unix, quando um processo é terminado por signal (ex.: SIGKILL), `ExitStatus::code()` retorna `None` e o signal é obtido via `ExitStatusExt::signal()`; no Windows este conceito NÃO existe — processos terminados sempre têm um exit code
- CAUSA TERCIÁRIA: os 3 blocos foram adicionados durante o pipeline de embedding (v1.0.79 G42) sem verificação cross-platform
- CAUSA QUATERNÁRIA: os outros 10 usos de `std::os::unix` no codebase POSSUEM guards corretos, indicando que o padrão era conhecido mas não foi aplicado nos 3 blocos de embedding
- AGRAVANTE: o ADR-0018 (v1.0.69) documenta que "v1.0.68 é o primeiro release desde v1.0.65 que compila no Windows" mas os 3 blocos foram adicionados na v1.0.79 sem regressão de CI


## Solução Proposta
- Envolver cada bloco `else` com `#[cfg(unix)]` e adicionar um `#[cfg(not(unix))]` alternativo que retorna `(None, None)` para o par `(exit_code, signal)`
- Padrão correto para cada um dos 3 locais:
```rust
let (exit_code, signal) = if let Some(code) = output.status.code() {
    (Some(code), None)
} else {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        (None, output.status.signal())
    }
    #[cfg(not(unix))]
    {
        (None, None)
    }
};
```
- Aplicar a mesma correção nos 3 locais: linhas 782-788, 892-898 e 971-977
- Garantir que o CI `windows-build-check` inclua `src/extract/llm_embedding.rs` no escopo de verificação
- Considerar criar uma função helper `exit_code_and_signal(status: &ExitStatus) -> (Option<i32>, Option<i32>)` para eliminar duplicação dos 3 blocos idênticos


## Benefícios da Solução
- `cargo install sqlite-graphrag` compila no Windows sem erros
- Consistência: todos os 13 usos de `std::os::unix` passam a ter `#[cfg(unix)]` guard
- A promessa cross-platform do crate é restaurada
- O CI `windows-build-check` previne regressões futuras
- O helper `exit_code_and_signal()` elimina duplicação de 3 blocos idênticos (DRY)


## Como Solucionar — Etapas Incrementais
- Etapa 1 — Helper: criar `fn exit_code_and_signal(status: &std::process::ExitStatus) -> (Option<i32>, Option<i32>)` com `#[cfg(unix)]` e `#[cfg(not(unix))]` em `llm_embedding.rs` ou em `src/llm/exit_code_hints.rs`
- Etapa 2 — Substituir: trocar os 3 blocos inline (linhas 782-788, 892-898, 971-977) por chamada ao helper
- Etapa 3 — CI: verificar que `cargo check --target x86_64-pc-windows-msvc --lib` passa (requer toolchain Windows cross-compilation)
- Etapa 4 — Teste: `cargo test --lib` confirma que a refatoração não quebrou a lógica no Linux


## Relações Causa x Efeito
- CAUSA: `use std::os::unix::process::ExitStatusExt` sem `#[cfg(unix)]` → EFEITO: `error[E0433]: could not find 'unix' in 'os'` no Windows
- CAUSA: `.signal()` chamado em `ExitStatus` no Windows → EFEITO: `error[E0599]: no method named 'signal' found` no Windows
- CAUSA: 3 blocos idênticos sem guard → EFEITO: 4 erros de compilação (2 E0433 + 2 E0599) no Windows
- CAUSA: pipeline de embedding (v1.0.79) adicionou blocos sem verificação cross-platform → EFEITO: regressão de compilabilidade no Windows
- CAUSA: CI `windows-build-check` não cobria `llm_embedding.rs` → EFEITO: regressão não detectada automaticamente
- CAUSA: 10 usos protegidos + 3 desprotegidos → EFEITO: inconsistência de padrão no codebase


## Referências no Código-Fonte
- `src/extract/llm_embedding.rs:786-787` — `invoke_claude_async()`: `use std::os::unix::process::ExitStatusExt; (None, output.status.signal())` SEM guard
- `src/extract/llm_embedding.rs:896-897` — `invoke_codex_async()`: idêntico, SEM guard
- `src/extract/llm_embedding.rs:975-976` — `invoke_opencode_async()`: idêntico, SEM guard
- `src/commands/claude_runner.rs:47-49` — CORRETO: `#[cfg(target_os = "linux")]` + `use std::os::unix::process::CommandExt`
- `src/commands/opencode_runner.rs:383-385` — CORRETO: `#[cfg(target_os = "linux")]` + `use std::os::unix::process::CommandExt`
- `src/storage/connection.rs:218-220` — CORRETO: `#[cfg(unix)]` + `use std::os::unix::fs::PermissionsExt`
- `src/commands/enrich.rs:4124-4125` — CORRETO: `#[cfg(unix)]` + `use std::os::unix::fs::PermissionsExt`
- `src/commands/backup.rs:164-166` — CORRETO: `#[cfg(unix)]` + `use std::os::unix::fs::PermissionsExt`
- `src/commands/sync_safe_copy.rs:83-85` — CORRETO: `#[cfg(unix)]` + `use std::os::unix::fs::PermissionsExt`
- `src/extract/llm_embedding.rs:1035-1037` — CORRETO: `#[cfg(unix)]` + `use std::os::unix::fs::PermissionsExt` (em testes)


## BUG-PENDING-CLEANUP-DB-001 — FECHADO (v1.0.90) — `pending cleanup` Não Aceita `--db`

## Problema
- `pending cleanup` NÃO aceitava a flag `--db` para especificar caminho do banco
- `pending list` e `pending show` aceitam `--db` (corrigidos no GAP-E2E-010b, v1.0.89)
- `PendingCleanupArgs` foi esquecido na mesma correção
- `run_cleanup()` chamava `open_conn(None)` hardcoded, ignorando qualquer override de PATH

## Causa Raiz
- GAP-E2E-010b (v1.0.89) adicionou `--db` a `PendingListArgs` e `PendingShowArgs`
- `PendingCleanupArgs` NÃO foi incluído nessa correção
- Resultado: `pending cleanup --db /caminho` falhava com exit 2 (argumento inesperado)

## Correção
- Adicionado campo `db: Option<String>` com `#[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]` em `PendingCleanupArgs`
- Alterado `run_cleanup()` de `open_conn(None)` para `open_conn(args.db.as_deref())`

## Validação
- `cargo check --lib` — ZERO erros
- `cargo clippy --lib --all-features` — ZERO warnings
- `cargo fmt --check` — ZERO diferenças
- `cargo test --lib` — 875 passaram, 0 falharam
- `pending cleanup --db /tmp/e2e-diag.sqlite --yes --json` — retorna JSON corretamente
- `pending cleanup --help` — mostra `--db <DB>` nas opções

## Arquivos Modificados
- `src/commands/pending.rs` — adicionado campo `db` em `PendingCleanupArgs`, alterado `open_conn` call


## BUG-REMEMBER-BATCH-DRYRUN-001 — `remember-batch --dry-run` inexistente (FECHADO v1.0.90)
- Status: FECHADO
- Severidade: MÉDIA
- Componente: `src/commands/remember_batch.rs`

## Problema
- A SKILL documenta `USAR --dry-run para validar o lote sem persistir` para `remember-batch`
- A struct `RememberBatchArgs` NÃO tinha campo `dry_run`
- `remember-batch --dry-run` falhava com exit 2 (`unexpected argument`)
- O Clap sugeria `--dry-run-backend` (flag global) como alternativa incorreta

## Causa Raiz
- `remember` (individual) e `ingest` (diretório) implementam `--dry-run`
- `remember-batch` foi adicionado na v1.0.67 mas a flag `--dry-run` foi omitida
- A SKILL herdou a documentação do `remember` sem verificar a implementação

## Correção
- Adicionado campo `dry_run: bool` com `#[arg(long)]` em `RememberBatchArgs`
- Implementado caminho de dry-run no `run()` que valida inputs e emite preview events
- Status de preview: `would_create`, `would_update`, `would_fail_duplicate`
- JSON inválido retorna `failed` com mensagem de erro
- ZERO escrita ao banco e ZERO embedding no modo dry-run

## Validação
- `cargo check --lib` — ZERO erros
- `cargo clippy --lib --all-features` — ZERO warnings
- `cargo fmt --check` — ZERO diferenças
- `cargo test --lib` — 875 passaram, 0 falharam
- `remember-batch --dry-run` — emite preview events sem persistir
- `remember-batch --dry-run --force-merge` — reporta `would_update` para existentes
- `remember-batch --dry-run` com JSON inválido — reporta `failed` com mensagem

## Arquivos Modificados
- `src/commands/remember_batch.rs` — adicionado campo `dry_run`, implementado caminho de preview


## BUG-INGEST-SKIP-EMBED-001 — FECHADO (v1.0.90) — ingest ignora --skip-embedding-on-failure
- Descoberto por: auditoria e2e v1.0.90
- Status: FECHADO
- Severidade: ALTA
- Componente: `src/commands/ingest.rs`

## Problema
- A flag global `--skip-embedding-on-failure` é declarada no struct `Cli` com `global = true`
- A flag aparece no `--help` do `ingest` e é documentada na SKILL
- O worker Phase A do `ingest` (`process_file_phase_a`) chama `embed_passage_with_choice()` com `?` (propagação direta de erro)
- Quando embedding falha (ex: `--llm-backend none`), o arquivo inteiro falha com `status: "failed"`
- O `remember` honra a flag via `should_skip_embedding_on_failure()` em guards `match`, mas o `ingest` NÃO

## Causa Raiz
- `remember.rs` usa `match` com guard `Err(e) if skip_embed => { ... }` para capturar erros de embedding
- `ingest.rs` usa `?` direto em 3 pontos: `embed_passage_with_choice`, `embed_passages_parallel_local`, `embed_entity_texts_cached`
- A flag global `--skip-embedding-on-failure` define a env var, mas o worker do ingest nunca a consulta
- `StagedFile.embedding` era `Vec<f32>` (obrigatório), impedindo representação de embedding ausente

## Correção
- Mudado `StagedFile.embedding` de `Vec<f32>` para `Option<Vec<f32>>`
- Mudado `StagedFile.entity_embeddings` de `Vec<Vec<f32>>` para `Option<Vec<Vec<f32>>>`
- Adicionado `should_skip_embedding_on_failure()` guard nos 3 pontos de embedding do worker
- Erros de `AppError::Validation` continuam propagando (são bugs reais, não falha de backend)
- Phase B (`persist_staged`) agora condiciona `upsert_vec` a `embedding.is_some()`
- Memórias ingeridas sem embedding ficam em `vec_memories_missing` para re-embed posterior

## Validação
- `cargo check --lib` — ZERO erros
- `cargo clippy --lib --all-features` — ZERO warnings
- `cargo fmt --check` — ZERO diferenças
- `RUSTDOCFLAGS="-D warnings" cargo doc --lib --no-deps` — ZERO warnings
- `cargo test --lib` — 875 passaram, 0 falharam
- `ingest --llm-backend none --skip-embedding-on-failure` — `files_succeeded: 2, files_failed: 0`
- `health` reporta `vec_memories_missing: 2` (correto, embedding NULL)
- `ingest --mode none` — continua funcionando normalmente
- `ingest --dry-run` — continua funcionando normalmente
- Re-ingestão do mesmo diretório — `status: "skipped", action: "duplicate"` (correto)

## Arquivos Modificados
- `src/commands/ingest.rs` — `StagedFile` com embedding/entity_embeddings opcionais, guards de skip no worker e Phase B condicional


## BUG-GRAPH-DB-PROPAGATION-001 — FECHADO (v1.0.90) — `graph --db X --namespace Y stats|traverse|entities` ignora flags do pai

## Problema
- `graph --db X --namespace Y stats` ignora `--db` e `--namespace` passados no comando pai `GraphArgs`
- Cada subcomando (`Stats`, `Traverse`, `Entities`) tem suas PRÓPRIAS flags `--db` e `--namespace`
- Clap NÃO propaga flags do comando pai para subcomandos aninhados
- Quando o usuário faz `graph --db /tmp/isolated.sqlite stats`, o `--db` fica em `GraphArgs.db` mas `run_stats()` lê `GraphStatsArgs.db` que é `None`
- O fallback `AppPaths::resolve(None)` carrega o banco do CWD (`graphrag.sqlite`) em vez do banco solicitado
- Afeta os 3 subcomandos: `stats`, `traverse` e `entities`
- Resultado: dados do banco ERRADO são retornados silenciosamente (sem erro)

## Causa Raiz
- `run()` em `graph_export.rs` despachava os subcomandos SEM propagar `args.db` e `args.namespace` do pai
- Código anterior: `Some(GraphSubcommand::Stats(a)) => run_stats(a)` — passava `a` diretamente sem herdar do pai

## Correção
- Propagação condicional: se o subcomando NÃO tem `--db`/`--namespace` próprio (é `None`), herda do pai `GraphArgs`
- Se o subcomando TEM flags próprias, respeita a do subcomando (precedência do mais específico)
- Aplicado aos 3 subcomandos: `Traverse`, `Stats`, `Entities`

## Validação
- `cargo check --lib` — ZERO erros
- `cargo clippy --lib --all-features` — ZERO warnings
- `cargo fmt --check` — ZERO diferenças
- `cargo test --lib` — 875 passaram, 0 falharam
- `graph --db /tmp/isolated.sqlite --namespace audit-e2e stats` — retorna `node_count: 2` (correto)
- `graph --db /tmp/isolated.sqlite --namespace audit-e2e traverse --from entity-a` — retorna `hops_count: 2` (correto)
- `graph --db /tmp/isolated.sqlite --namespace audit-e2e entities` — retorna `count: 2` (correto)
- Antes da correção: todos retornavam dados do banco do CWD (9602 nós)

## Arquivos Modificados
- `src/commands/graph_export.rs` — `run()` propagando `args.db` e `args.namespace` para subcomandos quando seus campos são `None`


## BUG-PENDING-EMBEDDINGS-DB-001 — FECHADO (v1.0.90) — `pending-embeddings list|abandon` não aceita `--db`

## Problema
- `pending-embeddings list --db /path/to/db.sqlite` retorna erro: `unexpected argument '--db' found`
- `pending-embeddings abandon --db /path/to/db.sqlite` retorna o mesmo erro
- `PendingEmbeddingsListArgs` e `PendingEmbeddingsAbandonArgs` NÃO tinham campo `db`
- `open_conn()` chamava `AppPaths::resolve(None)` hardcoded, ignorando qualquer override de DB
- Subcomandos irmãos (`embedding status/list`, `vec stats/orphan-list`, `pending list/cleanup`) ACEITAM `--db`

## Causa Raiz
- `open_conn()` hardcodava `AppPaths::resolve(None)` sem aceitar parâmetro `db`
- Structs clap dos subcomandos não declaravam `#[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]`

## Correção
- Adicionado campo `pub db: Option<String>` com `#[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]` a `PendingEmbeddingsListArgs` e `PendingEmbeddingsAbandonArgs`
- Alterado `open_conn()` para aceitar `db: Option<&str>` e passá-lo a `AppPaths::resolve(db)`
- `run_list()` e `run_abandon()` agora passam `args.db.as_deref()` para `open_conn()`

## Validação
- `cargo check --lib` — ZERO erros
- `cargo clippy --lib --all-features` — ZERO warnings
- `cargo fmt --check` — ZERO diferenças
- `cargo test --lib` — 875 passaram, 0 falharam
- `pending-embeddings list --db /tmp/test.sqlite` — exit 0, `count: 0` (correto)
- `pending-embeddings abandon --db /tmp/test.sqlite --status pending --yes` — exit 0, `candidates: 0` (correto)

## Arquivos Modificados
- `src/commands/pending_embeddings.rs` — campo `db` adicionado a 2 structs, `open_conn()` parametrizado


## BUG-LIST-TOTAL-COUNT-001 — FECHADO (v1.0.90) — `list` retorna `total_count` da pagina em vez do total global

## Problema
- `list --limit N --offset M` retorna `total_count` igual ao numero de items na pagina atual
- O docstring na struct `ListResponse` documenta `total_count` como "Total number of matching memories in the namespace (ignoring limit/offset)"
- A implementacao calculava `total_count = items.len()` APOS aplicar LIMIT/OFFSET
- Consumidores nao conseguiam calcular o numero total de paginas
- `truncated` tambem estava semanticamente incorreto: `items.len() >= lim` em vez de comparar contra o total global

## Causa Raiz
- Linha 152 de `src/commands/list.rs`: `let total_count = items.len()` usa o vetor JA paginado
- Nao existia query `SELECT COUNT(*)` separada na storage layer `src/storage/memories.rs`
- O `graph entities` NAO tinha o bug porque JA fazia query COUNT separada

## Solucao Aplicada
- Criada funcao `memories::count()` em `src/storage/memories.rs` com 4 variantes de query COUNT
- Atualizado `src/commands/list.rs` para chamar `memories::count()` antes de montar a resposta
- `truncated` agora compara `items.len() < total_count` (semantica correta)

## Validacao
- `list --limit 2 --offset 0` retorna `total_count: 15, items: 2, truncated: true`
- `list --limit 2 --offset 14` retorna `total_count: 15, items: 1, truncated: true`
- `list` sem limit retorna `total_count: 15, items: 15, truncated: false`
- `list --type note --limit 2` retorna `total_count: 9, items: 2, truncated: true`
- `cargo check --lib` — ZERO erros
- `cargo test --lib` — 875 passaram, 0 falharam
- `cargo clippy --lib --all-features` — ZERO warnings
- `cargo fmt --check` — ZERO diferencas

## Arquivos Modificados
- `src/storage/memories.rs` — funcao `count()` adicionada
- `src/commands/list.rs` — `total_count` agora vem de `memories::count()`, `truncated` corrigido
