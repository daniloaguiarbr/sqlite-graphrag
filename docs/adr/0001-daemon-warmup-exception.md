# ADR-0001: Daemon Warmup Exception to "No Persistent Daemons" Rule

**Status:** Accepted
**Date:** 2026-05-03
**Deciders:** Project owner
**Consulted:** Auditoria v1.0.42 (audit-empirical, audit-code teammates)

## Context

Two project rules forbid persistent daemons:
- `docs_rules/rules_rust_cli_stdin_stdout.md:96` — "PROIBIDO criar daemons persistentes que mantenham estado entre chamadas"
- `docs_rules/rules_rust_proibicao_de_MCPs.md:62` — same wording

However, `sqlite-graphrag` ships a `daemon` subcommand that:
- Loads BERT NER model + ONNX embedder once into memory (~3 GiB RSS)
- Stays alive between CLI invocations to amortize the ~17s cold-start cost
- Communicates with one-shot CLI clients via Unix domain socket / Windows named pipe

This is a direct conflict with the rule. Without the daemon, every `remember`, `ingest`, `recall`, or `hybrid-search` invocation would pay 17s of model loading, making interactive UX unusable.

## Decision

We grant an **authorized exception** to the no-daemons rule for the warmup daemon, with the following constraints:

1. **Single-purpose:** the daemon ONLY accelerates ML model warmup; it does NOT cache business data, sessions, or state that would compromise correctness if the daemon is killed mid-flight.
2. **Stateless from a correctness standpoint:** killing the daemon at any moment must NOT cause data loss, corruption, or inconsistent state. All persistence happens in the SQLite file via the calling CLI.
3. **Idle timeout:** daemon self-terminates after `DAEMON_IDLE_SHUTDOWN_SECS` (default 600) of inactivity.
4. **Explicit shutdown:** `sqlite-graphrag daemon stop` provides graceful termination; SIGTERM also works.
5. **Auto-spawn discipline:** spawn rate-limited via `daemon-spawn-state.json` exponential backoff with half-jitter.
6. **No environment leak:** spawn calls `.env_remove()` for `LD_PRELOAD`, `LD_LIBRARY_PATH`, `LD_AUDIT`, `DYLD_INSERT_LIBRARIES`, `DYLD_LIBRARY_PATH` (per `rules_rust_processos_externos.md`).
7. **Detach justification:** documented inline at spawn site per `rules_rust_processos_externos.md` section "Child detach justificado".

## Consequences

**Positive:**
- Interactive CLI UX is usable (sub-second response on warm daemon)
- ~17s startup amortized across many invocations
- Memory pressure bounded by single daemon process

**Negative:**
- Adds complexity to CLI install/upgrade flows (daemon must be restarted on upgrade)
- Increases attack surface (Unix socket / named pipe is one more boundary)
- Requires periodic memory monitoring (BERT models leak slowly per fastembed/ort upstream issues)

## Alternatives Considered

1. **No daemon** — rejected: 17s startup makes CLI unusable interactively.
2. **In-memory caching via shared library** — rejected: violates Rust "no global state" guidelines and complicates testing.
3. **External model server (gRPC)** — rejected: adds network surface and protocol overhead; daemon via Unix socket is simpler.

## Compliance

- `src/daemon.rs` SAFETY comment at the spawn site cross-references this ADR.
- `language-check` CI gate validates rule×design alignment.
- This ADR amends the rules above; future audits should treat the warmup daemon as compliant.
