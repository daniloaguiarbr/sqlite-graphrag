# ADR-0036: Persistência por Estágios com Checkpoint Retomável

- **Status**: Aceito
- **Data**: 2026-06-15
- **Versão**: v1.0.82 (resolve GAP-001)

## Contexto

O comando `remember` é executado como pipeline monolítico e all-or-nothing. O ciclo completo
— parse, validate, spawn de subprocesso LLM, geração de embedding, INSERT no SQLite — vive
em uma única transação sem checkpoints persistidos. Quando o subprocesso LLM é cancelado por
sinal externo (timeout do Bash tool, Ctrl-C, OOM killer, hook PreToolUse), o trabalho
validado é perdido.

Transcript de 2026-06-15 documentou 9 tentativas consecutivas com 9 variações de flags resultaram
em 9 falhas idênticas.

## Decisão

Implementar persistência por **estágios com checkpoint retomável** usando nova tabela
`pending_memories` (V014) e refatoração de `remember` em 3 sub-estágios:

- **Estágio A** (atômico, sem rede): parse + validate + INSERT em `pending_memories` com
  status `validated`
- **Estágio B** (rede): UPDATE para `embedding_in_progress`, spawn LLM, captura embedding,
  UPDATE para `embedding_done`
- **Estágio C** (atômico, sem rede): INSERT em `memories` + `memory_embeddings`, DELETE de
  `pending_memories`, UPDATE para `committed`

## Consequências

### Positivas
- Zero perda de trabalho em shutdown signal
- Retry idempotente sem re-validar Estágio A
- Economia de quota OAuth em retries parciais

### Negativas
- Complexidade adicional: 3 sub-estágios com transições explícitas
- Cleanup necessário: entradas órfãs devem ser abandonadas
