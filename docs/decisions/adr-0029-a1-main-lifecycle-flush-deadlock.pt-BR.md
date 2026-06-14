# ADR-0029 — Auditoria A1: Thread Main Síncrona, Flush Explícito e Watchdog de Deadlock (v1.0.80)

## Status

Aceito (v1.0.80, 2026-06-14).

## Contexto

A suíte de auditoria da v1.0.80 (ciclo de auditoria A1, escopo:
ciclo de vida e threading do core da CLI) identificou três
riscos interativos no entry point `main` e no watchdog de
detecção de deadlock:

- **A1/G1 — Assunção implícita de async**: o código tinha
  herdado um runtime tokio de pré-v1.0.76. Com a refatoração
  one-shot LLM-only da v1.0.76 o runtime não era mais
  necessário, mas a assunção persistia em comentários de
  código e influenciava como sinais de shutdown e tokens de
  cancelamento eram cabeados. A auditoria verificou que a
  thread main é intencionalmente 100% síncrona: todo
  `remember`, `ingest` e `enrich` spawna um subprocesso
  headless `claude` ou `codex` via `std::process::Command`
  e espera seu exit. O teto de concorrência por subprocesso
  é imposto pelo semáforo contador `acquire_cli_slot` e
  pelas constantes `MAX_CONCURRENT_CLI_*`; sync
  cross-process acontece via WAL do SQLite e `flock`. O
  design pré-tokio é uma escolha de política deliberada:
  zero contexto de runtime async para cancelar, zero braços
  de `tokio::select!` para pular, e zero `JoinSet` para
  drenar em shutdown (veja ADR-0034 para o SHUTDOWN global
  e o bypass de modo auditoria). Tocar este entry point
  exige revisitar a política de cancelamento por subprocesso,
  não apenas adicionar um runtime.
- **A1/G6 — Perda de linhas parciais em exit por sinal**:
  `std::process::ExitCode` é um wrapper transparente ao redor
  de um `u8` retornado de main; no exit do processo, o C
  runtime faz flush dos seus próprios buffers stdio mas NÃO
  sabe do `BufWriter` interno do Rust envolvendo
  stdout/stderr. Sem o flush explícito, a última linha
  parcial de output JSON (notavelmente de
  `output::emit_json_compact` e `emit_progress`) pode ser
  perdida quando o processo é morto por sinal ou sai com
  código de erro. Esta é uma política defensiva deliberada:
  fazer flush de todo caminho de erro E do caminho de
  sucesso antes de retornar.
- **A1/G7 — Watchdog de detecção de deadlock**: a thread
  de detecção de deadlock é intencionalmente
  process-scoped (não tem sinal de shutdown). É um
  watchdog: pole a cada 10 segundos e reporta quaisquer
  deadlocks via tracing, então dorme de novo. Quando o
  processo sai (via retorno de `std::process::ExitCode` ou
  sinal), o kernel destrói todas as threads; não há leak
  porque a thread nunca é joined ou detached no sentido
  Rust. O intervalo de poll de 10 segundos é um equilíbrio:
  curto o bastante para pegar deadlocks antes de qualquer
  timeout de usuário, longo o bastante para manter o
  overhead do watchdog desprezível.

## Decisão

Os três achados são registrados em `src/main.rs` como
comentários inline de documentação no topo de `fn main`.
NÃO são código novo: documentam o comportamento existente
da v1.0.80 que a auditoria verificou. Os comentários servem
como a explicação canônica para mantenedores futuros e para
os repositórios de audit-trail.

Cada comentário é um bloco de 5-10 linhas:

- Comentário `A1/G1` em `src/main.rs:39-49` documenta o
  design síncrono da thread main e seu relacionamento com o
  teto de concorrência por subprocesso e o bypass de
  SHUTDOWN do modo auditoria.
- Comentário `A1/G6` em `src/main.rs:29-38` documenta o
  contrato de flush explícito e a patologia de
  linha-parcial-perdida que o motiva.
- Comentário `A1/G7` em `src/main.rs:119-127` documenta o
  design do watchdog e a justificativa para o poll de
  10 segundos.

## Consequências

Positivas:

- O audit-trail é preservado em código-fonte como
  comentários inline, tornando a justificativa disponível
  a todo mantenedor que lê o arquivo (não precisa consultar
  este ADR para entender o código existente).
- Refatorações futuras que toquem o entry point main têm
  orientação explícita: qualquer mudança deve revisitar a
  política de cancelamento por subprocesso, não apenas o
  código circundante.
- O contrato de flush é a fonte única de verdade para
  "como é o exit"; futuros caminhos de erro devem segui-lo.

Negativas:

- Os comentários inline são ~25 linhas de prosa de
  código-fonte que devem ser mantidos em sync com o
  comportamento de runtime; se uma refatoração mudar o
  comportamento, os comentários devem ser atualizados.
- O watchdog de detecção de deadlock é um custo de
  runtime (poll de 10 segundos, `tracing::warn!` por
  deadlock); é off por padrão e só ativado via flag
  `deadlock-detection`.

## Referências

- `src/main.rs:29-38` (A1/G6 contrato de flush)
- `src/main.rs:39-49` (A1/G1 thread main síncrona)
- `src/main.rs:119-127` (A1/G7 watchdog de deadlock)
- `Cargo.toml:230` (feature Cargo `deadlock-detection`)
- ADR-0034 (SHUTDOWN global e bypass de modo auditoria)
- G28 (governança de ciclo de vida de processo da CLI)
- G30 (singleton cross-process via `flock`)

