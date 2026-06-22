# ADR-0050 — Remediação de Deadlock de Embedding

**Status**: Aceito
**Data**: 2026-06-21
**Contexto**: sqlite-graphrag v1.0.89 — GAP-RECALL-001, GAP-FLAGS-MORTAS, BUG-SKIP-EMBED, BUG-MODEL-VAZIO, BUG-SKIP-EMBED-INCOMPLETE

## Problema

Ambientes multi-sessão do Claude Code sofriam um deadlock
auto-perpetuante: `recall`, `hybrid-search` e `deep-research`
travavam indefinidamente no passo "Calculando embedding da
consulta...". Causas raiz:

1. O subprocesso LLM (`codex exec` / `claude -p`) travava ou morria
   sem liberar o semáforo host-wide de slots LLM.
2. Múltiplas sessões Claude Code spawnavam subprocessos de embedding
   que saturavam o pool de slots.
3. O timeout padrão de embedding era 300 segundos — muito longo para
   um embedding de query curta (RTT real de 2-3 segundos). Foi
   reduzido para 60 segundos.
4. `codex_embed_model()` e `claude_embed_model()` retornavam string
   vazia quando nenhuma env var estava definida, causando rejeição
   pelo codex com "The '' model is not supported".
5. `--skip-embedding-on-failure` era flag morta: aceita pelo clap,
   propagada para env var no `main.rs`, mas jamais lida por nenhum
   módulo de embedding.
6. Sete flags globais CLI eram aceitas pelo clap mas jamais propagadas
   para os módulos internos que as liam via `std::env::var`.

## Decisão

Aplicar sete correções em camadas na v1.0.89:

### FIX-1: `drop(stdin)` explícito antes de `wait_with_output`

A função `invoke_codex` em `src/extract/llm_embedding.rs` agora
chama `drop(stdin)` após `write_all` para fechar o file descriptor
de stdin do filho. `invoke_claude` não precisa disso: usa
`.stdin(Stdio::null())` e passa o prompt como argumento de
linha de comando.

### FIX-2: Timeout de embedding reduzido de 300s para 60s

`DEFAULT_EMBED_TIMEOUT_SECS` alterado de 300 para 60.
Adicionado `embed_timeout_for_batch(batch_size)` que escala:
base + 15s por item adicional (batch de 8 = 165s).

### FIX-3: Limpeza de slots obsoletos no startup

`llm_slots.rs` expõe `find_stale_slots()` que varre o diretório
de slots por lock files de PIDs que não existem mais. A limpeza
ativa no startup é `reaper::scan_and_kill_orphans()` (chamada do
`main.rs`), que coleta os slots obsoletos que ela identifica.

### FIX-4: Reaper mata processos sqlite-graphrag órfãos

`reaper.rs` expandido para varrer por processos `sqlite-graphrag`
órfãos (PPID=1, idade > 60s).

### FIX-5: Defaults de modelo sensatos

`codex_embed_model()` retorna `"gpt-5.5"` e `claude_embed_model()`
retorna `"claude-sonnet-4-6"` quando nenhuma env var está definida.
Anteriormente retornavam string vazia.

### FIX-6: `--skip-embedding-on-failure` conectada end-to-end

`should_skip_embedding_on_failure()` lê a env var. O comando
`remember` envolve os 3 pontos de embedding (passagem, chunks
paralelos, textos de entidade) com guards de erro que verificam
a flag. Tipo de embedding mudou de `Vec<f32>` para
`Option<Vec<f32>>` em `remember.rs`; `upsert_vec` condicionado
a `Some`.

### FIX-7: Propagação de flags CLI via `set_var`

Sete flags globais propagadas do struct CLI para env vars via
`std::env::set_var` em `main.rs` antes do dispatch de comandos.

## Alternativas Consideradas

### A. Retry global com backoff exponencial

Rejeitada: o subprocesso travado é a causa raiz. Retentar com
timeout maior apenas adiaria o deadlock.

### B. Embedding in-process (fastembed / ONNX)

Rejeitada: a decisão arquitetural v1.0.76 (ADR-0019) removeu
todos os modelos locais. Reverter adicionaria 30+ MB ao binário.

### C. Fallback para FTS5-only na exaustão de slots

Implementada em `deep-research` (GAP-DEEPRESEARCH-001). Não
aplicada a `recall`/`hybrid-search` porque esses comandos existem
para fornecer similaridade vetorial.

## Consequências

- `recall` e `hybrid-search` recuperam de subprocessos travados
  em 30s em vez de 300s
- Ambientes multi-sessão não acumulam mais processos órfãos
- `--skip-embedding-on-failure` funciona: `remember` retorna exit 0
  e persiste a memória sem embedding
- Defaults de modelo (`gpt-5.5`, `claude-sonnet-4-6`) eliminam o
  modo de falha "modelo vazio"
- Flags CLI funcionam via CLI ou env var

## Validação

- Build: 0 erros, 0 warnings do clippy
- Testes: 847 lib tests, 0 falhas
- E2E: 18 cenários end-to-end verificados contra binário release
