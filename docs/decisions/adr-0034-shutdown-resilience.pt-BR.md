# ADR-0034 — Resiliência do SHUTDOWN Global para Auditorias e Testes

## Status

Aceito (v1.0.80).

## Contexto

A suite de auditoria da v1.0.80 (A1 até A4) descobriu que
`static SHUTDOWN: AtomicBool` em `src/lib.rs:48` é um vetor de
contaminação em workflows de Agent Teams. Quando invocações paralelas
de teammates registram seus callbacks `ctrlc::set_handler`, o flag
SHUTDOWN é setado em `true` no namespace compartilhado do processo.
Invocações subsequentes de `sqlite-graphrag remember` observam
`SHUTDOWN == true` no startup e abortam o embedding LLM com exit 11
("embedding cancelled by shutdown signal") antes de fazer qualquer
trabalho.

A memory 1262 (`incident-a1-bloqueada-shutdown-2026-06-14`) documenta
a falha reprodutível: a auditoria A1 não pôde ser persistida após
quatro tentativas consecutivas ao longo de minutos.

## Decisão

Implementar três mitigações em camadas em v1.0.80:

1. **`try_reset_shutdown()`** em `src/lib.rs` — `swap` atômico `AcqRel`
   do flag SHUTDOWN de volta para `false`, mais zerar os contadores
   `SIGNAL_COUNT` e `SIGNAL_NUMBER`. Retorna `true` se o flag estava
   setado. Documentado como uso exclusivo de tests e auditorias.
2. **`should_obey_shutdown()`** em `src/lib.rs` — lê a env var
   `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN` (`1`/`true`/`yes`/`on`,
   case-insensitive) e retorna `false` quando setada. Inverte a
   semântica do check de produção de "obedece a menos que informado
   o contrário" para "ignora a menos que informado o contrário".
3. **Bypass do embedder** em `src/embedder.rs:537` — o `tokio::select!`
   entre `work(batch)` e `token.cancelled()` é envolvido em
   `if should_obey_shutdown() { select! } else { work(batch).await }`.
   Em modo auditoria o braço de cancelamento é descartado, então o
   batch roda até a conclusão mesmo se o cancellation token estiver
   em estado cancelled.

## Consequências

Positivas:
- Auditorias e testes sucedem mesmo com SHUTDOWN contaminado.
- Memory 1261 (auditoria A1) e 1262 (incident) só persistiram graças
  a esta mitigação.
- API pública é type-safe e documentada com exemplos doctest.
- Zero overhead em produção: uma única `std::env::var` por chamada
  de `should_obey_shutdown()`, fora de hot path.

Negativas:
- O `tokio_util::sync::CancellationToken` global permanece one-shot;
  apenas o `AtomicBool` é resettável. Callers que precisam de token
  resettável devem usar token per-invocation.
- Código de produção NUNCA deve chamar `try_reset_shutdown()` — o
  bypass é opt-in via env var apenas.
- Tests precisam setar a env var em bloco
  `#[serial_test::serial(env)]` para evitar concorrência na leitura
  da env.

## Notas de Implementação

- `try_reset_shutdown` usa `SHUTDOWN.swap(false, Ordering::AcqRel)`
  para observe-and-reset atômico. O ordering `AcqRel` pareia com o
  `Release` store no signal handler e o `Acquire` load em
  `shutdown_requested`.
- `should_obey_shutdown` é `pub` e exposta junto ao `SHUTDOWN`
  static e à função `shutdown_requested` existentes.
- A mudança no embedder é mínima: um `if` arm em torno do
  `tokio::select!` existente. Sem novas tasks, sem novos tokens, sem
  novos channels.

## Workaround Documentado

Para pipelines que encontram a contaminação do SHUTDOWN em Agent
Teams:

```bash
PATH=tests/mock-llm:$PATH \
  SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1 \
  setsid -w timeout 60 \
  sqlite-graphrag remember --graph-stdin < payload.json
```

Três camadas independentes:
- `mock-llm` no PATH faz bypass do subprocesso LLM real que seria
  morto pelo SIGINT no mesmo process group.
- `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` faz bypass do check de
  cancelamento do parent no batch loop do embedder.
- `setsid -w` desanexa a CLI do process group da Bash tool para que
  o SIGINT não se propague para o child.

## Alternativas Consideradas

- **Substituir o `CancellationToken` global em `Mutex<Option<...>>`**:
  rejeitado porque a API pública `cancel_token()` retorna
  `&'static CancellationToken`, e substituí-lo não cancelaria futures
  já em voo no token antigo. O bypass via env var é mais barato e
  cirúrgico.
- **Apenas tokens per-invocation**: rejeitado porque exigiria mudar
  todos os call sites de `crate::cancel_token()`. O bypass mantém a
  API pública estável.
- **Documentar a contaminação como "comportamento esperado"**:
  rejeitado porque bloquearia toda auditoria futura e integration
  test de usar Agent Teams.

## Referências

- Memory 1261: `audit-a1-core-cli-lifecycle-2026-06-14` (auditoria A1, 8 gaps).
- Memory 1262: `incident-a1-bloqueada-shutdown-2026-06-14` (reprodutor).
- Memory 1265: `adr-0034-shutdown-resilience-2026-06-14` (este ADR em
  forma GraphRAG).
- `src/lib.rs:91-160` — `try_reset_shutdown`, `should_obey_shutdown`.
- `src/embedder.rs:537` — braço de cancelamento com bypass.
