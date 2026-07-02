# ADR-0060: v1.1.0 — Convergência do Backlog de Enrichment na Raiz (GAP-SG-70..78)

- **Status**: Aceito
- **Data**: 2026-07-01
- **Versão**: v1.1.0 (fecha GAP-SG-70, GAP-SG-71, GAP-SG-72, GAP-SG-73, GAP-SG-74, GAP-SG-75, GAP-SG-76, GAP-SG-77, GAP-SG-78)

## Contexto

Depois das v1.0.96/97/99, a superfície de dead-letter e observabilidade do
enrichment ainda tinha nove arestas afiadas que deixavam um backlog saudável se
disfarçar de cemitério de dead-letter ou de falsa "fila vazia". Completions truncadas
do OpenRouter eram re-emitidas com o *mesmo* `max_tokens`, então cada retry truncava
de forma idêntica e acabava indo para dead-letter — um laço auto-reforçado que o
operador não conseguia quebrar. A classificação de retry casava por substring da
mensagem de erro, então um esgotamento de retries internos ("max retries exceeded")
era rotulado como permanente e caía direto no dead-letter em vez de tomar o backoff
da fila. O laço de dequeue colapsava `SQLITE_BUSY` num falso backlog vazio via
`.ok()`, sub-processando silenciosamente sob contenção de lock. O `enrich --status`
lia apenas o sidecar de memory-bindings, reportando um falso `pending=0` para
`entity-descriptions`, `body-enrich` e `re-embed`. E uma entidade transitória, ainda
não materializada, ia para o dead-letter logo na primeira falha. Cada um desses
casos transformava uma condição recuperável em terminal, e o estado reportado não
podia ser confiado como sinal de convergência.

## Decisão

1. **Retry de completions truncadas com orçamento crescido (GAP-SG-70).** O
   `chat_api` desserializa `choices[].finish_reason`; em `"length"` ele re-emite a
   requisição com um `max_tokens` crescido — limitado por `ENRICH_MAX_LENGTH_RETRIES`
   — *antes* de tentar o reparo de JSON, quebrando o laço em que um retry reusava o
   mesmo orçamento e truncava de forma idêntica.

2. **Constantes adaptativas de `max_tokens` (GAP-SG-71).** Constantes nomeadas
   (`ENRICH_INITIAL_MAX_TOKENS`, `ENRICH_MAX_TOKENS_GROWTH_FACTOR`,
   `ENRICH_MAX_TOKENS_CEILING`, `ENRICH_MAX_LENGTH_RETRIES`) dimensionam o orçamento
   inicial e o seu crescimento por retry, substituindo o default ilimitado anterior
   do provedor.

3. **Colunas de diagnóstico do dead-letter (GAP-SG-72).** A fila sidecar do enrich
   ganha as colunas `finish_reason`, `input_tokens`, `output_tokens` via um `ALTER`
   idempotente; o `complete()` retorna um `ChatCompletion`/`ChatError` carregando
   esses valores, e o `--list-dead --json` os expõe para o operador ver *por que* um
   item morreu.

4. **Classificação de retry tipada, nunca por substring (GAP-SG-73).** O
   `classify_enrich_outcome` decide puramente pela variante de `AppError`; falhas do
   OpenRouter carregam um `retry_class` computado na origem (status HTTP exato /
   código estruturado do provedor). A correção-chave do falso-permanente: uma falha
   de esgotamento de retries internos ("max retries exceeded") agora é `Transient`
   (elegível ao backoff `--max-attempts` da fila) em vez de dead-letter imediato.

5. **Módulo compartilhado `openrouter_http` (GAP-SG-74, DRY).** O `ApiError`,
   `code_string`, `MAX_RETRIES` e `backoff` duplicados entre os clientes de chat e de
   embedding são extraídos para um novo módulo `openrouter_http`, que também hospeda
   os classificadores `status_retry_class` / `provider_error_retry_class` — uma única
   fonte de verdade para a semântica de retry entre chat e embedding.

6. **User-Agent carimbado com versão (GAP-SG-75).** O `User-Agent` HTTP do OpenRouter
   é atualizado para `sqlite-graphrag/1.1.0` (estava defasado em 1.0.95/1.0.96).

7. **Dequeue limitado sob contenção (GAP-SG-76).** O `open_queue_db` define
   `busy_timeout`, e o dequeue reusa o `with_busy_retry` limitado (backoff exponencial
   com teto + jitter, ciente do kill-switch), falhando alto com exit 15 sob contenção
   sustentada em vez de colapsar `SQLITE_BUSY` num falso "backlog vazio" via `.ok()`.

