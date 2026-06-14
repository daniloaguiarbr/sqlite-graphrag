# ADR-0035 — Auditoria A2: Observabilidade Estruturada para Backup e Health (v1.0.80)

## Status

Aceito (v1.0.80, 2026-06-14).

## Contexto

A suíte de auditoria da v1.0.80 (ciclo de auditoria A2, escopo:
observabilidade dos comandos de manutenção) verificou que
`commands/backup.rs` e `commands/health.rs` emitem seus
diagnósticos não-fatais através do subscriber estruturado
`tracing` em vez de chamadas diretas a `eprintln!` ou
`println!`. A auditoria A2 identificou que versões
anteriores desses comandos usavam `eprintln!` para
warnings não-fatais (fixes de permissão, notas de NTFS
DACL, diagnósticos de vec-table) que bypassavam o
subscriber global e produziam output impossíveis de
parsear para agregadores de log.

## Decisão

Os dois sítios de diagnóstico não-fatal em
`commands/backup.rs` e os quatro sítios de diagnóstico em
`commands/health.rs` foram auditados e verificados como
usando o subscriber estruturado
`tracing::{info, warn, error, debug}`. Cada emissão é
chaveada por `target = "<command>"` (ex.: `target: "backup"`,
`target: "health"`) e inclui os campos estruturados
relevantes (`path`, `error`, `integrity_ok`,
`vec_memories_ok`, `vec_entities_ok`, `vec_missing`,
`vec_orphaned`, `fts_ok`, `fts_query_ok`, `model_ok`).

As emissões específicas são:

- `commands/backup.rs:171` — `tracing::warn!` quando a
  chamada Unix mode 0o600 `set_permissions` falha após
  `temp.persist`. O warning carrega os campos `path` e
  `error` e usa `target: "backup"`. O arquivo de backup
  persistido permanece no lugar (o warning é
  informacional; o persist teve sucesso).
- `commands/backup.rs:181` — `tracing::debug!` no
  Windows notando que o step Unix mode 0o600 é pulado
  porque o default NTFS DACL já é private-to-user. A
  emissão em debug é o nível certo: é o comportamento
  esperado no Windows, não uma condição de warning.
- `commands/health.rs:209` — `tracing::info!` após o
  `PRAGMA integrity_check` rodar, carregando
  `integrity_ok` e o tempo decorrido. Este é o sinal
  primário para checagens de saúde de agregadores de
  log.
- `commands/health.rs:370` — `tracing::info!` após as
  checagens de vec-table completarem, carregando
  `vec_memories_ok`, `vec_entities_ok`, `vec_missing`,
  `vec_orphaned`. As duas contagens diagnósticas são
  exigidas pelo G66 (diagnóstico de desync de vec-table).
- `commands/health.rs:385` — `tracing::info!` após as
  checagens de FTS5 completarem, carregando `fts_ok` e
  `fts_query_ok`. O campo `fts_query_ok` é novo em
  v1.0.65 e indica que uma query FTS5 ao vivo teve
  sucesso (além da integridade de schema).
- `commands/health.rs:423` — `tracing::info!` após a
  checagem de disponibilidade da CLI LLM, carregando
  `model_ok`. Este é o sinal primário para a checagem
  de runtime do mandato OAuth-only.

## Consequências

Positivas:

- Todos os diagnósticos não-fatais dos comandos de
  manutenção fluem pelo subscriber global e são
  capturados pelo formato de log JSON
  (`SQLITE_GRAPHRAG_LOG_FORMAT=json`) para agregadores
  de log.
- As chaves `target: "<command>"` permitem filtros de
  log destacar diagnósticos de um comando específico
  sem fazer grep no texto da mensagem.
- Os campos estruturados (`integrity_ok`,
  `vec_memories_ok`, `fts_query_ok`, `model_ok`, etc.)
  são estáveis entre v1.x.y e formam um contrato de
  observabilidade documentado.

Negativas:

- Leitores humanos em modo pretty veem uma linha por
  emissão com campos estruturados; o formato é mais
  verboso do que uma única linha de `eprintln!`. Este
  é o trade-off por diagnósticos parseáveis por máquina.
- A emissão de `debug!` do Windows é silenciosa no
  nível de log default; operadores que precisam
  verificar o comportamento NTFS DACL devem definir
  `SQLITE_GRAPHRAG_LOG_LEVEL=debug`.

## Referências

- `src/commands/backup.rs:171` (warning Unix mode 0o600)
- `src/commands/backup.rs:181` (debug Windows DACL)
- `src/commands/health.rs:209` (PRAGMA integrity_check)
- `src/commands/health.rs:370` (checagens de vec-table)
- `src/commands/health.rs:385` (checagens de FTS5)
- `src/commands/health.rs:423` (disponibilidade da CLI
  LLM)
- Ciclo de auditoria A2 (v1.0.80, escopo: observabilidade
  de comandos de manutenção)

