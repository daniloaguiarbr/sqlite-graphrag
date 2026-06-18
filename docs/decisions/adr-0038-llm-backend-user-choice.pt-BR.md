# ADR-0038: Escolha Explícita de Backend LLM pelo Usuário

- **Status**: Aceito
- **Data**: 2026-06-15
- **Versão**: v1.0.82 (resolve GAP-003)
- **Autores**: tech-lead

## Contexto

Pipeline de embedding hardcodou `codex exec` como backend LLM único. Usuários sem ChatGPT
Pro OAuth mas com Claude Pro/Max ficavam bloqueados. Transcript 2026-06-15: 19+ codex
simultâneos saturaram rate limit OAuth.

## Decisão

Trait `LlmBackendFactory` com 4 implementações e flag CLI global:

- `CodexFactory { binary, model }`
- `ClaudeFactory { binary, model }`
- `NullFactory` (embedding NULL)
- `AutoFactory` (PATH probing: codex → claude → null)

Flag global `--llm-backend <auto|claude|codex|none>` (env: `SQLITE_GRAPHRAG_LLM_BACKEND`,
default `auto`). Default `auto` é 100% retrocompatível com v1.0.81.

`with_default_codex()` permanece como helper de compatibilidade.

## Consequências

### Positivas
- Usuário escolhe CLI headless por invocação
- CI sem LLM CLI usa `--llm-backend=none` (embedding NULL em `pending_embeddings`)
- Default `auto` é retrocompatível

### Negativas
- 4 implementações exigem manutenção quando API evolui
- `AutoFactory` faz PATH probing no startup (latência ~5-10ms)
- Validação de modelo (whitelist ChatGPT Pro OAuth) ainda é responsabilidade do caller
## Alternativas Consideradas

1. Manter `Auto` como única opção, sem override — REJEITADO: casos de OAuth exhausted em codex exigem bypass explícito
2. Flag separada `--force-claude` em vez de `--llm-backend claude` — REJEITADO: aumenta superfície CLI; `claude` já é enum value
3. Aceitar `codex,claude,none` apenas — REJEITADO: `none` deve ser opt-in explícito, não dentro de chain
4. Renomear `--llm-backend` para `--embedder` — REJEITADO: quebra compat com v1.0.82


## Referências

- `gaps.md:413-670`
- `src/cli.rs` (flag `--llm-backend`)
- `src/extract/llm_backend.rs` (trait + 4 implementações)
- `src/embedder.rs:embed_with_fallback`
### Refinado por ADR-0042 (v1.0.84)

ADR-0042 (`docs/decisions/adr-0042-claude-backend-split.pt-BR.md`) separou o entry point Claude de modo que `LlmBackendChoice::to_chain()` não mais roteia silenciosamente `Claude` através de codex via `LlmEmbedding::detect_available`. O novo `LlmEmbeddingBuilder` (`src/extract/llm_embedding.rs:232+`) e `embed_via_claude_local` (`src/embedder.rs:190+`) tornam o caminho `--llm-backend claude` observável via `backend_invoked: "claude"` em 7 envelopes (edit, embedding-status, enrich-summary, hybrid-search, ingest-summary, recall, remember). O pre-flight `--dry-run-backend` é a forma recomendada para verificar qual backend será invocado antes de qualquer chamada de embedding.
