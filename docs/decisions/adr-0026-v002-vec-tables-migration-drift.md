# ADR-0026: V002 `vec_tables` Migration Drift Must Be Fixed At The Binary Level

- Status: Accepted (2026-06-09)
- Deciders: Danilo Aguiar
- Scope: `migrations/V002__vec_tables.sql`, `src/commands/migrate.rs`, operator install/rebuild flow

## Context

The true v1.0.76 source tree embeds `migrations/V002__vec_tables.sql`
as a 721-byte no-op migration that ends in `SELECT 1;`. The historical
v1.0.54 source tree embeds a different V002 file: 834 bytes of
`sqlite-vec` DDL that creates `vec_memories`, `vec_entities`, and
`vec_chunks`.

Refinery 0.9.1 does not compare migration files with SHA-256. It
verifies the applied migration against the embedded file content using
its own SipHasher13 checksum over `(name, version, sql)`. If the
checksum stored in `refinery_schema_history` does not match the SQL
embedded in the running binary, refinery aborts with exit code 20 before
any write path (`remember`, `ingest`, `edit`, and related commands) can
continue.

In concrete terms, the full SipHasher13 mechanism is:

```rust
let mut hasher = SipHasher13::new();   // keys 0, 0
name.hash(&mut hasher);                // "vec_tables"
version.hash(&mut hasher);             // 2i32
sql.hash(&mut hasher);                 // contents of the .sql file
let checksum = hasher.finish();        // u64
```

For `name = "vec_tables"` and `version = 2i32`, the historical 834-byte
V002 embedded in the binary produces `10367736093436539632`. The 721-byte
no-op V002 recorded by the database produces `16903500262185826246`.

On 2026-06-09 the installed binary at `~/.cargo/bin/sqlite-graphrag`
reported version `1.0.76`, but inspection showed it had been compiled
from the v1.0.54 source package and still embedded the old 834-byte V002
file. The database itself had been created by the true v1.0.76 build, so
its stored checksum matched the 721-byte no-op V002. The mismatch was
therefore binary provenance drift, not database corruption.

## Decision

When the failure is `applied migration V2__vec_tables is different than
filesystem one V2__vec_tables`, treat the binary as the first suspect.
Do not trust `sqlite-graphrag --version` alone.

The fix is to rebuild the binary from the intended source checkout and
replace the installed executable:

```bash
cargo build --release
cp target/release/sqlite-graphrag ~/.cargo/bin/sqlite-graphrag
```

Manual edits to `refinery_schema_history` are a last resort. Before any
checksum surgery:

- back up the database
- confirm which V002 SQL the running binary embeds
- compute the checksum with Refinery's SipHasher13 algorithm, not SHA-256

## Consequences

### Positive

- The remediation is simple and preserves existing data.
- Operators avoid rewriting `refinery_schema_history` when the database is
  already correct.
- Future drift triage starts with binary provenance, which is faster and
  safer than mutating migration history rows.

### Negative

- `--version` is not a sufficient integrity signal for installed
  binaries.
- A mislabeled binary can look current while still embedding obsolete
  migrations.
- Operators must keep a rebuild path available when debugging migration
  drift.

## Verification

- `cargo build --release`: green — rebuilt the local source checkout into
  a new `target/release/sqlite-graphrag`
- Installed binary replaced at `~/.cargo/bin/sqlite-graphrag`: the new
  executable size dropped from ~37 MB to ~15 MB
- `remember` validation: `echo "teste" | timeout 60 sqlite-graphrag
  remember --name diagnose-final-006 --type note --description "fix"
  --body-stdin`
- `remember` result: `{"memory_id": 1207, "action": "created",
  "chunks_created": 1, "elapsed_ms": 34437}`
- The residual gap of 1138 memories without embeddings was not the root
  cause of the ~28-hour stall. The actual blocker was migration drift.
- Post-fix health checks: `health`, `stats`, `hybrid-search`, `graph
  entities`, `read`, `list`, `remember`, and `forget` all succeeded
- After fixing the drift, the local embedding pipeline was functional
  again and the validation run completed successfully in ~34 s.
- Decision persisted in the graph as
  `decisao-fix-migration-v2-vec-tables` with `type = decision` and a
  short description
- Post-fix database state: 1142 preserved memories before recording this
  decision, 1143 after; `schema_version = 13`; FTS5 and integrity checks
  remained OK

## Lessons

- Never trust `--version` without checking the embedded migration
  content when refinery reports drift.
- Rebuild from the local source tree before diagnosing V002 drift in
  depth.
- Back up first if you must touch `refinery_schema_history`.
- Refinery 0.9.1 uses SipHasher13 with the default keys for migration
  checksums.
