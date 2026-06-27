# ADR-0055 — Convergência dead-letter do `enrich` e concorrência REST de embedding OpenRouter

**Status**: Aceito
**Data**: 2026-06-27
**Contexto**: sqlite-graphrag v1.0.96 — GAP-ENRICH-BACKLOG-CONVERGE, GAP-OPENROUTER-REST-CONCURRENCY

## Problema

Dois gaps independentes surgiram depois que o transporte de chat OpenRouter
da v1.0.95 entrou (ADR-0054).

### GAP-ENRICH-BACKLOG-CONVERGE

O `enrich` conduz um pipeline SCAN→JUDGE→PERSIST apoiado na fila de
trabalho `.enrich-queue.sqlite`. Um item da fila que falhava — um rate
limit, um timeout, um 5xx, ou um erro duro de validação/parse vindo do
JUDGE — ficava no estado enfileirado sem status terminal e sem agenda de
retry. Toda execução seguinte re-escaneava os mesmos itens não
processáveis, então o backlog nunca encolhia comprovadamente até zerar.
Os operadores contornavam com um loop bash externo que reinvocava o
`enrich` até a contagem "parecer" estável, o que não é garantia de
convergência e disputa o singleton do enrich.

### GAP-OPENROUTER-REST-CONCURRENCY

O embedding via OpenRouter (`embed_passages_parallel_with_embedding_choice`,
`src/embedder.rs`) emitia uma chamada REST por lote de cada vez. Num
corpus multi-lote a rede ficava ociosa entre as idas e voltas: o lote N+1
só começava depois que o lote N retornava por completo. O nome dizia
"parallel" mas as chamadas HTTP por lote eram seriais, deixando a maior
parte do tempo de parede para a latência de ida e volta em vez de
throughput.

## Decisão

### Convergência dead-letter (GAP-ENRICH-BACKLOG-CONVERGE)

Dar à fila do enrich uma disciplina de dead-letter para que o conjunto
vivo encolha estritamente.

- O schema da `.enrich-queue.sqlite` ganha duas colunas via `ALTER TABLE`
  idempotente (`error_class`, `next_retry_at`) e um novo status terminal
  `dead`. O ALTER é idempotente, então filas existentes são atualizadas no
  lugar sem passo de migração.
- As falhas por item são classificadas reusando `AttemptOutcome` e
  `compute_delay` de `src/retry.rs` — a mesma política de backoff que o
  resto do código já usa, sem lógica de retry nova. Transient (rate-limit
  / timeout / 5xx) define `next_retry_at` como agora + backoff;
  HardFailure (validação / parse) é terminal imediatamente.
- Um item vira `dead` após `--max-attempts` (padrão 5) retentativas
  Transient, ou na primeira HardFailure. O dequeue é alterado para honrar
  `next_retry_at` (pula itens ainda não vencidos) e excluir `dead`. O
  conjunto vivo então encolhe monotonicamente: cada passada ou persiste um
  item ou o move em direção a `dead`, e itens `dead` nunca reentram.
- Novos flags: `--until-empty` roda um loop interno scan→drain até a
  convergência (substituindo o loop bash externo), `--max-runtime <SECS>`
  é um teto de tempo de parede que encerra o loop de forma limpa,
  `--max-attempts <N>` é o orçamento de retentativas Transient, e
  `--status` é um relatório read-only de contagens de backlog/fila/dead
  que não chama o LLM nem adquire o singleton do enrich.

### Fan-out REST bounded (GAP-OPENROUTER-REST-CONCURRENCY)

Fazer o embedding OpenRouter sobrepor suas idas e voltas sem dependência
nova nem nova superfície de falha.

- `embed_passages_parallel_with_embedding_choice` agora faz fan-out das
  chamadas REST por lote com um `tokio::task::JoinSet` bounded. Os
  resultados são remontados pelo índice de chunk, então a ordem de saída é
  idêntica à do caminho serial — os callers não veem mudança de ordenação.
- As requisições em voo sofrem clamp para `1..16`, a faixa segura para o
  Cloudflare observada no endpoint REST do OpenRouter. O `enrich` ganha
  `--rest-concurrency` (padrão 8 para `--mode openrouter`, clamp `1..16`).
