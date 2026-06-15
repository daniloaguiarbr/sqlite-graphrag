# ADR-0039: Semáforo Cross-Process para Spawn de Subprocessos LLM

- **Status**: Aceito
- **Data**: 2026-06-15
- **Versão**: v1.0.82 (resolve GAP-004)
- **Autores**: tech-lead

## Contexto

Quando N sessões Claude Code (ou agentes paralelos) rodam no mesmo host, cada invocação
`remember` / `edit` / `recall` / `hybrid-search` / `enrich` / `ingest` quer spawnar seu próprio
subprocesso `claude -p` ou `codex exec`. Sem coordenação cross-process, N subprocessos
saturam o rate limit OAuth compartilhado.

Transcript 2026-06-15 documentou 19+ `codex exec` simultâneos em `ps`, todos retornando exit
11 sistemático por rate limiting do ChatGPT Pro OAuth. Stub pattern degradado foi necessário
como workaround manual.

O semáforo CLI existente (`acquire_cli_slot` em `src/lock.rs:215-261`) cobre apenas
concorrência de invocações CLI, não de subprocessos LLM por invocação.

## Decisão

Introduzir `src/llm_slots.rs` com semáforo RAII cross-process usando `fs4::FileExt`:

- Slot files em `${XDG_RUNTIME_DIR:-~/.local/share}/sqlite-graphrag/llm-slots/slot-{0..N}.lock`
- `OpenOptions::create_new` + `try_lock_exclusive` para acquire atômico cross-process
- RAII guard `LlmSlotGuard` com `Drop` libera o slot automaticamente (inclusive em panic)
- `default_max_concurrency()` deriva N de nCPU + RSS disponível (4 GiB assumido em hosts
  desconhecidos) com clamp via `MAX_CONCURRENT_CLI_INSTANCES`
- Subcomando `slots { status | release | cleanup }` para inspeção e admin

A integração é feita em `embedder.rs:acquire_llm_slot_for_embedding()` que lê env vars
`SQLITE_GRAPHRAG_LLM_MAX_HOST_CONCURRENCY` e `SQLITE_GRAPHRAG_LLM_SLOT_WAIT_SECS` e adquire
um guard antes de cada spawn LLM em `embed_passage_local` e `embed_query_local`.

## Consequências

### Positivas
- Host-wide limit de subprocessos LLM simultâneos (default ~N CPU)
- Saturação OAuth rate limit prevenida
- Reaper de órfãos (`reaper::scan_and_kill_orphans`) detecta slots de PIDs mortos
- Drop automático em panic via RAII
- `slots status` permite observabilidade (`{max, active, pids}`)

### Negativas
- Polling de 100ms quando todos os slots ocupados adiciona latência
- Lock cross-process com `flock`/`LockFileEx` tem semântica ligeiramente diferente
  cross-platform
- Slots órfãos de PIDs stuck (mas vivos) não são liberados automaticamente

## Alternativas Consideradas

1. **Semáforo in-process com `tokio::Semaphore`**: não cobre múltiplas sessões CLI no
   mesmo host — descartado
2. **PID file único com flock global**: simples mas degrada para serialização total —
   descartado
3. **Cada sessão tem seu próprio binário subprocess**: dobra o footprint de RAM e ainda
   satura rate limit — descartado

## Referências

- `gaps.md:672-1110` — GAP-004 completo
- `src/llm_slots.rs` (semáforo RAII)
- `src/commands/slots.rs` (subcomando de inspeção)
- `src/embedder.rs:acquire_llm_slot_for_embedding` (integração)
- `src/reaper.rs:scan_and_kill_orphans` (cleanup de PIDs mortos)