8. **`scan_backlog` real por operação no `--status` (GAP-SG-77).** O `enrich
   --status` reporta um `scan_backlog` real por operação — os candidatos do banco que
   um scan de fato enfileiraria — em vez de apenas o `unbound_backlog` de
   memory-bindings, eliminando o falso `pending=0` para `entity-descriptions`,
   `body-enrich` e `re-embed`. Um novo `count_operation_backlog` (só contagem)
   compartilha os predicados WHERE exatos com os scanners, então o backlog reportado
   nunca pode divergir de um scan real; o campo `state` deriva seu veredito
   `pending-scan` do `scan_backlog` da operação atual.

9. **Entidade transitória ainda não materializada (GAP-SG-78).** Uma entidade ainda
   não materializada é classificada como `Transient` (retentada, não enviada ao
   dead-letter na primeira falha) via um `AppError::EntityNotYetMaterialized { name,
   namespace }` tipado (`exit_code` 4, `is_retryable` true), substituindo o `NotFound`
   por string nos dois call sites de entidade (`entity-descriptions`,
   `entity-type-validate`); o lookup cego a namespace em `call_entity_type_validate`
   (que ignorava `_namespace` e casava só por `name`) é corrigido para
   `WHERE namespace = ?1 AND name = ?2`.

## Alternativas Consideradas

- **Manter a classificação de retry por substring.** Rejeitada: frágil, dependente da
  redação da mensagem do provedor, e viola a regra do projeto de retry tipado com
  backoff. Qualquer mudança de texto do provedor re-quebraria a classificação em
  silêncio.
- **Trocar o modelo LLM de enrichment.** Rejeitada por política:
  `deepseek/deepseek-v4-flash:nitro` é o modelo de enrichment fixo; o laço de
  truncamento é um defeito de orçamento/retry, não do modelo, e é corrigido na camada
  de requisição.
- **Migração de schema para as colunas de diagnóstico.** Desnecessária: o `ALTER`
  idempotente mantém o schema na versão 15, então não há passo de migração nem quebra
  de compatibilidade.

## Consequências

### Positivas

- O dead-letter fica confiável: condições recuperáveis (truncamento, esgotamento de
  retries internos, falha transitória de entidade) tomam o caminho do backoff, então
  um `queue_dead == 0` convergido é de fato alcançável.
- O `enrich --status` vira a fonte de verdade: o `scan_backlog` por operação casa com
  o que um scan real enfileiraria, então `pending=0` não mente mais para
  `entity-descriptions`, `body-enrich` ou `re-embed`.
- Sem mais laço de truncamento: uma completion `"length"` cresce seu orçamento e ou
  sucede ou termina em `ENRICH_MAX_LENGTH_RETRIES`, nunca re-truncando de forma
  idêntica.
- DRY: os clientes de chat e embedding compartilham um único módulo
  `openrouter_http` de retry/classificação, então a semântica de retry não pode mais
  divergir entre os dois caminhos.
- O operador consegue diagnosticar mortes via `--list-dead --json` (`finish_reason`,
  `input_tokens`, `output_tokens`) em vez de adivinhar.

### Negativas / Notas

- Itens **já** marcados como `dead` em bancos reais antes da v1.1.0 permanecem
  mortos; a nova classificação só governa resultados futuros. A recuperação
  operacional é via `--requeue-dead` (opcionalmente `--ignore-backoff`) e
  `--prune-dead-orphans`.
- Uma entidade que **nunca** materializa não é retentada para sempre: ela é encerrada
  por `--max-attempts`, então uma entidade genuinamente ausente ainda chega ao
  dead-letter após o backoff limitado — a correção só evita o dead-letter na
  *primeira* falha transitória.
- O schema permanece v15; as colunas de diagnóstico chegam via `ALTER` idempotente,
  então um binário antigo lendo um sidecar mais novo simplesmente ignora as colunas
  extras.

## Referências Cruzadas

- ADR-0054 — enrichment de chat via OpenRouter.
- ADR-0055 — dead-letter do enrich + concorrência REST.
- ADR-0057 — fila sidecar do enrich.
- ADR-0058 — `prune-dead-orphans`.
- ADR-0059 — v1.0.99 (remoção do degree-cap, convergência de doc).
- CHANGELOG.md — seção 1.1.0.
- gaps.md — GAP-SG-70..78.
