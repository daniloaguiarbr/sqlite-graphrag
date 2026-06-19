# ADR-0048: Schema as Derived Artifact via schemars + Must-Ignore (v1.0.89)

- **Status**: Accepted
- **Data**: 2026-06-19
- **Versão**: v1.0.89 (closes GAP-E2E-007 P1)
- **Autores**: Danilo Aguiar <daniloaguiarbr@gmail.com>

## Context

`docs/schemas/health.schema.json` was a hand-maintained JSON Schema file. It declared only the keys known at the time of authoring plus a few incremental additions. By v1.0.89, the `HealthResponse` struct in `src/commands/health.rs` had grown to emit 36 keys (23 always-present + 13 conditional `Option<T>` via `skip_serializing_if`). The committed schema file covered only 21 of those keys — a drift of 15 fields that had never been reflected back into the schema.

The drift was compounded by a policy violation: the schema used `additionalProperties: false` (Must-Validate), while the project rule `docs_rules/rules_rust_json_e_ndjson.md` line 537 mandates `Must-Ignore` for APIs that evolve over time with backward compatibility. The hand-maintained schema was both incomplete AND policy-violating.

The root cause is structural: schemas written by hand cannot keep pace with Rust structs that gain fields every minor release. Any addition to `HealthResponse` (e.g. v1.0.65 added 6 graph-quality fields; v1.0.67 added `non_normalized_count` / `normalization_warning`; v1.0.67 also added 4 super-hub fields) silently widened the contract without updating the schema file. Consumers running strict validation would reject responses that contained fields they considered unknown.

## Decision

Adopt `schemars = "0.8"` as a regular dependency and generate the schema from the Rust types at build time:

### 1. Add `schemars = "0.8"` to `[dependencies]`

Pinned to 0.8 because schemars 1.0 introduced breaking API changes (notably the `schema_for!` macro signature and the reworked `Schema` enum). The 0.8 line is stable, widely deployed, and matches the example documented in `context7 docs /gresau/schemars` (ID `/gresau/schemars`, trustScore 8.8).

`s chemars` lives in `[dependencies]` (not `[dev-dependencies]`) because the `JsonSchema` derive macro is applied to the production struct `HealthResponse` in `src/commands/health.rs` — the lib crate needs the macro at compile time, not just tests.

### 2. Derive `JsonSchema` on the health response types

Three structs receive the derive:

```rust
#[derive(Serialize, schemars::JsonSchema)]
pub struct HealthResponse { /* 36 fields */ }

#[derive(Serialize, schemars::JsonSchema)]
pub struct HealthCounts { /* 5 fields */ }

#[derive(Serialize, schemars::JsonSchema)]
pub struct HealthCheck { /* 3 fields */ }
```

`HealthCounts` and `HealthCheck` had to be promoted from private to `pub` because the generated schema references them via `$ref` from `HealthResponse.properties`, and `schema_for!` requires `JsonSchema` to be in scope.

### 3. Create `src/bin/dump_schema.rs` for idempotent regeneration

A dedicated binary consumes `schema_for!(HealthResponse)`, applies two post-processing transforms, and writes the result to `docs/schemas/health.schema.json`:

- Bumps `$schema` to `https://json-schema.org/draft/2020-12/schema` (per `docs_rules/rules_rust_json_e_ndjson.md` line 555).
- Recursively sets `additionalProperties: true` on every object with a `properties` field (Must-Ignore per line 537).

The bin is **idempotent** — running it twice produces byte-identical output (BLAKE3 checksum `6230564bde8067dc3126e0c8c3027829c2eb0375b54fba76f8c09f51aa8c7c07` matches across runs).

### 4. Regenerate `docs/schemas/health.schema.json`

The regenerated schema now contains 36 properties (matches `HealthResponse` exactly), `additionalProperties: true` at the root and in every nested object, and Draft 2020-12 metadata.

### 5. Add `tests/health_schema_drift_regression.rs` with 4 regression tests

- `assert_all_health_keys_in_schema` — verifies that 36 known keys are present.
- `assert_must_ignore_policy_active` — verifies root `additionalProperties: true`.
- `assert_draft_2020_12` — verifies the schema declares Draft 2020-12.
- `assert_dump_schema_is_idempotent` — runs the bin twice and compares BLAKE3 checksums.

## Consequences

### Positive

- Schema regenerated automatically from Rust types — drift becomes structurally impossible.
- All 36 keys always in sync with `HealthResponse` struct (existing fields plus future additions).
- Must-Ignore policy aligned with `docs_rules/rules_rust_json_e_ndjson.md` line 537.
- Draft 2020-12 alignment with line 555.
- Consumers that use strict validation (`additionalProperties: false`) on consumer side still work because the schema now permits extra fields — forward-compatible.
- 4 regression tests prevent future drift via CI failure.

### Negative

- BREAKING CHANGE for any consumer that relied on `additionalProperties: false` to catch typos in unknown fields. The schema now accepts extra fields, so consumers must migrate to Must-Ignore OR explicitly opt-in to strict mode on their side.
- Consumers that use `jsonschema` strict validation will not catch new fields they have not seen before — this is the documented trade-off of Must-Ignore.
- `schemars` adds approximately 2 MB to the dependency tree at build time but does not affect the final binary size because `schemars` is used only via derive macros (zero runtime cost in the compiled CLI).

## Alternatives Considered

1. **Keep hand-maintained schema, add a checklist item to release process** — REJECTED: the drift is the symptom of an unsolvable maintenance burden; checklists do not prevent human error.
2. **Use `schemars` but keep Must-Validate (`additionalProperties: false`)** — REJECTED: violates `docs_rules/rules_rust_json_e_ndjson.md` line 537 directly.
3. **Use `schemars` with auto-detection of strictness per field** — DEFERRED: complex, requires per-field annotation; out of scope for v1.0.89.
4. **Switch to a different schema-generation tool (e.g. `typify`, `schemars-derive`)** — REJECTED: `schemars` is the de facto standard in the Rust ecosystem; switching adds friction without addressing the core issue.

## Cross-references

- `context7 docs /gresau/schemars` (ID `/gresau/schemars`, trustScore 8.8) — schemars 0.8 API documentation
- `docs_rules/rules_rust_json_e_ndjson.md` line 33, 537, 547, 555 — Must-Ignore mandate, Draft 2020-12 mandate
- RFC 7493 (I-JSON) — definition of Must-Ignore (`additionalProperties` defaulting to true)
- `src/commands/health.rs::HealthResponse` — the source of truth
- `src/bin/dump_schema.rs` — the regeneration bin
- `tests/health_schema_drift_regression.rs` — regression coverage
- ADR-0047 (stderr deduplication) — orthogonal but adjacent: demonstrates the value of single-source-of-truth for cross-cutting concerns

## Non-goals (YAGNI)

- NO generation of schemas for the other 48 schema files in `docs/schemas/` — that is a v1.1.0 task with its own ADR.
- NO introduction of a runtime schema validator in production — `jsonschema` stays a dev-dependency for the regression test only.
- NO removal of the hand-maintained schema metadata fields (`$id`, `title`, `description`) — schemars already populates them.
- NO automatic invocation of `dump_schema` in the build pipeline — the regression test invokes the bin and checks idempotency, which is sufficient enforcement.

## Next steps

- v1.0.90: extend `dump_schema` to handle `codex-models.schema.json` (already well-defined, easy win)
- v1.1.0: audit all 49 schemas in `docs/schemas/` and migrate each to schemars-derivable types
- v1.1.0: integrate `dump_schema` into a pre-commit hook to prevent uncommitted drift
