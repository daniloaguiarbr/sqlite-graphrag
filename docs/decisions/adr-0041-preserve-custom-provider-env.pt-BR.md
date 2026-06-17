# ADR-0041: Preservar Credenciais de Provider Customizado no Env dos Subprocessos LLM

- **Status**: Aceito
- **Data**: 2026-06-17
- **Versão**: v1.0.83 (resolve parcialmente GAP-058 / G58)
- **Autores**: tech-lead

## Contexto

O `sqlite-graphrag` v1.0.76+ bloqueia uso de providers customizados (MiniMax/api.minimax.io, OpenRouter, gateways corporativos) porque os três spawners de LLM aplicam `env_clear()` e depois injetam apenas uma whitelist restrita que NÃO inclui `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL` nem outras variáveis de provider customizado.

### Causa raiz verificada

| Fator | Local | Linha |
|---|---|---|
| `env_clear()` remove credenciais | `src/commands/claude_runner.rs` | 278, 286 |
| Whitelist incompleto | `src/commands/claude_runner.rs` | 14-35 |
| Duplicação em `codex_spawn` | `src/commands/codex_spawn.rs` | 277-293 |
| Duplicação em `ingest_claude` | `src/commands/ingest_claude.rs` | 299-319 |
| Guard OAuth-only correto | `claude_runner.rs:273`, `ingest_claude.rs:282`, `codex_spawn.rs:259`, `extract/llm_embedding.rs:237-253` | rejeitam apenas `ANTHROPIC_API_KEY`/`OPENAI_API_KEY` |

### Distinção semântica crítica

- `ANTHROPIC_API_KEY` — chave de API paga Anthropic oficial (`sk-ant-...`), rejeitada por design (ADR-0011)
- `ANTHROPIC_AUTH_TOKEN` — token OAuth usado por Claude Code com provider customizado, sem custo de API (pago via assinatura)
- A v1.0.69 generalizou o guard e acabou rejeitando implicitamente via `env_clear()`. A v1.0.83 corrige a interpretação literal.

### Impacto em produção

Em 2026-06-17, o cenário `ANTHROPIC_AUTH_TOKEN=sk-cp-...` + `ANTHROPIC_BASE_URL=https://api.minimax.io/anthropic` resultava em:

1. `remember`/`edit`/`ingest` retornam `exit 11` (embedding failure)
2. Memória parcial gravada em `memories` sem embedding em `memory_embeddings`
3. Entrada órfã cresce em `pending_embeddings` a cada tentativa
4. `recall`/`hybrid-search` perdem precisão semântica

### Cross-reference com G58

O gap `gap-g58-recall-sem-fallback-deterministic-2026-06-13` documenta fadiga OAuth como ponto único de falha em `recall`/`hybrid-search`. Este ADR resolve G58 parcialmente: provider customizado via env vars contorna a quota OAuth oficial.

## Decisão

Preservar seis variáveis de provider customizado no whitelist compartilhado dos três spawners:

- `ANTHROPIC_AUTH_TOKEN` — token OAuth de provider customizado para Claude Code
- `ANTHROPIC_BASE_URL` — endpoint customizado Anthropic-compatible
- `OPENAI_BASE_URL` — endpoint customizado OpenAI-compatible
- `CLAUDE_CODE_ENTRYPOINT` — override Claude Code-specific
- `DISABLE_TELEMETRY` — opt-out de telemetria do provider
- `OTEL_EXPORTER_OTLP_ENDPOINT` — collector OTel customizado

### Helper compartilhado

Criar `src/spawn/env_whitelist.rs` com `preserved_env_vars()` (lista canônica) e `apply_env_whitelist(cmd, strict)` (aplica whitelist ou `env_clear` estrito quando `strict=true`). Refatorar os três spawners para usar o helper.

### Flag opt-out para compliance

Adicionar flag global `--strict-env-clear` (env `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1`). Quando ativa, `apply_env_whitelist` preserva apenas `PATH` mínimo — nenhum credential customizado passa adiante. Uso restrito a ambientes com política de segurança que exige env_clear estrito.

### Mensagem OAuth-only orientativa

Os quatro locais que rejeitam `ANTHROPIC_API_KEY`/`OPENAI_API_KEY` agora emitem mensagem de erro apontando para OAuth subscription como resolução:

```
OAuth-only mandate violated (ANTHROPIC_API_KEY in env)
Resolution: Use OAuth subscription (Claude Pro/Max) and ANTHROPIC_AUTH_TOKEN instead.
See ADR-0011 and ADR-0041.
```

## Consequências

### Positivas

- Providers customizados funcionam (MiniMax, OpenRouter, gateways)
- Defesa em profundidade OAuth-only preservada (rejeição de `ANTHROPIC_API_KEY`/`OPENAI_API_KEY` intacta)
- Helper compartilhado elimina duplicação dos três spawners
- Flag opt-out cobre cenário compliance
- Cross-reference com G58 estabelece narrativa coerente

### Negativas

- Aumento de 6 strings no whitelist compartilhado — overhead zero em runtime
- Helper compartilhado exige mudança coordenada nos três spawners — diff mecânico mas múltiplos arquivos
- Flag `--strict-env-clear` adiciona superfície de teste (mas opt-in)

### SEM telemetria nova

O fix é silencioso. Nenhum `tracing::info!` adicionado para uso de provider customizado. A única mensagem de log adicional é a do guard OAuth-only (user-facing error, não telemetria). Esta é uma decisão consciente para minimizar observabilidade em dados sensíveis.

## Alternativas Consideradas

1. **Flag global `--preserve-custom-env` opt-in**: fricção operacional, usuário precisaria descobrir a flag — descartado
2. **Apenas documentar workaround via env manual**: não resolve causa raiz, requer edição manual dos spawners a cada release — descartado
3. **Refator completo dos spawners em abstração única**: blast radius maior, ADR-0011 já marca como follow-up — fora do escopo deste patch
4. **Adicionar telemetria de provider customizado**: vazamento potencial de tokens via logs estruturados, opt-out complexo — proibido por restrição do usuário

## Referências

- `src/commands/claude_runner.rs:14-35` (whitelist Unix), `:38-48` (whitelist Windows), `:273` (guard), `:278,286` (env_clear)
- `src/commands/codex_spawn.rs:259` (guard), `:261,272` (env_clear), `:277-293` (whitelist inline)
- `src/commands/ingest_claude.rs:282` (guard), `:299-319` (whitelist)
- `src/extract/llm_embedding.rs:237-253` (guard, sem mudança)
- `src/spawn/env_whitelist.rs` (novo helper compartilhado)
- `tests/claude_runner_env.rs` (novo arquivo de teste, 6 cenários)
- `gap-g58-recall-sem-fallback-deterministic-2026-06-13` (gap parcialmente resolvido)
- `codex-cli-0-137-mudancas-stdin-auth-approval` (codex CLI 0.137.0 lê `~/.codex/auth.json` filesystem, não env)
- ADR-0011 (OAuth-only mandate v1.0.69), ADR-0025 (OAuth-only embedding v1.0.76)
- ADR-0033 (Windows resilience)
