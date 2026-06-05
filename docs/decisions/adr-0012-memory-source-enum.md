# ADR-0012 — MemorySource Enum Tipado (v1.0.69)

- **Status.** Accepted.
- **Date.** 2026-06-05.
- **Deciders.** Danilo Aguiar (operator), Claude Code (advisor).
- **Supersedes.** None.
- **Related gaps.** G29 (violação de CHECK constraint), G29 Passo 2 (runtime guard).

## Context

The `memories` table in SQLite has a CHECK constraint: `source TEXT NOT NULL DEFAULT 'agent' CHECK(source IN ('agent','user','system','import','sync'))`. The Rust `NewMemory` struct declared `pub source: String`, allowing any string at the type level. The CHECK constraint was the only line of defence, and `enrich.rs:902` introduced a `source: "enrich".to_string()` literal that broke the contract — every `enrich --operation body-enrich` invocation failed with `SQLITE_CONSTRAINT_CHECK`.

The hotfix changed the literal to `"agent"`, but the underlying fragility persisted: eight call-sites (`remember`, `rename`, `ingest`, `ingest_claude`, `ingest_codex`, `remember_batch`, `enrich`, `edit`) all used `String` literals, and a future refactor could re-introduce the same bug.

## Decision

1. Create `src/memory_source.rs` with a `MemorySource` enum (`Agent`, `User`, `System`, `Import`, `Sync`) implementing `as_str`, `Display`, `TryFrom<&str>`, `Serialize`, and `Deserialize`. Eight unit tests cover valid/invalid/empty/display/serialisation paths.
2. Add `pub fn validate_source(raw: &str) -> Result<&'static str, AppError>` as a runtime guard. It is called from `memories::insert` and `memories::update`, providing defence-in-depth even when call-sites still use `String`.
3. Existing call-sites keep using `String` for binary compatibility (no migration needed). The enum is the foundation for the v1.0.70 schema migration that will replace the `String` field with the enum type.
4. The runtime guard is OBSERVABLE in the changelog and behaves identically to the type-level check: an invalid `source` returns `AppError::Validation` listing the accepted values.

## Consequences

- The CHECK constraint can no longer be violated through the documented code paths. Any future call-site that uses a literal not in the five-value set will fail at compile time or runtime.
- The migration from `String` to `MemorySource` in `NewMemory` is deferred to v1.0.70 to keep v1.0.69 a non-breaking change.
- 8 unit tests are added; the runtime guard adds 4 more tests. Total +12 tests.
- The enum is the public API surface for the v1.0.70 migration and is exported from `src/lib.rs`.

## Alternatives Considered

- Replace the `String` field with the enum in v1.0.69. REJECTED. The change would break every call-site that constructs `NewMemory` (8 files). A migration release (v1.0.70) should land the breaking change with a clear upgrade guide.
- Drop the runtime guard and rely only on the type-level enum. REJECTED. The migration is not yet done, so the runtime guard is the safety net for the existing 8 `String` call-sites.

## References

- `src/memory_source.rs` (enum + 8 tests + runtime guard).
- `src/storage/memories.rs:180-195` (insert calls `validate_source`).
- `src/storage/memories.rs:212-260` (update calls `validate_source`).
- `src/lib.rs:179-181` (`pub mod memory_source`).
- `src/commands/enrich.rs:1227` (hotfix literal `"agent"`).
- gaps.md G29 lines 533-1038 (full history).
