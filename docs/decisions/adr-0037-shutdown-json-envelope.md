# ADR-0037: Shutdown Signal Envelope JSON Determinístico

- **Status**: Aceito
- **Data**: 2026-06-15
- **Versão**: v1.0.82 (resolve GAP-002)
- **Autores**: tech-lead

## Contexto

Quando o comando `sqlite-graphrag` é cancelado por sinal externo (`timeout` do Bash, `Ctrl-C`,
`SIGTERM` de hook PreToolUse, OOM killer), o handler de shutdown emitem uma linha de texto
legível em **stderr** e chamava `std::process::exit(0)`. Consumidores que parseam `stdout` via
`jaq` recebiam 0 bytes e interpretavam como sucesso — o orquestrador (Claude Code) entrava em
loop "achou um novo arquivo?".

Em 2026-06-15 o transcript documentou 9 invocações `remember` consecutivas que retornaram exit 0
mas nunca persistiram a memória (subprocesso codex morto por sinal).

## Decisão

Padronizar o handler de shutdown para emitir **envelope JSON no stdout** antes de `exit`,
independente de qual Unix signal disparou o cancelamento:

```json
{
  "error": true,
  "code": 19,
  "message": "shutdown signal received; operation cancelled by SIGTERM",
  "signal": "SIGTERM",
  "graceful": true
}
```

- Exit code `19` (`SHUTDOWN_EXIT_CODE`) é determinístico e distinto de `128 + N` (que varia por
  signal). Foi escolhido por nunca colidir com exit codes legítimos do app (1, 2, 9, 10, 11,
  14, 15, 20, 75, 77).
- `signal` field é `"SIGINT" | "SIGTERM" | "SIGHUP" | "unknown"` para diagnóstico.
- `graceful: true` distingue shutdown solicitado de crash (que usa stderr-only).

## Comportamento Cross-Signal

- **SIGINT** via `ctrlc` crate (já presente)
- **SIGTERM** e **SIGHUP** via `signal-hook` crate (adicionado nesta release com feature
  `iterator` para abstração cross-platform)
- Ambos os handlers chamam `handle_first_signal("SIGINT", 2)` /
  `handle_first_signal("SIGTERM", 15)` / `handle_first_signal("SIGHUP", 1)` que deduplica via
  `AtomicBool`.

## Consequências

### Positivas
- Contrato JSON honrado em 100% dos caminhos de terminação
- Orquestrador detecta cancelamento e para loop imediatamente
- Exit code `19` é estável cross-OS e cross-signal
- `signal` field permite troubleshooting sem rerun

### Negativas
- Signal handlers agora fazem I/O em stdout — pode falhar em pipes quebrados
  (`BrokenPipe` é tratado como no-op gracioso)
- Requer `std::io::Write::flush` antes de `exit` para garantir entrega
- Limitação: SIGKILL e SIGSTOP não podem ser capturados (kernel-level, by design)

## Alternativas Consideradas

1. **Emitir só stderr (status quo)**: quebra contrato JSON documentado — descartado
2. **Exit `128 + signal` tradicional**: ambíguo quando signal não tem mapeamento claro —
   descartado
3. **Reescrever handler para cada subcommand separadamente**: duplicação massiva — descartado

## Referências

- `gaps.md:196-411` — GAP-002 completo
- `src/signals.rs` (handler cross-signal)
- `src/output.rs:emit_shutdown_envelope` (helper)
- `src/constants.rs:SHUTDOWN_EXIT_CODE = 19`
- `src/main.rs:392-403` (propagação do código 19 ao exit)
