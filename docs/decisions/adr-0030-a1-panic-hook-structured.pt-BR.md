# ADR-0030 — Auditoria A1: Panic Hook Estruturado Substitui Dump Default no stderr (v1.0.80)

## Status

Aceito (v1.0.80, 2026-06-14).

## Contexto

A suíte de auditoria da v1.0.80 (ciclo de auditoria A1, escopo:
telemetria e observabilidade) identificou que o panic hook
default do Rust imprime o payload e a localização do panic no
stderr, que combinado com um evento `tracing::error!` produz
um trace duplo (um evento estruturado em JSON ou pretty, um
dump não-estruturado no stderr). Para agregadores de log
que parseam output JSON, o dump não-estruturado é
impossível de parsear e obscurece o evento estruturado. Para
leitores humanos em modo pretty, o trace duplo é
visualmente ruidoso.

## Decisão

O panic hook instalado em `src/telemetry.rs:47-72` (via
`std::panic::set_hook` durante init do tracing) emite um
único evento `tracing::error!` estruturado com o payload e
localização do panic, e DELIBERADAMENTE não chama o hook
anterior. O panic hook default do Rust é portanto substituído
pela vida do processo. Runs de teste ainda falham em panic
porque o Rust aborta o processo independentemente de qual
hook está instalado, então testes `#[should_panic]` existentes
e invariantes de `cargo test` não são afetados.

O hook lida com dois tipos de payload (`&str` e `String`),
cai para o marcador `<non-string panic>` para outros
payloads, e resolve `info.location()` para uma string
`file:line:column`. A localização é renderizada como
`unknown` quando não disponível (ex.: panics em builds
otimizados onde a localização é elidida).

## Consequências

Positivas:

- Agregadores de log que parseam output JSON veem
  exatamente um evento `tracing::error!` estruturado por
  panic, com os mesmos campos de payload e localização
  do dump não-estruturado anterior.
- Leitores humanos em modo pretty veem uma linha
  formatada por panic em vez de um trace dobrado.
- O hook é instalado durante init do tracing, então
  qualquer panic que ocorra ANTES do init do tracing
  ainda usa o hook default (aceitável: esses panics
  ocorrem na pequena janela de startup antes da
  observabilidade ser ativada).

Negativas:

- O panic hook default do Rust é substituído pela vida do
  processo; ferramentas que dependem do formato de
  output stderr do hook default (ex.: `--error-format=human`
  do `rustc`) veem o evento estruturado em vez do dump
  formatado humano. Isso é aceitável porque o projeto
  usa seu próprio formato de log e a CLI é a interface
  canônica.
- Panics de `cargo test` produzem um evento estruturado
  por panic no output de teste; pipelines de CI que
  fazem grep pelo padrão `thread 'foo' panicked at` do
  hook default devem atualizar suas regexes para o
  formato do evento estruturado.

## Referências

- `src/telemetry.rs:47-72` (implementação do panic hook)
- G28 (governança de ciclo de vida de processo da CLI)
- Ciclo de auditoria A1 (v1.0.80, escopo: telemetria)

