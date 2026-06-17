# ADR-0038: Escolha Explícita de Backend LLM pelo Usuário

- **Status**: Aceito
- **Data**: 2026-06-15
- **Versão**: v1.0.82 (resolve GAP-003)
- **Autores**: tech-lead

## Contexto

O pipeline de embedding hardcodou `codex exec` como backend LLM único. A função
`composite_backend::default_backend` retornava `Arc::new(LlmBackend::with_default_codex())` em
todos os caminhos, sem alternativa para usuários:

- Sem ChatGPT Pro OAuth mas com Claude Pro/Max: não conseguem usar a CLI
- Com Claude Pro/Max mas preferindo codex por custo/latência: não conseguem trocar
- Em ambiente de CI/dev sem qualquer CLI LLM: pipeline trava em stderr literal `codex not
  found`

Transcript 2026-06-15 mostrou 19+ codex simultâneos saturando o rate limit OAuth compartilhado
mesmo quando Claude Pro/Max estava disponível como fallback válido.

## Decisão

Introduzir trait `LlmBackendFactory` com 4 implementações explícitas e dispatcher controlado por
flag CLI global:

- `CodexFactory { binary: PathBuf, model: String }`
- `ClaudeFactory { binary: PathBuf, model: String }`
- `NullFactory` (skip embedding; persiste com `embedding = NULL`)
- `AutoFactory` (PATH probing: prefere codex, fallback claude, fallback null)

Flag global `--llm-backend <auto|claude|codex|none>` (env: `SQLITE_GRAPHRAG_LLM_BACKEND`,
default `auto`) seleciona a implementação por invocação. Default `auto` reproduz
comportamento da v1.0.81 (prefere codex → claude → null).

A factory trait desacopla configuração de instanciação e permite teste unitário de cada
implementação isoladamente (3 testes em `factory_tests`).

## Consequências

### Positivas
- Usuário com ChatGPT Pro OAuth pode forçar `codex` mesmo quando claude está no PATH
- Usuário com Claude Pro/Max pode preferir `claude` por menor latência
- CI sem qualquer CLI pode usar `--llm-backend=none` (embedding NULL em `pending_embeddings`)
- Default `auto` é 100% retrocompatível
- `with_default_codex()` permanece como helper de compatibilidade

### Negativas
- 4 implementações de factory exigem manutenção quando API evolui
- `AutoFactory` faz PATH probing no startup de cada invocação (latência ~5-10ms)
- Validação de modelo (ex: codex aceita apenas whitelist do ChatGPT Pro OAuth) ainda é
  responsabilidade do caller

## Alternativas Consideradas

1. **Env var `SQLITE_GRAPHRAG_LLM_BACKEND` apenas**: funcional mas não user-friendly em
   invocação one-shot — escolhida como CANAL SECUNDÁRIO ao lado da flag
2. **Auto-detect sem flag explícita**: usuário sem ChatGPT Pro mas com Claude Pro ficaria
   travado em codex — descartado

## Referências

- `gaps.md:413-670` — GAP-003 completo
- `src/cli.rs` (flag `--llm-backend`)
- `src/extract/llm_backend.rs` (trait + 4 implementações)
- `src/embedder.rs:embed_with_fallback` (usa factory trait)

## Decisões Relacionadas

- **ADR-0041 — Preservação de Credenciais de Provider Customizado (v1.0.83)**:
  complementa este ADR-0038 ao estender o whitelist env-clear para que
  `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL` e `OPENAI_BASE_URL`
  cheguem ao subprocesso codex ou claude quando o usuário escolhe
  `--llm-backend codex` ou `--llm-backend claude` apontando para
  provider customizado. Os dois ADRs compõem: ADR-0038 dá ao usuário
  o controle de qual backend usar; ADR-0041 garante que providers
  customizados funcionem quando o backend escolhido for roteado via
  env vars customizadas.
