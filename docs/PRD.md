# PRD — sqlite-graphrag v2.1.0

> **Source of truth.** This document defines the contracts that any release MUST satisfy.
> Behavioral compliance is verified by `tests/prd_compliance.rs` (gated under `--features slow-tests`).

---

## 1. Naming Rules

### 1.1 Memory names with `__` prefix MUST be rejected (exit 1)
- Rationale: the `__` prefix is a reserved sentinel namespace; storing user content under it would collide with internal keys.
- Test: `prd_name_double_underscore_rejected`
- Validation: `sqlite-graphrag remember --name __reserved --type user --description "x" --body "y"` → exit 1

---

## 2. Cross-namespace Operations

### 2.1 Cross-namespace `link` MUST fail with exit 4 (entity not found)
- Rationale: entities are scoped to a namespace; referencing a non-existent target in another namespace must surface as a not-found error, not a silent corruption.
- Test: `prd_cross_namespace_link_rejected`
- Validation: `sqlite-graphrag link --from <existing> --to <nonexistent> --relation related --namespace global` → exit 4

---

## 3. Soft-delete & Restore

### 3.1 Forgotten memories MUST NOT appear in active queries
- Rationale: soft-delete sets `deleted_at`; any query filtering `WHERE deleted_at IS NULL` must exclude soft-deleted rows.
- Test: `prd_soft_delete_recall_does_not_return_forgotten`
- Validation: after `forget`, `SELECT COUNT(*) FROM memories WHERE name=<name> AND deleted_at IS NULL` → 0; `deleted_at IS NOT NULL` → 1

### 3.2 The FTS trigger `trg_fts_ad` MUST be idempotent on double-delete
- Rationale: a second deletion attempt on an already-removed FTS row must not corrupt the FTS index or raise an error.
- Test: `prd_trg_fts_ad_idempotent_double_delete`
- Validation: after manual soft-delete + `DELETE FROM fts_memories WHERE rowid=<id>`, `INSERT INTO fts_memories(fts_memories) VALUES('integrity-check')` → succeeds (no error)

### 3.3 `restore` MUST revert a soft-deleted memory to active state
- Rationale: `restore` is the recovery path for accidental `forget`; the memory must have `deleted_at = NULL` after a successful restore.
- Test: `prd_restore_reverte_soft_delete`
- Validation: `sqlite-graphrag forget --name <name> --namespace global` then `sqlite-graphrag restore --name <name> --namespace global --version <v>` → `deleted_at IS NULL`

### 3.4 `purge --retention-days N` MUST permanently remove soft-deleted memories older than N days
- Rationale: purge is the hard-delete mechanism; records with `deleted_at` older than the retention window must be removed from the `memories` table entirely.
- Test: `prd_purge_retention_removes_old_soft_deleted`
- Validation: `sqlite-graphrag purge --retention-days 1 --yes` with a memory soft-deleted 2 days ago → `SELECT COUNT(*) FROM memories WHERE name=<name>` → 0

---

## 4. Versioning & History

### 4.1 `remember` with `--force-merge` MUST include `merged_into_memory_id` in JSON output
- Rationale: callers need to detect whether a new record was created or merged into an existing one; the field signals merge semantics.
- Test: `prd_remember_duplicate_returns_merged_into_memory_id`
- Validation: `sqlite-graphrag remember --name <existing> ... --force-merge` → JSON stdout contains key `merged_into_memory_id`

### 4.2 `remember` JSON output MUST include `entities_persisted` and `relationships_persisted`
- Rationale: these fields allow callers to confirm that NER extraction produced graph artifacts without querying the database directly.
- Test: `prd_remember_json_contains_entities_and_relationships_persisted`
- Validation: `sqlite-graphrag remember ... --skip-extraction` → JSON stdout contains keys `entities_persisted` and `relationships_persisted`

### 4.3 `history` output MUST include `created_at_iso` in each version entry
- Rationale: ISO 8601 timestamps in history entries allow callers to correlate versions with wall-clock events without epoch conversion.
- Test: `prd_history_includes_created_at_iso`
- Validation: `sqlite-graphrag history --name <name> --namespace global` → `json["versions"][0]["created_at_iso"]` present

### 4.4 `rename` MUST update the memory version in `memory_versions`
- Rationale: a rename is a semantic change; the versioning table must record it so history reflects the transition.
- Test: `prd_rename_updates_version`
- Validation: `sqlite-graphrag rename --name <orig> --new-name <new> --namespace global` → `SELECT COUNT(*) FROM memory_versions WHERE name=<new> OR name=<orig>` ≥ 1; memory exists with new name

