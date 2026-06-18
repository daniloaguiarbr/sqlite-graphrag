# ADR-0039: Semáforo Cross-Process para Spawn de Subprocessos LLM

- **Status**: Aceito
- **Data**: 2026-06-15
- **Versão**: v1.0.82 (resolve GAP-004)
- **Autores**: tech-lead

## Contexto

N sessões paralelas saturavam OAuth rate limit: cada `remember` / `edit` / `recall` /
`hybrid-search` / `enrich` / `ingest` spawnava subprocesso LLM sem coordenação host-wide.
Transcript 2026-06-15: 19+ `codex exec` simultâneos com exit 11 sistemático.

## Decisão

`src/llm_slots.rs` com semáforo RAII cross-process via `fs4::FileExt`:

- Slot files em `${XDG_RUNTIME_DIR:-~/.local/share}/sqlite-graphrag/llm-slots/slot-{0..N}.lock`
- `OpenOptions::create_new` + `try_lock_exclusive` para acquire atômico
- `LlmSlotGuard` com `Drop` libera slot em panic
- `default_max_concurrency()` deriva N de nCPU + RSS (4 GiB assumido) com clamp
- Subcomando `slots { status | release | cleanup }` para inspeção

Integração em `embedder.rs:acquire_llm_slot_for_embedding()` que lê
`SQLITE_GRAPHRAG_LLM_MAX_HOST_CONCURRENCY` e `SQLITE_GRAPHRAG_LLM_SLOT_WAIT_SECS`.

## Consequências

### Positivas
- Host-wide limit de subprocessos LLM simultâneos
- Saturação OAuth prevenida
- Reaper detecta slots de PIDs mortos
- Drop automático em panic via RAII
- `slots status` expõe `{max, active, pids}`

### Negativas
- Polling de 100ms adiciona latência quando saturado
- `flock` (Unix) e `LockFileEx` (Windows) têm semântica ligeiramente diferente
- Slots de PIDs stuck (mas vivos) não liberados automaticamente

## Referências

- `gaps.md:672-1110`
- `src/llm_slots.rs`
- `src/commands/slots.rs`
- `src/embedder.rs:acquire_llm_slot_for_embedding`
- `src/reaper.rs:scan_and_kill_orphans`
### Refinado por ADR-0043 (v1.0.85)

ADR-0043 (`docs/decisions/adr-0043-five-gap-remediation.pt-BR.md`) refinou `acquire_llm_slot_for_embedding` em `src/embedder.rs:260-277`. O timeout de 300s foi substituído por um teto de backoff de 750ms através de tentativas [50ms, 100ms, 200ms, 400ms]. A nova variante `FallbackReason::SlotExhausted` (uma das 7 do enum ADR-0043) carrega `reason_code: "slot_exhausted"` para o chamador, distinguindo contenção de slot de exaustão de quota OAuth e mismatch de backend. O circuit breaker permanece como limite superior após 3 falhas consecutivas.
