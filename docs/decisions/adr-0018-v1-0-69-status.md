# ADR-0018 — Status de Fechamento v1.0.69 (2026-06-05)

- **Status.** Accepted.
- **Date.** 2026-06-05.
- **Deciders.** Danilo Aguiar (operator), Claude Code (advisor).
- **Supersedes.** None.
- **Related.** ADR-0011, ADR-0012, ADR-0013, ADR-0014, ADR-0015, ADR-0016, ADR-0017.

## Context

The release v1.0.69 closes 12 gaps documented in `gaps.md` (G28 through G39). Each gap has its own ADR (0011-0017 plus 0008-0010 inherited from v1.0.68). This ADR is the executive summary that the operator reads FIRST to confirm the release is ready for publication.

## Decision

### Gap closure matrix

| Gap | Severity | Decision | ADR |
| --- | --- | --- | --- |
| G28 (CRITICAL) | Proliferation of subprocesses | A: 7 flags hardening + B: singleton by db_hash + C: SIGTERM on timeout + D: system_load + CircuitBreaker + reaper | ADR-0011 |
| G29 | CHECK constraint + audit + preservation | Enum MemorySource + audit trail + Jaccard gate + blake3 idempotency + scripts/legacy | ADR-0012, ADR-0015 |
| G30 | Singleton ignores `--db` | db_hash BLAKE3 + --wait-job-singleton + --force-job-singleton | ADR-0013 |
| G31 | Missing 5 hardening flags in `enrich --mode codex` | codex_spawn helper unificado | ADR-0014 |
| G32 | Wrong JSON parser in `enrich --mode codex` | parse_codex_jsonl shared | ADR-0014 |
| G33 | No model validation against OAuth whitelist | validate_codex_model + codex-models subcommand | ADR-0014 |
| G34 | Worker warning ignores mode | match args.mode | (inline in `enrich.rs:1502`) |
| G35 | No preflight or fallback for rate limit | --preflight-check, --fallback-mode, --rate-limit-buffer | (inline in `enrich.rs:653-749`) |
| G36 | FTS5 unconditional rebuild | --fts-dry-run, --fts-progress, --yes | ADR-0016 |
| G37 | No --names / --names-file | comma-delimited + file-based | (inline in `enrich.rs`) |
| G38 | Backup step size too small | 1000/5ms defaults + 4 new flags | (inline in `backup.rs:20-22`) |
| G39 | vec_memories_orphaned without remediation | vec orphan-list + purge-orphan + stats + forget hook | ADR-0017 |

### OAuth-Only Enforcement (BEHAVIOUR CHANGE)

ADR-0011 documents the most consequential change in v1.0.69: the spawn of `claude -p` and `codex exec` now ABORTS when `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is set in the environment. The `--bare` flag (which demands an API key and disables OAuth) is REMOVED from all executable code. The variable whitelist excludes both API-key variables as defence-in-depth.

Operators using API keys MUST migrate to OAuth. The error message is actionable and points at the OAuth login flow.

### Test count delta

- v1.0.68: 692 tests.
- v1.0.69: 745 tests (+53).
- Notable additions: 11 in `codex_spawn`, 10 in `preservation`, 8 in `memory_source`, 5 in `vec`, 4 OAuth-only conformance, 4 reaper, 5 system_load, 6 lock.

### Documentation

- 7 new ADRs (0011-0017) document each architectural decision.
- `gaps.md` is the canonical source of truth for what was wrong; this ADR is the canonical source of truth for what was fixed.
- `CHANGELOG.md` (EN) and `CHANGELOG.pt-BR.md` (PT) list every change.
- `AGENTS.md` (EN) and `AGENTS.pt-BR.md` (PT) include the v1.0.69 section AND fix the obsolete "API keys are optional" line in the Authentication Note.

## Consequences

- The v1.0.69 release is feature-complete and safe to publish.
- Operators running v1.0.68 who relied on API keys will see a `Validation` error and a clear migration path. The migration is OAuth login, which the v1.0.68 documentation already described.
- The test count of 745 is the floor for v1.0.70; any regression that drops tests below 745 must be fixed in a hotfix.
- The 7 ADRs and `gaps.md` together form the audit trail. Future maintainers can reconstruct every decision by reading them in order.

## References

- `gaps.md` (2424 lines, 12 gaps, full history).
- `CHANGELOG.md` and `CHANGELOG.pt-BR.md` v1.0.69 sections.
- `docs/AGENTS.md` and `docs/AGENTS.pt-BR.md` v1.0.69 sections.
- `docs/decisions/adr-0008-0018*.md` (8 inherited + 7 new ADRs).
- `src/commands/claude_runner.rs:574-666` (4 OAuth-only tests).
- `src/commands/codex_spawn.rs:684-758` (4 OAuth-only tests).
- `src/commands/optimize.rs:36-67` (3 new FTS5 flags).
- `src/commands/vec.rs` (~430 lines, 3 tests).
- `src/preservation.rs` (10 tests).
- `src/memory_source.rs` (8 tests).
- `src/reaper.rs` (4 tests).
- `src/system_load.rs` (5 tests).