---

## 5. Graph Operations

### 5.1 `link` MUST create an entry in `memory_relationships` or `relationships`
- Rationale: a successful link must be persisted so the graph is queryable; no silent no-op allowed.
- Test: `prd_link_creates_memory_relationships`
- Validation: `sqlite-graphrag link --from <src> --to <dst> --relation related --namespace global` (success path) → `COUNT(*) FROM memory_relationships` > 0 OR `COUNT(*) FROM relationships` > 0

### 5.2 `unlink` MUST remove only the specified relation, preserving others
- Rationale: unlink is a targeted operation; removing one edge must not cascade to other edges from the same source.
- Test: `prd_unlink_removes_only_specific_relation`
- Validation: with edges A→B and A→C, `sqlite-graphrag unlink --from ent-a --to ent-b --relation related --namespace global` → `COUNT(*) FROM relationships WHERE source_id=A` = 1

### 5.3 `graph --format json` output MUST contain `nodes` and `edges` fields
- Rationale: downstream consumers (visualisers, agents) depend on a stable JSON schema with these two top-level keys.
- Test: `prd_graph_json_contains_nodes_and_edges`
- Validation: `sqlite-graphrag graph --format json --namespace global` → JSON contains `nodes` and `edges`

### 5.4 `graph --format dot` output MUST start with `digraph sqlite-graphrag {`
- Rationale: the DOT format header is required for any compliant Graphviz consumer; deviating breaks toolchain integrations.
- Test: `prd_graph_dot_is_valid_digraph`
- Validation: `sqlite-graphrag graph --format dot --namespace global` → stdout contains `digraph sqlite-graphrag {`

### 5.5 `graph --format mermaid` output MUST contain `graph LR`
- Rationale: the Mermaid directive `graph LR` signals a left-to-right directed graph; renderers require this exact token.
- Test: `prd_graph_mermaid_starts_with_graph_lr`
- Validation: `sqlite-graphrag graph --format mermaid --namespace global` → stdout contains `graph LR`

### 5.6 `cleanup-orphans` MUST remove entities that have no associated memories
- Rationale: orphan entities inflate the graph without contributing recall value; cleanup-orphans must remove them and report the count.
- Test: `prd_cleanup_orphans_removes_entities_without_memories`
- Validation: `sqlite-graphrag cleanup-orphans --yes` with one orphan entity → `json["deleted"]` ≥ 1; entity absent from `entities` table

---

## 6. Search & Recall

### 6.1 FTS5 index MUST use `unicode61 remove_diacritics` tokenizer
- Rationale: diacritic-insensitive search (e.g., "nao" matching "não") is a documented feature; the tokenizer must be configured accordingly.
- Test: `prd_fts5_unicode61_remove_diacritics`
- Validation: `SELECT sql FROM sqlite_master WHERE name='fts_memories'` → contains `unicode61` or `remove_diacritics`

### 6.2 `hybrid-search` MUST accept `--rrf-k` argument (default 60)
- Rationale: RRF k=60 is the documented default for Reciprocal Rank Fusion; the argument must be accepted without error.
- Test: `prd_hybrid_search_rrf_k_default_60`
- Validation: `sqlite-graphrag hybrid-search "query" --rrf-k 60 --namespace global` → exit 0

---

## 7. Vector Storage

### 7.1 `vec_memories` table MUST declare `distance_metric=cosine`
- Rationale: cosine distance is the correct metric for semantic embedding similarity; using a different default would silently degrade recall quality.
- Test: `prd_vec_memories_distance_metric_cosine`
- Validation: `SELECT sql FROM sqlite_master WHERE name='vec_memories'` → contains `cosine`

---

## 8. Optimisation & Maintenance

### 8.1 `edit` with a stale `--expected-updated-at` timestamp MUST return exit 3 (Conflict)
- Rationale: optimistic concurrency control requires that edits based on an outdated snapshot are rejected; exit 3 is the designated conflict code.
- Test: `prd_edit_expected_updated_at_stale_returns_exit_3`
- Validation: `sqlite-graphrag edit --name <name> --namespace global --body "x" --expected-updated-at 0` → exit 3

