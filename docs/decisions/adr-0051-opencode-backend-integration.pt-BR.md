# ADR-0051: Integração do Backend OpenCode (v1.0.90)

## Status
- Aceito (2026-06-22)

## Contexto
- A CLI sqlite-graphrag suportava apenas `codex` e `claude` como backends LLM
- `src/spawn/opencode_adapter.rs` existia desde a v1.0.75 (G22) mas nunca foi conectado aos pipelines de embedding, ingest ou enrich
- O padrão factory em `llm_backend.rs` foi desenhado para extensibilidade mas só tinha implementações Codex/Claude/None
- A CLI OpenCode v1.17.7 fornece modelos gratuitos (deepseek-v4-flash-free, mimo-v2.5-free, nemotron-3-ultra-free, north-mini-code-free, big-pickle)

## Decisão
- Adicionar OpenCode como terceiro backend LLM nos 3 pipelines: embedding, ingest, enrich
- Prioridade de auto-detecção: codex (1º) > claude (2º) > opencode (3º) > none (4º)
- Zero hardcode: caminho do binário e modelo resolvidos via env var ou flag CLI
- Sem enforcement OAuth para opencode (usa o próprio sistema de auth)

## Interface
- Comando: `opencode run --format json -m <provider/model> --dangerously-skip-permissions "<prompt>"`
- Saída: NDJSON com 3 tipos de evento (step_start, text, step_finish)
- Texto da resposta em `.part.text` dos eventos `type=="text"`
- Sem equivalente a `--output-schema`: saída estruturada via prompt + parsing JSON

## Consequências
- 6 enums expandidos com a variante `Opencode` (EmbeddingFlavour, LlmBackendKind, LlmBackendKindFactory, LlmBackendChoice, IngestMode, EnrichMode)
- 4 arquivos novos criados (opencode_runner.rs, ingest_opencode.rs, mock-opencode, este ADR)
- 12 arquivos existentes modificados
- 874 testes passando (de 854)
- Cadeia de fallback estendida: `[Codex, Claude, Opencode, None]`

## Limitações
- OpenCode não tem flag de saída estruturada (--output-schema / --json-schema)
- O enforcement de JSON depende de prompt de definição de papel ("You are an embedding function") + parsing robusto (Estratégia 3 em parse_llm_json)
- O parser extrai JSON de fences markdown, brace-matching e parse direto como estratégias de fallback
- O prompt é passado como argumento posicional (limite argv ~128KB no Linux)

## Correções de Auditoria da v1.0.90
- Prompt de embedding reescrito: a definição de papel ("You are an embedding function") produz vetores reais de 64 dims; o prompt genérico anterior fazia os modelos recusarem
- Contaminação cruzada de modelo: `opencode_embed_model()` e `resolve_opencode_model()` NÃO caem para `SQLITE_GRAPHRAG_LLM_MODEL` — essa var pode conter modelos codex/claude (ex.: "gpt-5.4-mini") que o opencode não resolve (ProviderModelNotFoundError)
- Propagação de env: `propagate_opencode_env()` encaminha OPENCODE_*, OPENROUTER_*, XDG_*, LANG, TERM, USER, LOGNAME, TMPDIR para o subprocesso após env_clear()
- Pipeline de ingest: `run_opencode_ingest()` agora executa o loop completo de extração por arquivo com persistência de entidades/relações (era um stub retornando Err)

## Variáveis de Ambiente
- `SQLITE_GRAPHRAG_OPENCODE_BINARY` — override do caminho do binário
- `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL` — modelo de embedding
- `SQLITE_GRAPHRAG_OPENCODE_MODEL` — modelo de extração/enriquecimento
- `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT` — timeout em segundos (padrão: 300)

## Flags CLI
- `--opencode-binary <PATH>` (global)
- `--llm-backend opencode` (global)
- `--mode opencode` (ingest, enrich)
- `--opencode-model <MODEL>` (ingest, enrich)
- `--opencode-timeout <SECONDS>` (ingest, enrich)
