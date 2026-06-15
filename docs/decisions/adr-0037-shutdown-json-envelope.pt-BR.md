# ADR-0037: Envelope JSON de Shutdown Determinístico

- **Status**: Aceito
- **Data**: 2026-06-15
- **Versão**: v1.0.82 (resolve GAP-002)
- **Autores**: tech-lead

## Contexto

Quando `sqlite-graphrag` recebia sinal externo (`timeout`, `Ctrl-C`, `SIGTERM`), o handler
emitia texto legível em stderr e chamava `std::process::exit(0)`. Consumidores que parseiam
stdout via `jaq` recebiam 0 bytes e interpretavam como sucesso — orquestrador entrava em
loop. Transcript 2026-06-15: 9 invocações `remember` retornaram exit 0 sem persistir nada.

## Decisão

Handler de shutdown emite envelope JSON no stdout antes de exit, com exit code determinístico
`19` (`SHUTDOWN_EXIT_CODE`):

```json
{
  "error": true,
  "code": 19,
  "message": "shutdown signal received; operation cancelled by SIGTERM",
  "signal": "SIGTERM",
  "graceful": true
}
```

- Exit code 19 distinto de `128+N` (que varia por signal) e nunca colide com exit codes
  legítimos do app (1, 2, 9, 10, 11, 14, 15, 20, 75, 77)
- `signal` field: SIGINT/SIGTERM/SIGHUP/unknown para diagnóstico
- `graceful: true` distingue shutdown solicitado de crash (que usa stderr-only)

Cross-signal via `signal-hook` crate (feature `iterator`) com dedup via `AtomicBool`.

## Consequências

### Positivas
- Contrato JSON honrado em 100% dos caminhos de terminação
- Orquestrador detecta cancelamento e para loop
- Exit code estável cross-OS e cross-signal

### Negativas
- I/O em stdout pode falhar em pipes quebrados (BrokenPipe tratado gracioso)
- Requer `Write::flush` antes de `exit`
- SIGKILL/SIGSTOP não capturáveis (kernel-level, by design)

## Referências

- `gaps.md:196-411`
- `src/signals.rs`
- `src/output.rs:emit_shutdown_envelope`
- `src/constants.rs:SHUTDOWN_EXIT_CODE = 19`
- `src/main.rs:392-403`