### 8.2 `optimize` MUST exit successfully and return `{"status": "ok"}`
- Rationale: the optimize command runs FTS and vector maintenance; callers must be able to detect completion via the status field.
- Test: `prd_optimize_runs_and_returns_status_ok`
- Validation: `sqlite-graphrag optimize` → exit 0; `json["status"]` = `"ok"`

### 8.3 `vacuum` MUST return `size_before_bytes` and `size_after_bytes`
- Rationale: callers need to measure the space reclaimed; both fields are required for before/after comparison.
- Test: `prd_vacuum_returns_size_before_and_size_after`
- Validation: `sqlite-graphrag vacuum` → exit 0; JSON contains `size_before_bytes` and `size_after_bytes`

---

## 9. Configuration & Security

### 9.1 At most 4 concurrent instances MUST be allowed; the 5th MUST return exit 75
- Rationale: bounded concurrency prevents database contention; exit 75 (`EX_TEMPFAIL`) signals the caller to retry later.
- Test: `prd_five_instances_fifth_returns_exit_75`
- Validation: with 4 lock slots occupied, `sqlite-graphrag --max-concurrency 4 --wait-lock 0 namespace-detect` → exit 75

### 9.2 Body length MUST NOT exceed 512,000 bytes; violations MUST return exit 6
- Rationale: an unbounded body would exhaust FTS and embedding memory; the hard limit protects runtime stability.
- Test: `prd_max_body_len_exceeded_returns_exit_6`
- Validation: `sqlite-graphrag remember ... --body-file <file-of-512001-bytes>` → exit 6

### 9.3 `SQLITE_GRAPHRAG_NAMESPACE` env var MUST be accepted as the default namespace
- Rationale: scripted pipelines set the namespace once via the environment; the CLI must honour it without requiring a per-command flag.
- Test: `prd_sqlite_graphrag_namespace_env_works`
- Validation: `remember --namespace ns-from-env` → `SELECT namespace FROM memories WHERE name=<name>` = `ns-from-env`

### 9.4 Database file MUST have permissions `600` (owner read/write only) after `init` on Unix
- Rationale: the database stores sensitive memory content; world-readable permissions would be a confidentiality breach.
- Test: `prd_chmod_600_aplicado_apos_init` (`#[cfg(unix)]`)
- Validation: `sqlite-graphrag init` → `stat <db>` → `mode & 0o777` = `0o600`

### 9.5 Path traversal (`..`) in `SQLITE_GRAPHRAG_DB_PATH` MUST be rejected
- Rationale: accepting paths containing `..` would allow the CLI to write outside the intended directory, a path traversal vulnerability.
- Test: `prd_path_traversal_rejected_in_db_path`
- Validation: `SQLITE_GRAPHRAG_DB_PATH=../../../etc/passwd sqlite-graphrag init` → non-zero exit

---

## 10. Output & Diagnostics

### 10.1 `health` MUST return JSON with `integrity_ok` and `schema_ok` fields
- Rationale: health checks are consumed by monitoring agents; these two boolean fields are the documented contract for liveness and schema conformance.
- Test: `prd_health_emits_integrity_ok_and_schema_ok`
- Validation: `sqlite-graphrag health` → exit 0; JSON contains `integrity_ok` and `schema_ok`

### 10.2 `stats` MUST return JSON with `memories`, `entities`, and `relationships` fields
- Rationale: stats is the primary observability command; the three counters give an overview of the graph state.
- Test: `prd_stats_inclui_memories_entities_relationships`
- Validation: `sqlite-graphrag stats` → exit 0; JSON contains `memories`, `entities`, `relationships` (and optionally `memories_total`)

### 10.3 `list` MUST respect the `--limit` argument
- Rationale: pagination requires that the server-side limit is enforced; returning more items than requested breaks downstream consumers.
- Test: `prd_list_respeita_limit`
- Validation: with 5 memories stored, `sqlite-graphrag list --namespace global --limit 2` → `json["items"].length` = 2

### 10.4 `sync-safe-copy` MUST produce a coherent snapshot with `bytes_copied > 0` and `status = "ok"`
- Rationale: backup integrity depends on the snapshot being a consistent copy; `bytes_copied` confirms data was written and `status` confirms no error occurred.
- Test: `prd_sync_safe_copy_generates_coherent_snapshot`
- Validation: `sqlite-graphrag sync-safe-copy --dest <path>` → exit 0; JSON contains `bytes_copied` > 0 and `status = "ok"`; destination file exists
