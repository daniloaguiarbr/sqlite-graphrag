# ADR-0036: Persistência por Estágios com Checkpoint Retomável

- **Status**: Aceito
- **Data**: 2026-06-15
- **Versão**: v1.0.82 (resolve GAP-001)
- **Autores**: tech-lead

## Contexto

O comando `remember` é executado como pipeline monolítico e all-or-nothing. O ciclo completo
— parse, validate, spawn de subprocesso LLM, geração de embedding, INSERT no SQLite — vive
em uma única transação sem checkpoints persistidos. Quando o subprocesso LLM é cancelado por
sinal externo (timeout do Bash tool, Ctrl-C, OOM killer, hook PreToolUse, parent death, SIGPIPE),
o trabalho validado é perdido.

Transcript de 2026-06-15 documentou 9 tentativas consecutivas com 9 variações de flags resultaram
em 9 falhas idênticas (`code: 11 — embedding cancelled by shutdown signal`).

## Decisão

Implementar persistência por **estágios com checkpoint retomável** usando nova tabela
`pending_memories` (V014) e refatoração de `remember` em 3 sub-estágios:

- **Estágio A** (atômico, sem rede): parse + validate + INSERT em `pending_memories` com
  status `validated`. Retorna `pending_id`.
- **Estágio B** (rede): UPDATE para `embedding_in_progress`, spawn LLM, captura embedding,
  UPDATE para `embedding_done`. Incrementa `attempt_count` em cada tentativa.
- **Estágio C** (atômico, sem rede): INSERT em `memories` + `memory_embeddings`, DELETE de
  `pending_memories`, UPDATE para `committed`.

## Comandos

- `remember --stage-only` — executa só Estágio A, retorna `pending_id`
- `remember --resume <pending_id>` — retoma de Estágio B
- `remember --skip-embedding` — executa A e C pulando B; embedding NULL
- `pending list/resume/cleanup` — subcomando novo para gerenciar pendings

## Consequências

### Positivas
- Zero perda de trabalho em shutdown signal
- Retry idempotente sem re-validar Estágio A
- Economia de quota OAuth em retries parciais
- Compatibilidade retroativa: comportamento default idêntico ao atual

### Negativas
- Complexidade adicional: 3 sub-estágios com transições explícitas
- Cleanup necessário: entradas `embedding_in_progress` órfãs devem ser abandonadas
- Latência de commit aumenta ligeiramente (transação extra)
- Risco: a Estágio C (atômica) ainda pode falhar se SQLite for corrompido

## Alternativas Consideradas

1. **Checkpoint por linha de log append-only**: mais simples, mas impossível de retomar
   parcialmente — descartado
2. **Saga pattern com compensação**: complexo, sem benefício claro sobre Estágios A/B/C —
   descartado
3. **Usar `sqlite-vec` para embedding (removido v1.0.76)**: fora de escopo — descartado

## Referências

- `gaps.md:18-194` — GAP-001 completo
- `migrations/V014__pending_memories.sql`
- `src/storage/pending_memories.rs` (DAO)
- `src/llm_slots.rs` (GAP-004) — relacionado, controla concorrência