- O `tokio::task::JoinSet` já está disponível (tokio é dependência atual);
  nenhum crate é adicionado.

## Alternativas Consideradas / Desvios Deliberados

### A. Converter o thread-pool do enrich em tarefas tokio

Não feito (deliberado). O pool de workers do enrich permanece um
thread-pool. O ganho de concorrência para embedding está em sobrepor as
idas e voltas de rede, que o JoinSet bounded entrega localmente dentro da
chamada de embedding; reescrever a orquestração do enrich sobre tarefas
tokio seria uma mudança grande e ortogonal sem throughput adicional,
porque o ponto real de serialização é o single writer do SQLite, não o
modelo de workers.

### B. Adicionar uma tarefa writer mpsc para serializar escritas no banco

Não feito (deliberado). As escritas já são seriais via WAL mais um claim
atômico, então um writer mpsc dedicado adicionaria um canal e uma tarefa
sem remover contenção alguma — a invariante de single-writer já é imposta
na camada do SQLite.

### C. Remover os guardrails de subprocesso agora que o OpenRouter é REST

Não feito (deliberado). Os guardrails de preflight/spawn são preservados
porque ainda protegem os modos `claude-code` / `codex` / `opencode`; o
`--mode openrouter` simplesmente não os exercita. Removê-los regrediria os
três transportes de CLI sem benefício para o caminho REST.

### D. Fan-out ilimitado para throughput máximo de embedding

Rejeitado. Concorrência ilimitada contra o endpoint REST do OpenRouter
dispara o rate limiting do Cloudflare; o clamp `1..16` é a faixa segura de
operação, e 8 é um default conservador.

### E. Uma implementação de retry/backoff separada para a fila

Rejeitado (DRY). `AttemptOutcome` e `compute_delay` em `src/retry.rs` já
codificam a classificação Transient-vs-HardFailure e o backoff
exponencial; a fila os reusa literalmente em vez de bifurcar uma política
paralela.

## Consequências

- Positivo: o backlog do enrich converge comprovadamente — `--until-empty`
  o leva a um conjunto vivo vazio numa única invocação, com `--max-runtime`
  como teto de segurança e `--status` para observabilidade read-only que
  nunca toca o LLM nem o singleton.
- Positivo: itens permanentemente não processáveis caem em `dead` em vez
  de serem retentados para sempre, e falhas transientes fazem backoff numa
  agenda em vez de hot-loop.
- Positivo: o embedding OpenRouter sobrepõe suas idas e voltas REST,
  cortando o tempo de parede em corpora multi-lote, com ordem preservada e
  o clamp `1..16` seguro para o Cloudflare.
- Neutro: o thread-pool do enrich, o writer serializado por WAL e os
  guardrails de subprocesso ficam intencionalmente inalterados (ver
  Desvios A–C).
- Negativo: um item `dead` exige inspeção do operador (via `--status`)
  para diagnóstico; não é auto-ressuscitado. Esse é o trade-off pretendido
  — convergência em vez de retry indefinido.

## Validação

- Resultados de build/clippy/test a serem confirmados pela verificação do lead (Fase 7).

## Referências Cruzadas

- `gaps.md` — GAP-ENRICH-BACKLOG-CONVERGE e GAP-OPENROUTER-REST-CONCURRENCY marcados RESOLVIDO em v1.0.96
- ADR-0054 (transporte de chat OpenRouter para `enrich`) — a mudança da v1.0.95 sobre a qual esta se apoia
- ADR-0053 (remediação de quatro gaps da v1.0.94) — tornou `enrich --mode` obrigatório
- `src/commands/enrich.rs` (`--until-empty`, `--max-runtime`, `--max-attempts`, `--status`, `--rest-concurrency`, dequeue dead-letter), `src/embedder.rs` (fan-out JoinSet em `embed_passages_parallel_with_embedding_choice`), `src/retry.rs` (`AttemptOutcome`, `compute_delay` reusados pela fila), `.enrich-queue.sqlite` (`error_class`, `next_retry_at`, status `dead`)
