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

## Referências

- `gaps.md:413-670`
- `src/cli.rs` (flag `--llm-backend`)
- `src/extract/llm_backend.rs` (trait + 4 implementações)
- `src/embedder.rs:embed_with_fallback`
