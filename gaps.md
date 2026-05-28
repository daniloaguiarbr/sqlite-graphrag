# Gaps v1.0.64 — Acceptance Testing 2026-05-28

- Acceptance result: 170/170 PASS + 6 FINDINGs
- Binary: sqlite-graphrag 1.0.64 (crates.io)
- Production DB: 955 memories, 2000 entities, 2519 relationships, schema v11
- All 4 v1.0.64 fixes confirmed, all 6 v1.0.60 bugs confirmed fixed


## GAP-01 HIGH: Missing deep-research.schema.json

- Test ID: T050, T239e
- Phase: F3, F19c

### Problem
- `docs/schemas/` has 48 schema files covering every subcommand except `deep-research`
- The `deep-research` command emits a JSON response with 5 top-level fields, 4 nested structs, and 7 stats fields
- No machine-readable contract exists for consumers to validate the response

### Consequences
- Agents parsing `deep-research` JSON output cannot validate against a schema
- `schema_contract_strict.rs` cannot include a `schema_36_deep_research` test
- CI has no schema drift detection for `deep-research` — struct changes in `src/commands/deep_research.rs` can break consumers silently
- Inconsistency: 48 of 49 JSON-emitting commands have schemas; `deep-research` is the sole exception

### Root Cause
- `deep-research` was implemented in a single release (v1.0.64) with 802 LOC and 14 unit tests
- Schema creation was not part of the implementation checklist for the new command
- The PDCA verification phase did not include schema file existence as a gate

### Solution
- Create `docs/schemas/deep-research.schema.json` following Draft 2020-12 with `additionalProperties: false`
- Schema must cover 5 structs: `DeepResearchResponse`, `SubQuery`, `DeepResult`, `EvidenceChain`, `EvidenceNode`, `ResearchStats`
- Use `skip_serializing_if` semantics: `body` and `hop_distance` in `DeepResult`, `relation` and `weight` in `EvidenceNode` must be marked as non-required
- Derive schema from `src/commands/deep_research.rs` lines 101-154

### Benefits
- Restores 100% schema coverage (49/49 commands)
- Enables `schema_36_deep_research` contract test in CI
- Consumers can validate `deep-research` output programmatically
- Prevents silent breaking changes in the response format

### How to Resolve
- Step 1: Read structs `DeepResearchResponse`, `SubQuery`, `DeepResult`, `EvidenceChain`, `EvidenceNode`, `ResearchStats` from `src/commands/deep_research.rs:101-154`
- Step 2: Create `docs/schemas/deep-research.schema.json` with `$defs` for each nested struct
- Step 3: Mark optional fields (`body`, `hop_distance`, `relation`, `weight`) as non-required
- Step 4: Add `"additionalProperties": false` at every object level
- Step 5: Validate with `jaq '.' docs/schemas/deep-research.schema.json` for well-formedness
- Step 6: Run `sqlite-graphrag deep-research "test" --json` and validate output against schema
- Effort: ~1 hour
- Files: `docs/schemas/deep-research.schema.json` (new)


## GAP-02 HIGH: No contract tests for deep-research

- Test ID: T051
- Phase: F3

### Problem
- `tests/doc_contract_integration.rs` has 36 contract tests (`contract_01` through `contract_35`) covering every subcommand
- `tests/schema_contract_strict.rs` has 35 schema validation tests (`schema_01` through `schema_35`)
- Neither file includes tests for `deep-research`
- The 14 unit tests in `src/commands/deep_research.rs` cover decomposition and serialization but NOT the full CLI round-trip

### Consequences
- No CI gate validates that `sqlite-graphrag deep-research "query" --json` produces conformant output
- Struct field renames, type changes, or missing fields go undetected until production
- The 14 existing unit tests verify internal logic (decompose_query, serde) but not the integrated path (CLI args -> DB query -> JSON output)
- Regression risk: refactoring `deep_research.rs` can break the JSON contract without any test failing

### Root Cause
- Contract tests follow a sequential numbering pattern (`contract_01` to `contract_35`)
- Adding `contract_36_deep_research` requires manually writing the test function and adding it to the test file
- The v1.0.64 implementation focused on unit tests for the new decomposition logic but omitted integration-level contract tests
- No checklist item in the release process requires contract test coverage for new commands

### Solution
- Add `contract_36_deep_research` to `tests/doc_contract_integration.rs`
- Add `schema_36_deep_research` to `tests/schema_contract_strict.rs` (requires GAP-01 resolved first)
- Test must validate: exit 0, top-level keys, stats fields, result item fields, sub_query fields

### Benefits
- Extends CI coverage to 37/37 contract tests (100%)
- Detects JSON schema drift automatically on every PR
- Prevents regression in the `deep-research` JSON contract
- Aligns with the project pattern: every command has contract + schema tests

### How to Resolve
- Step 1: Add `contract_36_deep_research` to `tests/doc_contract_integration.rs` after `contract_35_rename_entity`
- Step 2: Test body: invoke `sqlite-graphrag deep-research "test query" --k 3 --json`, parse JSON, assert top-level keys, stats fields, result structure
- Step 3: Add `schema_36_deep_research` to `tests/schema_contract_strict.rs` (loads `deep-research.schema.json`, validates output)
- Step 4: Run `cargo test --test doc_contract_integration contract_36` and `cargo test --test schema_contract_strict schema_36`
- Effort: ~1 hour (depends on GAP-01 for schema test)
- Files: `tests/doc_contract_integration.rs`, `tests/schema_contract_strict.rs`
- Dependency: GAP-01 must be resolved first for schema_36


## GAP-03 MEDIUM: docs/schemas/README.md missing deep-research entry

- Test ID: T052, T239f
- Phase: F3, F19c

### Problem
- `docs/schemas/README.md` lists 48 schema files in a Markdown table mapping subcommand to schema file
- The table does not include an entry for `deep-research`
- The table is bilingual (EN + PT-BR sections) — both sections are missing the entry

### Consequences
- Developers and agents consulting the schema index cannot discover the `deep-research` schema
- The README becomes an incomplete index — 48/49 commands listed
- Automated tools that parse the README table to enumerate available schemas will miss `deep-research`

### Root Cause
- Schema README updates are manual — each new schema requires adding a row to both EN and PT-BR tables
- The v1.0.64 doc audit covered 21 files but did not check `docs/schemas/README.md`
- No CI gate validates that every `.schema.json` file has a corresponding README entry

### Solution
- Add row `| \`deep-research\` | \`deep-research.schema.json\` |` to both EN and PT-BR tables
- Position after `hybrid-search` row (alphabetical or by command family)

### Benefits
- Restores 100% index coverage (49/49 entries)
- Developers can discover the schema from the README
- Consistent with every other subcommand in the table

### How to Resolve
- Step 1: Edit `docs/schemas/README.md`
- Step 2: Add `| \`deep-research\` | \`deep-research.schema.json\` |` after the `hybrid-search` row in EN table
- Step 3: Add the same row in the PT-BR table
- Step 4: Verify with `rg 'deep-research' docs/schemas/README.md`
- Effort: 5 minutes
- Files: `docs/schemas/README.md`
- Dependency: GAP-01 must be resolved first (schema file must exist before indexing)


## GAP-04 MEDIUM: docs/TESTING.md missing deep-research integration test section

- Test ID: T053, T196
- Phase: F3, F15

### Problem
- `docs/TESTING.md` has a section "v1.0.64 Regression Tests" (line 67) that mentions unit tests in `deep_research.rs`
- The document does NOT mention `deep-research` as a command name (hyphenated) anywhere
- Other sections document test categories: "Claude Code Ingest Tests", "Codex Ingest Tests", "Daemon Tests"
- No equivalent section exists for `deep-research` integration testing

### Consequences
- Contributors do not know how to test the `deep-research` command
- The v1.0.64 section implies only unit tests exist (14 tests) but omits the contract/schema testing gap
- `docs/TESTING.pt-BR.md` has the same gap (bilingual docs must stay synchronized)

### Root Cause
- The TESTING.md v1.0.64 section was written when only unit tests existed
- Contract and schema tests were not created (GAP-01, GAP-02) so there was nothing to document
- The doc audit did not flag the absence of integration-level test documentation

### Solution
- Add a section "### Deep Research Tests" after "Codex Ingest Tests (v1.0.62)" (line 57)
- Document: unit tests location, contract test (when GAP-02 resolved), schema test (when GAP-01 resolved)
- Add the same section to `docs/TESTING.pt-BR.md`

### Benefits
- Contributors know how to run and extend `deep-research` tests
- Complete documentation of all test categories
- Bilingual consistency maintained

### How to Resolve
- Step 1: Edit `docs/TESTING.md` — add section after line 61
- Step 2: Document unit tests (`src/commands/deep_research.rs`, 14 tests) and planned contract/schema tests
- Step 3: Mirror the same section in `docs/TESTING.pt-BR.md`
- Step 4: Verify with `rg 'deep-research' docs/TESTING.md`
- Effort: 15 minutes
- Files: `docs/TESTING.md`, `docs/TESTING.pt-BR.md`
- Dependency: GAP-01 and GAP-02 for complete documentation


## GAP-05 LOW: mentions_ratio at 0.0% after graph quality cleanup

- Test ID: N/A (observational)
- Phase: F10

### Problem
- `health --json` reports `mentions_ratio: 0.0` and `mentions_warning: null`
- Previous versions had mentions_ratio at 61.5% (v1.0.63 acceptance testing IMPROVEMENT-1)
- The ratio dropped to 0.0% which is ideal but should be verified as intentional cleanup

### Consequences
- No negative consequence — 0.0% mentions is the ideal state
- Graph quality is at its best historical level
- Signals that prior graph quality improvements were effective

### Root Cause
- Previous graph cleanup operations (prune-relations, cleanup-orphans) removed low-signal `mentions` relationships
- The mentions_ratio health check correctly reflects the cleaned state

### Solution
- No action required — this is the desired state
- Continue monitoring mentions_ratio in future acceptance tests
- Document as baseline for v1.0.65+

### Benefits
- N/A — already in ideal state

### How to Resolve
- No action needed
- Verify in next release: `sqlite-graphrag health --json | jaq '.mentions_ratio'` should remain near 0.0


## GAP-06 LOW: Test plan entity name convention not enforced

- Test ID: T116, T243 (reclassified from FAIL to PASS)
- Phase: F7, F20

### Problem
- Two acceptance tests used entity name `"sqlite-graphrag CLI"` (with space) for `graph traverse --from`
- The entity does not exist because GraphRAG normalizes all entity names to kebab-case (`sqlite-graphrag-cli`)
- Both tests returned exit 4 (entity not found) — correct binary behavior, incorrect test input
- The test plan was written with a non-kebab-case entity name

### Consequences
- False FAIL results in acceptance tests waste investigation time
- Test plans that reference entities by display name instead of DB name produce unreliable results
- The same error pattern appeared in two independent test phases (F7 and F20), executed by two different teammates

### Root Cause
- Entity names in GraphRAG are normalized to kebab-case during creation
- The acceptance test plan was written using the human-readable display name instead of the actual DB name
- No validation step in the test plan requires verifying entity existence before using it in `graph traverse`

### Solution
- Always use `graph entities --json | jaq -r '.entities[].name' | rg '<partial>'` to discover exact entity names before writing traverse tests
- Standardize entity name references in test plans to kebab-case
- Add a pre-flight check in F7/F20 test plans: verify entity exists before testing traverse

### Benefits
- Eliminates false FAIL results in entity-dependent tests
- Reduces investigation time during acceptance testing
- Enforces consistency between test plans and DB conventions

### How to Resolve
- Step 1: In future test plans, replace `"sqlite-graphrag CLI"` with `"sqlite-graphrag-cli"` (or discover dynamically)
- Step 2: Add pre-flight entity existence check in graph traverse test plans
- Step 3: Document convention in acceptance test methodology
- Effort: 5 minutes per test plan update
- Files: Test plan documents (not code)


## GAP-13 HIGH: No reclassify-relation command — applies_to dominates at 25.4% (644/2540)

- Severity: HIGH
- Phase: Post-acceptance graph quality audit

### Problem
- The production DB has 2540 relationships with this distribution:
  - `uses`: 681 (26.8%)
  - `applies_to`: 644 (25.4%)
  - `supports`: 480 (18.9%)
  - `depends_on`: 202 (8.0%)
  - `causes`: 152 (6.0%)
  - `fixes`: 130 (5.1%)
  - `tracked_in`: 128 (5.0%)
  - `follows`: 54 (2.1%)
  - `contradicts`: 31 (1.2%)
  - `replaces`: 29 (1.1%)
  - `related`: 8 (0.3%)
  - `mentions`: 1 (0.0%)
- `applies_to` is the second most common relation at 25.4%, but many are misclassified
- Analysis: 447/644 applies_to (69%) involve entities with "rules" or "rule" in the name — these are CORRECT (a rule applies-to a sub-topic)
- The remaining 197 (31%) are non-rules and include patterns like:
  - `incident→tool` (should be `causes` or `fixes`)
  - `concept→tool` (should be `uses` or `depends-on`)
  - `project→file` (should be `tracked-in`)
  - `concept→concept` without rules context (should be `supports`, `uses`, or `depends-on`)
- sqlite-graphrag has `reclassify` for ENTITY types but NO equivalent for RELATIONSHIP types
- The only workaround is `unlink --from A --to B --relation applies-to` + `link --from A --to B --relation uses` — 2 commands per reclassification
- For 197 relationships, this requires 394 CLI invocations

### Consequences
- Graph traversal returns low-signal paths because `applies_to` is overused as a default catch-all
- The `related` command and graph-expansion in `recall --with-graph` traverse `applies_to` edges equally, diluting relevance
- Evidence chains (GAP-09) include `applies_to` edges that should be stronger typed (e.g., `depends-on` carries more semantic weight than `applies-to`)
- LLMs using the graph for reasoning cannot distinguish "A applies to B" from "A depends on B" when both are labeled `applies_to`
- 34 applies_to relationships share the same (source_id, target_id) pair with another relation — reclassifying these would hit UNIQUE constraint without dedup
- The `health` command checks `mentions_ratio` (threshold 50%) but has NO check for `applies_to` concentration — the dominance goes undetected

### Root Cause
- The LLM extraction prompt (ingest --mode claude-code) and the CLAUDE.md mapping table both map `part-of` → `applies-to`
- The extraction regex (`src/extraction.rs:DEFAULT_RELATION = "mentions"`) was changed to reduce mentions, but `applies_to` absorbed many of those edges
- The CLAUDE.md rule "Mapeamento: part-of→applies-to" is too broad — `part-of` can mean `uses`, `depends-on`, or `supports` depending on context
- There is no CLI command to reclassify relationship types in batch
- The `reclassify` command was designed for entity types only — the same UX pattern was not extended to relationships

### Solution

#### Phase A: New CLI command `reclassify-relation` (v1.0.65)
- Add `reclassify-relation --from-relation <old> --to-relation <new> --batch --json`
- Single mode: `reclassify-relation --source <A> --target <B> --from-relation applies-to --to-relation uses --json`
- Batch mode: `reclassify-relation --from-relation applies-to --to-relation uses --batch --json` (reclassifies ALL applies_to → uses)
- Handle UNIQUE constraint: if (source_id, target_id, new_relation) already exists, merge and delete the old edge
- SQL: `UPDATE OR IGNORE relationships SET relation = ?1 WHERE relation = ?2 AND namespace = ?3; DELETE FROM relationships WHERE relation = ?2 AND namespace = ?3;`
- Add `--dry-run` to preview count before committing
- Add `--filter-source-type` and `--filter-target-type` for targeted reclassification (e.g., only incident→tool edges)

#### Phase B: Curated reclassification of 197 non-rules applies_to
- `reclassify-relation --from-relation applies-to --to-relation causes --filter-source-type incident --batch --json` (incidents cause things)
- `reclassify-relation --from-relation applies-to --to-relation tracked-in --filter-source-type project --filter-target-type file --batch --json`
- Manual review for concept→concept edges that need context-aware classification

#### Phase C: Health check for relation concentration
- Add `applies_to_ratio` and `top_relation_ratio` to `health --json` output
- Warning threshold: any single relation > 40% triggers `relation_concentration_warning`
- This prevents future accumulation going undetected

### Benefits
- Relationship reclassification becomes a single CLI command instead of 394 invocations
- Graph traversal quality improves: edges carry their actual semantic weight
- Evidence chains built from correctly typed edges produce meaningful narratives
- The `health` command detects relation dominance before it becomes a quality problem
- Parity with entity management: entities have `reclassify`, `rename-entity`, `merge-entities` — relationships get equivalent tooling

### How to Resolve
- Step 1: Add `ReclassifyRelation` variant to CLI enum in `src/cli.rs`
- Step 2: Implement `src/commands/reclassify_relation.rs` with `UPDATE OR IGNORE` + `DELETE` pattern (same as `merge-entities` for junction tables)
- Step 3: Add `--dry-run` and `--filter-source-type`/`--filter-target-type` flags
- Step 4: Add `--batch` flag for bulk reclassification
- Step 5: Add JSON response schema: `{action, from_relation, to_relation, count, merged_duplicates, namespace, elapsed_ms}`
- Step 6: Add relation concentration check to `health.rs` with threshold 40%
- Step 7: Execute Phase B curated reclassification on production DB
- Step 8: Create contract test `contract_37_reclassify_relation` and schema `reclassify-relation.schema.json`
- Effort: ~4 hours (command) + ~1 hour (health check) + ~1 hour (curated reclassification)
- Files: `src/cli.rs`, `src/commands/reclassify_relation.rs` (new), `src/commands/health.rs`, `docs/schemas/reclassify-relation.schema.json` (new)


## GAP-07 CRITICAL: Sub-queries are cosmetic — KNN uses single embedding for all

- Severity: CRITICAL
- Phase: Post-acceptance deep analysis

### Problem
- `deep-research` decomposes the query into up to 7 sub-queries (e.g., "A and B" becomes ["A", "B"])
- The embedding is computed ONCE using the ORIGINAL query at `src/commands/deep_research.rs:211`:
  ```rust
  let embedding = Arc::new(crate::daemon::embed_query_or_local(
      &paths.models,
      &args.query,  // <-- ORIGINAL QUERY, not sub-query
      args.daemon.autostart_daemon,
  )?);
  ```
- This single embedding is shared via `Arc` to ALL sub-query executions (L233: `let emb = Arc::clone(&embedding)`)
- Each `execute_sub_query` runs `knn_search(&conn, embedding, ...)` at L513 with the SAME vector
- Result: ALL sub-queries return the SAME KNN results — decomposition is purely cosmetic
- The `sub_query_ids` field on each result lists ALL sub-query IDs (e.g., `[0,1,2,3,4,5]`) because every sub-query found the same memories

### Consequences
- 0% diversification: 20/20 KNN results are identical to running `recall` with the original query
- Sub-queries like "professional profile" and "career transition" point to different regions of the embedding space, but since one embedding is shared, only the centroid region is explored
- Memories reachable by individual sub-queries (verified: 23 unique of 30 = 77% diversification when run separately) are invisible
- The `sub_queries` field in JSON output is misleading — implies parallel execution that did NOT happen for KNN
- Users deploying deep-research expecting multi-perspective retrieval get single-perspective retrieval with decoration

### Root Cause
- `src/commands/deep_research.rs:211-214`: `embed_query_or_local(&paths.models, &args.query, ...)` computes embedding for `args.query` (original), not for each sub-query text
- The embedding is stored in `Arc<Vec<f32>>` and cloned for all spawned tasks
- FTS5 search at L535 DOES use `query_text` (the sub-query text), but FTS results get score 0.5 (GAP-08) and are dominated by KNN results with scores 0.8-0.9
- The fan-out architecture (JoinSet, Semaphore) is correct — the bug is that the INPUT to each task is the same embedding

### Solution
- Compute a SEPARATE embedding for EACH sub-query text
- Replace L211-214 with a per-sub-query embedding computation inside the JoinSet::spawn closure
- Each `execute_sub_query` receives its own embedding vector derived from its own sub-query text
- The daemon connection can batch-embed if supported, or compute sequentially with the same model

### Benefits
- Each sub-query explores a DISTINCT region of the embedding space
- The 77% diversification observed in individual recall tests is captured
- Memories appearing in MULTIPLE sub-queries (convergence signal) rise naturally in the merged ranking
- The promise "parallel multi-hop research via query decomposition" becomes REAL

### How to Resolve
- Step 1: Move `embed_query_or_local` inside the JoinSet loop, computing one embedding per sub-query
- Step 2: Alternatively, batch-embed all sub-query texts before the loop: `let embeddings: Vec<Vec<f32>> = sub_query_texts.iter().map(|t| embed(t)).collect()`
- Step 3: Pass `&embeddings[idx]` instead of `&emb` to each `execute_sub_query`
- Step 4: Keep the Arc<Semaphore> pattern for concurrency control
- Step 5: Add test: run deep-research with "A and B" where A and B are semantically distant, verify results differ between sub-queries
- Effort: ~2 hours
- Files: `src/commands/deep_research.rs` (lines 206-266)
- Risk: Embedding computation is the slowest step (~50ms per query). With 7 sub-queries, latency increases from ~50ms to ~350ms. Acceptable tradeoff for correctness.


## GAP-08 CRITICAL: FTS5 results get hardcoded score 0.5 without RRF fusion

- Severity: CRITICAL
- Phase: Post-acceptance deep analysis

### Problem
- `execute_sub_query` at L534-551 runs FTS5 search and assigns a HARDCODED score of 0.5 to ALL FTS results:
  ```rust
  hits.push((row.id, 0.5, "fts".to_string(), snippet, row.body, None));
  ```
- KNN results have scores in range 0.8-0.95 (cosine similarity)
- FTS results with score 0.5 are always ranked BELOW KNN results
- No Reciprocal Rank Fusion (RRF) is applied — KNN and FTS are simply concatenated
- The existing `hybrid-search` command DOES implement RRF fusion correctly — deep-research ignores this

### Consequences
- Memories findable by exact term matching (names, acronyms, codes like "NR-01", "ICMS") are ranked below semantically similar but lexically different memories
- Average loss: 2-4 memories per query compared to `hybrid-search`
- For query "PDCA SDCA Falconi": missed pdca-manual-indg-falconi-aquila-completo (the COMPLETE PDCA manual)
- For query "NR-01 riscos psicossociais": missed nr01-guia-mte-22-perguntas (official MTE guide)
- The FTS5 signal (BM25 ranking, exact term matching) is wasted — assigned a meaningless constant

### Root Cause
- `src/commands/deep_research.rs:541`: `0.5` is a placeholder constant, not a computed score
- The `hybrid-search` command uses `memories::hybrid_search()` which fuses KNN + FTS via RRF internally
- `deep-research` was built on the lower-level primitives (`knn_search` + `fts_search`) directly instead of reusing `hybrid_search`
- Probable reason: `hybrid_search` was not easy to parallelize as a single function call, so the developer split KNN and FTS manually but forgot to implement fusion

### Solution
- Option A (preferred): Replace separate `knn_search` + `fts_search` calls with a single `hybrid_search` call per sub-query, reusing the existing RRF implementation
- Option B: Keep separate calls but implement RRF fusion inside `execute_sub_query`: assign FTS results a rank-based score via `1.0 / (rrf_k + fts_rank)` and normalize against KNN scores

### Benefits
- Captures BOTH signals: semantic (embedding) and lexical (exact terms)
- Eliminates 2-4 memory loss per query observed in testing
- For queries with proper nouns, acronyms, or codes, improvement is dramatic
- Parity with `hybrid-search` quality while adding decomposition and graph traversal

### How to Resolve
- Step 1: In `execute_sub_query`, replace L512-551 with a call to `memories::hybrid_search()` (or equivalent internal function)
- Step 2: If hybrid_search is not accessible at the storage layer, implement RRF inline: sort FTS results by BM25 rank, compute `fts_rrf_score = 1.0 / (60.0 + rank)`, combine with KNN scores
- Step 3: Remove the hardcoded `0.5` constant
- Step 4: Add `--rrf-k` flag to deep-research (default 60, same as hybrid-search)
- Effort: ~2 hours
- Files: `src/commands/deep_research.rs` (lines 512-551)


## GAP-09 HIGH: Evidence chains are a flat dump of global relationships, not directed paths

- Severity: HIGH
- Phase: Post-acceptance deep analysis

### Problem
- Evidence chain construction at L594-626 runs a SQL query that fetches the top 20 relationships from the ENTIRE namespace:
  ```sql
  SELECT se.name, te.name, r.relation, r.weight
  FROM relationships r
  JOIN entities se ON se.id = r.source_id
  JOIN entities te ON te.id = r.target_id
  WHERE r.namespace = ?1 AND r.weight >= ?2
  ORDER BY r.weight DESC
  LIMIT 20
  ```
- This query is NOT filtered by the entities found in the current sub-query results
- It returns the top 20 relationships by weight across ALL entities in the namespace
- Result: evidence chains contain disconnected pairs like "peter-levine --[causes]--> somatic-experiencing" followed by "bain-company --[causes]--> piramide-dos-elementos-de-valor"
- There is no path reconstruction — it is a flat list of (entity, relation, weight) tuples

### Consequences
- Evidence chains have zero narrative value — disconnected pairs cannot support reasoning
- An LLM receiving this chain cannot construct causal or logical arguments
- The chain occupies context tokens without providing signal
- Worse than useless: it is MISLEADING — suggests an evidence chain that does not exist
- No human or LLM can extract insight from "Peter Levine causes Somatic Experiencing THEREFORE Bain Company causes Value Elements Pyramid"

### Root Cause
- `src/commands/deep_research.rs:596-602`: The SQL query uses `WHERE r.namespace = ?1 AND r.weight >= ?2` — no filter on entity IDs from the current search results
- L612-626: Each row produces TWO `EvidenceNode` entries (source with relation+weight, target with None/None) — this is a flat pair dump, not a path
- The dedup at L363 uses entity names joined by "->" as key, which prevents duplicate chains but does not create real paths
- The algorithm was designed to collect visited edges, not to reconstruct directed paths from source to destination

### Solution
- Replace the global relationship dump with path reconstruction between KNN seeds and graph results
- For each pair (seed_memory, graph_result), reconstruct the shortest path using the BFS data already computed by `traverse_from_memories_with_hops`
- Output: multiple short chains (3-5 nodes each), each connecting ONE seed to ONE graph result with typed relations
- Discard 1-hop chains (trivial, no informational value)

### Benefits
- Each chain tells ONE connection story with beginning, middle, and end
- LLM can use chains for chained reasoning: "A applies-to B, which depends-on C, which causes D"
- Short chains (3-5 nodes) are digestible in limited context
- Multiple chains enable evidence triangulation

### How to Resolve
- Step 1: After graph traversal, for each `graph_match` with `hop_distance > 1`, call `find_shortest_path(seed_entity, target_entity)` using BFS on the already-loaded subgraph
- Step 2: Filter the SQL to only include entities connected to the current search results: `WHERE r.source_id IN (...discovered_entity_ids...) OR r.target_id IN (...discovered_entity_ids...)`
- Step 3: Return `evidence_chains: Vec<Chain>` where each Chain has `from`, `to`, `path: Vec<(entity, relation, weight)>`, `total_weight`
- Step 4: Sort by `total_weight` descending, limit to top N chains
- Effort: ~4 hours (most complex fix)
- Files: `src/commands/deep_research.rs` (lines 594-626), potentially `src/graph.rs`


## GAP-10 HIGH: Embedding centroid collapse — multi-dimensional queries return single dimension

- Severity: HIGH
- Phase: Post-acceptance deep analysis

### Problem
- For a multi-dimensional query like "Danilo identity, career, therapy, business, technology, spirituality", the single embedding is the CENTROID of all 6 dimensions
- The centroid is dominated by the dimension with the densest cluster of memories in the embedding space
- In a DB with ~50+ therapy/emotions memories, the centroid falls in the therapy cluster
- ALL 20 KNN results come from the therapy dimension — zero results from business, technology, strategy, spirituality

### Consequences
- A query about "who is Danilo" returns ONLY the emotional/therapeutic Danilo
- The entrepreneur (pharmacies), technical (Rust/Claude Code), strategist (Falconi), spiritual Danilo — ALL invisible
- For an LLM building a complete portrait, 5/6 of the information is missing
- The more dimensions a query has, the WORSE the centroid collapse — the centroid moves toward the densest cluster

### Root Cause
- This is a DIRECT consequence of GAP-07 (single embedding for all sub-queries)
- `multilingual-e5-small` generates ONE 384-dimensional vector for the composite query
- The vector is the centroid of all semantic dimensions in the query
- KNN finds the 20 nearest neighbors of the centroid, which are ALL from the densest cluster
- Minority dimensions (Rust, pharmacy, Falconi) are far from the centroid and eliminated

### Solution
- This is AUTOMATICALLY resolved when GAP-07 is fixed (separate embeddings per sub-query)
- Each sub-query generates its OWN embedding pointing to its OWN region of the vector space
- `recall "Danilo therapy"` → emotional cluster
- `recall "Danilo pharmacy ICMS"` → business cluster
- `recall "Danilo Rust Claude Code"` → technical cluster
- The union with RRF re-ranking ensures multi-dimensional coverage

### Benefits
- Each dimension of the query is represented in results
- Complete portrait: emotional + business + technical + spiritual + strategic
- Eliminates the dense cluster bias that dominates the centroid

### How to Resolve
- Resolved automatically by fixing GAP-07 (per-sub-query embeddings)
- Additional improvement: if >80% of results come from the same cluster (measured by average cosine distance between results < 0.1), force re-decomposition with discriminating terms
- Effort: 0 additional (included in GAP-07 fix)
- Files: Same as GAP-07
- Dependency: GAP-07


## GAP-11 MEDIUM: No RRF fusion between KNN pool and graph pool in final ranking

- Severity: MEDIUM
- Phase: Post-acceptance deep analysis

### Problem
- Final results are CONCATENATED: KNN results (scores 0.87-0.92) + graph results (scores 0.25-0.5)
- No fusion or re-ranking considers BOTH signals
- KNN result with score 0.87 always ranks above a graph result with score 0.5, even if the graph result is more relevant (connected by a strong edge to a high-scoring seed)
- The `results[]` field mixes two incomparable score scales

### Consequences
- Consumers must implement their OWN ranking logic
- Sorting by `score` descending puts ALL KNN results above ALL graph results
- A graph result discovered at hop 1 from a 0.95-score seed with 0.9 edge weight should score higher than a KNN result at 0.87 — but it gets 0.5

### Root Cause
- KNN returns `1.0 - cosine_distance` (range 0.0-1.0, continuous, meaningful)
- Graph returns `1.0 - 1.0/(hop+1.0)` — a geometric decay that produces 0.5, 0.33, 0.25 for hops 1, 2, 3
- The graph score does NOT incorporate the seed's score or the edge weight
- No RRF or normalization step combines the two pools

### Solution
- Apply Reciprocal Rank Fusion (RRF) between KNN and graph pools, mirroring how `hybrid-search` fuses KNN and FTS5
- Graph score should incorporate seed context: `score = seed_score * decay^hop * edge_weight`
- Example: seed score 0.92, edge weight 0.8, hop 2 with decay 0.7 → 0.92 * 0.49 * 0.8 = 0.36
- Apply minimum score threshold (e.g., 0.2) to filter noise automatically

### Benefits
- Single meaningful score per result — consumers sort by `score` descending
- Graph results connected by strong edges to high-scoring seeds compete fairly with KNN
- Noise filtering via threshold reduces precision from 18% to ~60%+ (estimated)

### How to Resolve
- Step 1: Create `fn score_graph_result(seed_score: f64, hop: u32, edge_weight: f64, decay: f64) -> f64`
- Step 2: Apply when constructing graph hits at L575-591
- Step 3: After collecting all hits, apply RRF between KNN-ranked and graph-ranked pools
- Step 4: Add `--graph-decay` (default 0.7) and `--graph-min-score` (default 0.2) as optional flags
- Effort: ~2 hours
- Files: `src/commands/deep_research.rs` (lines 575-591, 296-336)


## GAP-12 LOW: FTS5 uses sub-query text but KNN uses original embedding — partial decomposition

- Severity: LOW (consequence of GAP-07, documented for completeness)
- Phase: Post-acceptance deep analysis

### Problem
- In `execute_sub_query`, FTS5 search at L535 uses `query_text` (the sub-query text) — this IS per-sub-query
- KNN search at L513 uses `embedding` (the shared original query embedding) — this is NOT per-sub-query
- Result: FTS5 provides marginal diversification (different text terms per sub-query) but KNN dominates with score 0.8-0.9 vs FTS5's fixed 0.5
- The asymmetry means decomposition is ~10% effective (FTS only) instead of 100% effective (KNN + FTS)

### Consequences
- Minor — FTS5 does add some per-sub-query diversity, but the impact is drowned by KNN's higher scores
- Not a separate bug — it is a direct consequence of GAP-07 and GAP-08

### Root Cause
- Same as GAP-07 (single embedding) and GAP-08 (FTS5 score 0.5)

### Solution
- Resolved automatically by fixing GAP-07 (per-sub-query embeddings) and GAP-08 (RRF fusion)
- No separate action needed

### How to Resolve
- No additional effort — included in GAP-07 and GAP-08 fixes
- Dependency: GAP-07, GAP-08


## GAP-14 CRITICAL: No LLM-augmented graph quality pipeline — 95.4% orphan memories, 71.1% entities without descriptions

- Severity: CRITICAL
- Phase: Post-acceptance graph quality audit

### Problem — Production DB Quality Metrics (verified 2026-05-28)
- 917/961 memories (95.4%) have ZERO entity bindings — orphan memories invisible to graph traversal
- 1438/2024 entities (71.1%) have NULL or empty description — entities are opaque labels without semantic context
- 1795/2024 entities (88.7%) have degree ≤ 3 — inert nodes with minimal connectivity
- 1723/2543 relationships (67.8%) have weight ≥ 0.7 — inflated weights destroy ranking signal
- Average weight: 0.698 — should be ~0.5 for calibrated distribution
- 644/2543 relationships (25.4%) are `applies_to` — over-used as catch-all (GAP-13)
- The CLI has `ingest --mode claude-code` and `--mode codex` for NEW file ingestion
- BUT there is NO equivalent pipeline for ENRICHING or AUDITING existing memories and entities
- All 12 quality operations identified below require manual CLI orchestration with 3+ commands each

### Consequences
- Graph traversal (`related`, `graph traverse`, `deep-research --with-graph`) returns near-zero results because 95.4% of memories have no entity bindings
- `recall` and `hybrid-search` find memories, but the graph CANNOT expand them — the multi-hop promise of GraphRAG is broken
- Entity KNN search (`entities::knn_search`) fails for 71.1% of entities because descriptions are NULL — the entity embedding is computed from the name alone
- Edge weight ranking is meaningless: 67.8% of edges have weight ≥ 0.7, so sorting by weight gives near-uniform results
- LLM agents consuming graph data get opaque entity names without context
- The gap between "memory found by vector search" and "memory discoverable by graph traversal" is 95.4%

### Root Cause
- `ingest --mode claude-code` extracts entities and relationships for NEW files being ingested
- There is NO `enrich` or `augment` command for EXISTING memories already in the DB
- Memories ingested before v1.0.60 (when `--mode claude-code` was added) have zero entity bindings
- Memories ingested with `--mode none` (default) or plain `remember` also have zero bindings
- Entity descriptions are optional in all creation paths and are rarely provided
- Weight calibration is subjective and inconsistent across sessions — no LLM validation step exists
- The CLI lacks a "graph quality pipeline" that orchestrates: scan → LLM judgment → persist corrections

### Solution — New CLI command: `enrich`

A new `sqlite-graphrag enrich` command that implements the universal 3-step pattern:
```
graphrag (search/export) → LLM headless (judgment) → graphrag (persist)
```

#### Sub-command structure

```
sqlite-graphrag enrich --mode <claude-code|codex> [OPTIONS] --json
```

#### 12 enrichment operations (all via `--operation` flag)

| Operation | Problem | Scope | LLM Task |
|---|---|---|---|
| `entity-descriptions` | 1438 entities without descriptions | Per entity | Read linked memory body, generate 15-word description |
| `memory-bindings` | 917 orphan memories (95.4%) | Per memory | Read body, extract entities + relationships |
| `relation-reclassify` | 644 applies_to over-used | Per relationship | Evaluate entity pair, suggest precise relation type |
| `weight-calibrate` | 67.8% edges weight ≥ 0.7 | Per relationship | Evaluate if weight matches calibration scale |
| `entity-connect` | 1795 entities degree ≤ 3 | Per entity | Recall neighbors, judge real connections |
| `entity-type-validate` | Unknown % with wrong types | Per entity | Evaluate if entity_type is correct |
| `description-enrich` | 19 memories with short descriptions | Per memory | Read body, generate semantic description |
| `cross-domain-bridges` | Isolated domain subgraphs | Per domain pair | Identify real connections between domains |
| `domain-classify` | Entities without domain assignment | Per entity | Classify into life domain (juridica, farmacia, digital, etc.) |
| `graph-audit` | Overall quality unknown | Global | Audit suspicious relations, noise entities, missing connections |
| `deep-research-synth` | Raw search results without synthesis | Per query | Synthesize cross-domain insights from deep-research output |
| `body-extract` | Large memory bodies without extraction | Per memory | Extract entities from long-form content (sessions, manuals) |

#### Architecture

```
enrich --operation memory-bindings --limit 50 --mode claude-code --json

Phase 1 — SCAN (graphrag pure):
  list memories with zero entity bindings (SQL: LEFT JOIN memory_entities IS NULL)
  read body for each (--limit controls batch size)

Phase 2 — JUDGE (LLM headless):
  For each memory, spawn claude -p / codex exec with:
    - Body content via stdin
    - Extraction prompt with structured output schema
    - --json-schema / --output-schema with entities[] + relationships[]
    - --max-turns 3, --no-session-persistence, --settings '{"hooks":{}}'

Phase 3 — PERSIST (graphrag):
  remember --name <name> --force-merge --graph-stdin --json
  (creates entity bindings without duplicating the memory)
```

#### LLM provider flags (reuse from ingest --mode claude-code)
- `--claude-binary`, `--claude-model`, `--claude-timeout` for Claude Code
- `--codex-binary`, `--codex-model`, `--codex-timeout` for Codex CLI
- `--max-cost-usd` for budget cap (OAuth users: ignored with warning)
- `--resume` and `--retry-failed` for crash resilience (queue DB pattern)
- `--dry-run` for preview without LLM invocation

#### Example pipelines using EXISTING CLI (workaround until `enrich` is implemented)

**1. Entity descriptions** (per entity, ~50ms LLM):
```bash
# Scan: find entities without descriptions
sqlite3 graphrag.sqlite "SELECT name FROM entities WHERE namespace='global' AND (description IS NULL OR description='')" | \
while read -r entity; do
  # Judge: LLM generates description
  DESC=$(sqlite-graphrag memory-entities --entity "$entity" --json 2>/dev/null | \
    claude -p "Generate a 15-word description for entity '$entity' based on linked memories" \
      --output-format json --json-schema '{"type":"object","properties":{"description":{"type":"string"}},"required":["description"],"additionalProperties":false}' \
      --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}' 2>/dev/null | \
    jaq -r '.result.description // .description // empty')
  # Persist: update entity description
  [ -n "$DESC" ] && sqlite-graphrag reclassify --name "$entity" --description "$DESC" --json 2>/dev/null
done
```

**2. Memory bindings** (per memory, ~200ms LLM):
```bash
# Scan: find orphan memories
sqlite3 graphrag.sqlite "SELECT m.name FROM memories m LEFT JOIN memory_entities me ON m.id=me.memory_id WHERE m.namespace='global' AND m.deleted_at IS NULL AND me.memory_id IS NULL LIMIT 50" | \
while read -r mem; do
  # Judge: LLM extracts entities and relationships
  sqlite-graphrag read --name "$mem" --json 2>/dev/null | \
    claude -p "Extract domain entities and typed relationships from this memory body" \
      --output-format json --json-schema '<EXTRACTION_SCHEMA>' \
      --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}' 2>/dev/null > /tmp/enrichment.json
  # Persist: force-merge with graph data
  jaq '{body: "", entities, relationships}' /tmp/enrichment.json | \
    sqlite-graphrag remember --name "$mem" --force-merge --graph-stdin --json 2>/dev/null
done
```

**3. Weight calibration** (per edge, ~30ms LLM):
```bash
# Scan: find inflated weights
sqlite3 graphrag.sqlite "SELECT se.name, te.name, r.relation, r.weight FROM relationships r JOIN entities se ON se.id=r.source_id JOIN entities te ON te.id=r.target_id WHERE r.namespace='global' AND r.weight >= 0.9 LIMIT 50" | \
while IFS='|' read -r src tgt rel weight; do
  # Judge: LLM evaluates calibration
  NEW_WEIGHT=$(claude -p "Evaluate weight for '$src' --[$rel]--> '$tgt' (current: $weight). Scale: 0.9=vital dependency, 0.7=design, 0.5=context, 0.3=weak" \
    --output-format json --json-schema '{"type":"object","properties":{"calibrated_weight":{"type":"number","minimum":0,"maximum":1}},"required":["calibrated_weight"],"additionalProperties":false}' \
    --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}' 2>/dev/null | \
    jaq -r '.result.calibrated_weight // .calibrated_weight // empty')
  # Persist: update weight via unlink + link
  [ -n "$NEW_WEIGHT" ] && sqlite-graphrag unlink --from "$src" --to "$tgt" --relation "$rel" --json 2>/dev/null && \
    sqlite-graphrag link --from "$src" --to "$tgt" --relation "$rel" --weight "$NEW_WEIGHT" --json 2>/dev/null
done
```

### Benefits
- Transforms the DB from 95.4% orphan memories to near-zero — every memory becomes graph-traversable
- Entity descriptions enable semantic entity KNN search — improving graph expansion quality
- Calibrated weights restore ranking signal — `weight` becomes meaningful instead of uniform
- The `enrich` command reuses the proven `ingest --mode claude-code` architecture: queue DB, resume/retry, cost tracking, structured output
- All 12 operations follow the SAME 3-step pattern — one command, one architecture
- LLM provider choice (Claude vs Codex) per operation — use Claude for nuanced judgment, Codex for fast quantitative tasks

### How to Resolve

#### Phase A: Implement `enrich` command skeleton (~4h)
- Step 1: Add `Enrich` variant to CLI enum in `src/cli.rs` with `--operation`, `--mode`, `--limit`, `--dry-run` flags
- Step 2: Create `src/commands/enrich.rs` with scan→judge→persist pipeline
- Step 3: Reuse `ingest_claude.rs` subprocess architecture: `env_clear()`, `wait_timeout`, `--settings '{"hooks":{}}'`, OAuth detection
- Step 4: Reuse queue DB pattern from `ingest_claude.rs` for resume/retry

#### Phase B: Implement priority operations (~8h)
- Step 5: `memory-bindings` operation (highest impact: 917 orphan memories)
- Step 6: `entity-descriptions` operation (1438 entities)
- Step 7: `weight-calibrate` operation (1723 inflated edges)
- Step 8: `relation-reclassify` operation (integrates with GAP-13)

#### Phase C: Implement remaining operations (~6h)
- Step 9: `entity-connect`, `entity-type-validate`, `description-enrich`
- Step 10: `cross-domain-bridges`, `domain-classify`, `graph-audit`
- Step 11: `deep-research-synth`, `body-extract`

#### Phase D: Validation (~2h)
- Step 12: Run `enrich --operation memory-bindings --limit 50 --dry-run` to validate pipeline
- Step 13: Run real enrichment on 50 memories, verify with `memory-entities` and `graph traverse`
- Step 14: Measure: orphan rate should drop from 95.4% to <50% after processing 917 memories
- Step 15: Create contract test `contract_38_enrich` and schema `enrich.schema.json`

#### Estimated impact on production DB
| Metric | Before | After (estimated) |
|---|---|---|
| Orphan memories | 95.4% (917/961) | <5% |
| Entities without description | 71.1% (1438/2024) | <10% |
| Entities degree ≤ 3 | 88.7% (1795/2024) | <40% |
| Edges weight ≥ 0.7 | 67.8% (1723/2543) | ~35% |
| Average weight | 0.698 | ~0.55 |
| applies_to dominance | 25.4% | <15% |

- Effort: ~20h total (skeleton + priority ops + remaining + validation)
- Files: `src/cli.rs`, `src/commands/enrich.rs` (new), `docs/schemas/enrich.schema.json` (new)
- Dependencies: GAP-13 (reclassify-relation) for relation-reclassify operation

### Appendix: 12 Concrete LLM Pipeline Recipes (workaround until `enrich` is implemented)

All 12 recipes follow the SAME 3-step pattern:
- Step 1 SCAN: sqlite-graphrag exports data (read, list, graph, recall, memory-entities)
- Step 2 JUDGE: claude -p or codex exec receives data via stdin, returns structured JSON
- Step 3 PERSIST: sqlite-graphrag applies the LLM decision (reclassify, link, remember --force-merge, edit)

Common Claude flags: `--output-format json --json-schema '<SCHEMA>' --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'`
Common Codex flags: `--json --output-schema /tmp/<name>.json --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules -`

Key differences between providers:
- Claude: prompt via `-p`, schema INLINE via `--json-schema 'STRING'`, output is JSON array with `structured_output` field
- Codex: prompt + data go TOGETHER via stdin (`-`), schema in FILE via `--output-schema /tmp/file.json`, output is JSONL with `text` in last `agent_message`

#### Recipe 1: Entity Descriptions (1438 entities, ~50ms/entity LLM)
```bash
# SCAN
sqlite-graphrag memory-entities --entity "$ENTITY" --json | jaq -r '.memories[0].name' | \
  xargs -I{} sqlite-graphrag read --name {} --json
# JUDGE
| claude -p "Given entity '$ENTITY', write ONE 15-word description" --output-format json \
  --json-schema '{"type":"object","properties":{"description":{"type":"string"}},"required":["description"],"additionalProperties":false}' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
# PERSIST
sqlite-graphrag reclassify --name "$ENTITY" --description "$DESC" --json
```

#### Recipe 2: Memory Entity Bindings (917 orphan memories, ~200ms/memory LLM)
```bash
# SCAN
sqlite-graphrag read --name "$MEM" --json
# JUDGE (extraction schema identical for Claude and Codex)
| claude -p "Extract domain entities and typed relationships from this memory body" --output-format json \
  --json-schema '{"type":"object","properties":{"body":{"type":"string"},"entities":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"},"entity_type":{"type":"string","enum":["project","tool","person","file","concept","incident","decision","organization","location","date"]}},"required":["name","entity_type"],"additionalProperties":false}},"relationships":{"type":"array","items":{"type":"object","properties":{"source":{"type":"string"},"target":{"type":"string"},"relation":{"type":"string","enum":["applies-to","uses","depends-on","causes","fixes","contradicts","supports","follows","related","replaces","tracked-in"]},"strength":{"type":"number","minimum":0,"maximum":1}},"required":["source","target","relation","strength"],"additionalProperties":false}}},"required":["body","entities","relationships"],"additionalProperties":false}' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
# PERSIST (--graph-stdin accepts SAME JSON from both Claude and Codex)
| sqlite-graphrag remember --name "$MEM" --force-merge --graph-stdin --json
```

#### Recipe 3: Relation Reclassification (644 applies_to edges, ~30ms/edge LLM)
```bash
# SCAN
sqlite-graphrag graph --format json  # or targeted SQL query
# JUDGE
claude -p "From: $SRC (type: $STYPE) To: $TGT (type: $TTYPE) Current: applies_to weight $W. What is the REAL relation? Scale: 0.9=vital, 0.7=design, 0.5=context, 0.3=weak" \
  --output-format json --json-schema '{"type":"object","properties":{"relation":{"type":"string","enum":["uses","depends-on","causes","supports","follows","tracked-in","fixes","contradicts","replaces","applies-to"]},"strength":{"type":"number","minimum":0,"maximum":1},"reasoning":{"type":"string"}},"required":["relation","strength","reasoning"],"additionalProperties":false}' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
# PERSIST (unlink old + link new)
sqlite-graphrag unlink --from "$SRC" --to "$TGT" --relation applies-to --json
sqlite-graphrag link --from "$SRC" --to "$TGT" --relation "$NEW_REL" --weight "$NEW_W" --json
```

#### Recipe 4: Weight Calibration (1723 inflated edges, ~30ms/edge LLM)
```bash
# JUDGE (no scan needed — data fits in prompt)
claude -p "From: $SRC To: $TGT Relation: $REL Weight: $W. Scale: 0.9=vital dependency no alternative, 0.7=important design, 0.5=useful context, 0.3=weak reference. Is the weight correct?" \
  --output-format json --json-schema '{"type":"object","properties":{"calibrated_weight":{"type":"number","minimum":0,"maximum":1},"reasoning":{"type":"string"}},"required":["calibrated_weight","reasoning"],"additionalProperties":false}' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
# PERSIST
sqlite-graphrag unlink --from "$SRC" --to "$TGT" --relation "$REL" --json
sqlite-graphrag link --from "$SRC" --to "$TGT" --relation "$REL" --weight "$NEW_W" --json
```

#### Recipe 5: Inert Entity Connection (1795 entities degree ≤ 3, ~100ms/entity LLM)
```bash
# SCAN (multi-step: recall → memory-entities for candidates)
sqlite-graphrag recall "$ENTITY" --k 5 --json | jaq -r '.results[].name' | \
  while read -r mem; do sqlite-graphrag memory-entities --name "$mem" --json; done
# JUDGE
claude -p "Target: $ENTITY (type: $TYPE, degree 3). Candidates: $CANDIDATES. Which have REAL connections? Max 3" \
  --output-format json --json-schema '{"type":"object","properties":{"connections":{"type":"array","items":{"type":"object","properties":{"target":{"type":"string"},"relation":{"type":"string","enum":["uses","depends-on","causes","supports","follows","tracked-in","applies-to"]},"strength":{"type":"number","minimum":0,"maximum":1}},"required":["target","relation","strength"],"additionalProperties":false},"maxItems":3}},"required":["connections"],"additionalProperties":false}' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
# PERSIST (one link per connection)
sqlite-graphrag link --from "$ENTITY" --to "$TARGET" --relation "$REL" --weight "$W" --json
```

#### Recipe 6: Long Body Extraction (large memories without entities, ~500ms/memory LLM)
```bash
# SCAN
sqlite-graphrag read --name "$MEM" --json
# JUDGE (Claude preferred for symbolic/metaphorical content)
| claude -p "Extract domain entities and relationships from this content. Entities in kebab-case. NEVER use mentions. Strengths: 0.9 vital, 0.7 design, 0.5 context, 0.3 weak" \
  --output-format json --json-schema '<EXTRACTION_SCHEMA>' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
# PERSIST
| sqlite-graphrag remember --name "$MEM" --force-merge --graph-stdin --json
```

#### Recipe 7: Domain Classification (per entity, ~30ms/entity LLM)
```bash
# JUDGE (no scan — entity name + domain list fits in prompt)
claude -p "Classify entity '$ENTITY' into life domain: juridica, espiritual, farmacia, digital, aquila, financeira, familiar, saude-mental, nenhum" \
  --output-format json --json-schema '{"type":"object","properties":{"domain":{"type":"string","enum":["juridica","espiritual","farmacia","digital","aquila","financeira","familiar","saude-mental","nenhum"]},"confidence":{"type":"number","minimum":0,"maximum":1},"reasoning":{"type":"string"}},"required":["domain","confidence","reasoning"],"additionalProperties":false}' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
# PERSIST
sqlite-graphrag link --from "danilo-vida-$DOMAIN" --to "$ENTITY" --relation applies-to --weight "$CONF" --json
```

#### Recipe 8: Cross-Domain Bridge Detection (per domain pair, ~100ms/pair LLM)
```bash
# SCAN
sqlite-graphrag related "$DOMAIN_A" --hops 1 --json  # entities in domain A
sqlite-graphrag related "$DOMAIN_B" --hops 1 --json  # entities in domain B
# JUDGE
claude -p "Domain A ($DOMAIN_A) entities: $ENTS_A. Domain B ($DOMAIN_B) entities: $ENTS_B. Identify max 5 REAL connections" \
  --output-format json --json-schema '{"type":"object","properties":{"bridges":{"type":"array","items":{"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"},"relation":{"type":"string","enum":["causes","supports","contradicts","depends-on","fixes","applies-to"]},"strength":{"type":"number","minimum":0,"maximum":1},"reasoning":{"type":"string"}},"required":["from","to","relation","strength","reasoning"],"additionalProperties":false},"maxItems":5}},"required":["bridges"],"additionalProperties":false}' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
# PERSIST (one link per bridge)
sqlite-graphrag link --from "$FROM" --to "$TO" --relation "$REL" --weight "$W" --json
```

#### Recipe 9: Memory Description Enrichment (19 memories with short desc, ~50ms/memory LLM)
```bash
# SCAN
sqlite-graphrag read --name "$MEM" --json
# JUDGE
| claude -p "Write ONE 15-20 word sentence answering: what is this memory about and WHY does it matter?" \
  --output-format json --json-schema '{"type":"object","properties":{"description":{"type":"string"}},"required":["description"],"additionalProperties":false}' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
# PERSIST
sqlite-graphrag edit --name "$MEM" --description "$DESC" --json
```

#### Recipe 10: Graph Quality Audit (global, ~2s LLM)
```bash
# SCAN (for large graphs, filter first)
sqlite-graphrag graph --format json | \
# JUDGE
  claude -p "Analyze graph quality: 5 suspicious relations, 5 noise entities, 3 missing connections, quality score 0-100" \
  --output-format json --json-schema '<AUDIT_SCHEMA>' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
# PERSIST (per finding: unlink/link/delete-entity as needed)
```

#### Recipe 11: Deep Research Synthesis (per query, ~1s LLM)
```bash
# SCAN
sqlite-graphrag deep-research "$QUERY" --k 20 --with-bodies --json | \
# JUDGE
  claude -p "Synthesize: what are the REAL cross-domain connections? Connect the evidence" \
  --output-format json --json-schema '<SYNTHESIS_SCHEMA>' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
# No persistence — synthesis is consumed directly by human/LLM
```

#### Recipe 12: Entity Type Validation (batch of 5-10, ~50ms/batch LLM)
```bash
# SCAN
sqlite-graphrag graph entities --json | jaq '.entities[] | select(.type == "concept") | {name, type}' | head -10
# JUDGE
claude -p "Evaluate entity types: 1. $E1 (current: $T1) 2. $E2 (current: $T2)..." \
  --output-format json --json-schema '{"type":"object","properties":{"evaluations":{"type":"array","items":{"type":"object","properties":{"entity":{"type":"string"},"current_type":{"type":"string"},"correct_type":{"type":"string","enum":["project","tool","person","file","concept","incident","decision","organization","location","date"]},"needs_change":{"type":"boolean"},"reasoning":{"type":"string"}},"required":["entity","current_type","correct_type","needs_change","reasoning"],"additionalProperties":false}}},"required":["evaluations"],"additionalProperties":false}' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
# PERSIST (per entity that needs change)
sqlite-graphrag reclassify --name "$ENTITY" --new-type "$CORRECT_TYPE" --json
```

#### Provider Selection Guide

| Operation | Preferred Provider | Reason |
|---|---|---|
| Entity descriptions | Either | Short input/output, equal quality |
| Memory bindings | Claude | Better at domain-specific extraction from long bodies |
| Relation reclassify | Either | Short input, structured judgment |
| Weight calibration | Either | Short input, quantitative judgment |
| Entity connection | Claude | Requires semantic reasoning about relationships |
| Long body extraction | Claude | Better at symbolic/metaphorical content interpretation |
| Domain classification | Either | Short input, categorical output |
| Cross-domain bridges | Claude | Requires cross-domain reasoning |
| Description enrichment | Either | Short input/output |
| Graph audit | Claude | Requires qualitative reasoning about graph structure |
| Deep research synthesis | Claude | Requires narrative synthesis |
| Entity type validation | Codex | Fast quantitative classification |


## GAP-15 HIGH: Entity names not normalized on input — 301/2030 (14.8%) non-conforming, 11 duplicate pairs

- Severity: HIGH (upgraded from MEDIUM — duplicates confirmed)
- Phase: Post-acceptance graph quality audit

### Problem — Verified on Production DB (2026-05-28)
- `validate_entity_name()` at `src/storage/entities.rs:51-72` only checks: min 2 chars, no newlines, no short ALL_CAPS
- It does NOT enforce: lowercase, no spaces, no accents, kebab-case
- Entity names are inserted AS-IS into the DB
- Result: 4 naming patterns coexist:

| Pattern | Count | % | Example |
|---|---|---|---|
| Clean lowercase kebab-case | 1729 | 85.2% | `pdca-ciclo`, `sqlite-graphrag-cli` |
| Has uppercase | 243 | 12.0% | `BERT NER`, `Claude Code`, `AGENTS.pt-BR.md` |
| Has spaces | 34 | 1.7% | `Green Belt`, `Danilo Aguiar`, `WAL checkpoint TRUNCATE` |
| Has underscores | 123 | 6.1% | `DAEMON_FORCE_AUTOSTART`, `CARGO_BIN_EXE_` |
| **Total non-conforming** | **301** | **14.8%** | — |

### Confirmed Duplicate Pairs (11 pairs, 22 entities)
Same entity exists in BOTH normalized and non-normalized forms:

| Non-normalized | Normalized | Problem |
|---|---|---|
| `Claude Code` | `claude-code` | Space + uppercase |
| `Danilo Aguiar` | `danilo-aguiar` | Space + uppercase |
| `BERT NER` | `bert-ner` | Space + ALL_CAPS |
| `Gemini CLI` | `gemini-cli` | Space + uppercase |
| `WAL checkpoint TRUNCATE` | `wal-checkpoint-truncate` | Space + mixed case |
| `JSON Error Envelope` | `json-error-envelope` | Space + uppercase |
| `GraphRAG memory quality` | `graphrag-memory-quality` | Space + uppercase |
| `deep-research command` | `deep-research-command` | Space (partial) |
| `documentation audit` | `documentation-audit` | Space |
| `export subcommand` | `export-subcommand` | Space |
| `CANONICAL_RELATIONS` | `canonical-relations` | Underscore + ALL_CAPS |

### Consequences
- Graph traversal treats `Claude Code` and `claude-code` as SEPARATE entities with SEPARATE relationships
- Relationships linked to `Claude Code` are invisible when querying from `claude-code` and vice versa
- `merge-entities` must be used manually for each duplicate pair — 11 merge operations needed
- New entities created by LLM extraction (ingest --mode claude-code) may create more duplicates if the LLM outputs "Claude Code" instead of "claude-code"
- Entity KNN search returns both versions, wasting result slots
- `graph entities --json` shows both versions, inflating entity count

### Root Cause
- `validate_entity_name()` at `src/storage/entities.rs:51-72` was designed to reject only clearly invalid names (empty, newlines, short ALL_CAPS noise)
- It was NOT designed to normalize names to a canonical format
- The `normalize_relation()` function exists for relations (kebab→snake) but NO equivalent exists for entity names
- Entity names come from multiple sources: LLM extraction (mixed case), user input (any format), ingest NER (mixed case)
- The `link --create-missing` path creates entities without normalization
- The `remember --graph-stdin` path creates entities without normalization

### Solution — Normalize entity names on ALL input paths

#### Step 1: Create `normalize_entity_name()` function
```rust
// src/parsers/mod.rs
pub fn normalize_entity_name(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfkd()
     .filter(|c| c.is_ascii() || *c == '-')
     .collect::<String>()
     .to_lowercase()
     .replace(' ', "-")
     .replace('_', "-")
     .replace("--", "-")
     .trim_matches('-')
     .to_string()
}
```

#### Step 2: Call normalize BEFORE validate in all input paths
- `upsert_entity()` at `src/storage/entities.rs:82`: add `let name = normalize_entity_name(&e.name);`
- This catches ALL paths: remember, ingest, link --create-missing, rename-entity

#### Step 3: Add `--normalize-existing` migration flag
- `sqlite-graphrag normalize-entities --dry-run --json` to preview normalizations
- `sqlite-graphrag normalize-entities --yes --json` to apply
- For duplicate pairs: auto-merge via `merge-entities` when normalized names collide
- For the 11 confirmed pairs: merge relationships from non-normalized into normalized entity

#### Step 4: Cleanup current production DB (immediate workaround)
```bash
# Merge the 11 confirmed duplicate pairs
sqlite-graphrag merge-entities --names "Claude Code" --into claude-code --json
sqlite-graphrag merge-entities --names "Danilo Aguiar" --into danilo-aguiar --json
sqlite-graphrag merge-entities --names "BERT NER" --into bert-ner --json
sqlite-graphrag merge-entities --names "Gemini CLI" --into gemini-cli --json
sqlite-graphrag merge-entities --names "WAL checkpoint TRUNCATE" --into wal-checkpoint-truncate --json
sqlite-graphrag merge-entities --names "JSON Error Envelope" --into json-error-envelope --json
sqlite-graphrag merge-entities --names "GraphRAG memory quality" --into graphrag-memory-quality --json
sqlite-graphrag merge-entities --names "deep-research command" --into deep-research-command --json
sqlite-graphrag merge-entities --names "documentation audit" --into documentation-audit --json
sqlite-graphrag merge-entities --names "export subcommand" --into export-subcommand --json
sqlite-graphrag merge-entities --names "CANONICAL_RELATIONS" --into canonical-relations --json
```

### Benefits
- Eliminates duplicate entities permanently
- All input paths produce consistent lowercase-kebab-case entity names
- Graph traversal finds ALL relationships regardless of how the entity was originally created
- Entity KNN search returns unique entities without duplicates
- Prevents future duplicates from LLM extraction

### How to Resolve
- Step 1: Immediate — run 11 merge-entities commands for confirmed duplicates (~15 min)
- Step 2: v1.0.65 — add `normalize_entity_name()` to `src/parsers/mod.rs` (~1h)
- Step 3: v1.0.65 — call normalize in `upsert_entity()` before validate (~30min)
- Step 4: v1.0.65 — add `normalize-entities` migration command (~4h)
- Step 5: Add unit tests for normalization edge cases (~1h)
- Effort: ~6.5h total (code) + 15 min (immediate cleanup)
- Files: `src/parsers/mod.rs`, `src/storage/entities.rs`, `src/commands/normalize_entities.rs` (new)
- Dependency: `unicode-normalization` crate (or manual ASCII transliteration)


## GAP-17 MEDIUM: Super-hub entities (degree 30-128) distort graph traversal — no redistribution command

- Severity: MEDIUM
- Phase: Post-acceptance graph quality audit

### Problem — Verified on Production DB (2026-05-28)
- 15 entities have degree ≥ 30, creating super-hubs that distort graph traversal:

| Entity | Type | Degree |
|---|---|---|
| sqlite-graphrag | project | 128 |
| ingest-claude-code | tool | 64 |
| rust-api-rules | concept | 52 |
| rust-crossplatform-rules | concept | 50 |
| sqlite-graphrag-ingest | tool | 50 |
| rust-testing-rules | concept | 45 |
| rust-memory-rules | concept | 43 |
| i18n-rules | concept | 42 |
| defensive-security | concept | 39 |
| (6 more with degree 35-37) | — | — |

- The top hub `sqlite-graphrag` has 128 edges: 51 `uses`, 30 `applies_to`, 29 `tracked_in`, 12 `fixes`, 3 `causes`
- Graph traverse from `sqlite-graphrag` at depth 2 fans out to potentially 128 × avg_degree = thousands of results
- BFS-based traversal (`traverse_from_memories_with_hops`) visits ALL neighbors equally — no pruning by relevance

### Consequences
- `related` and `graph traverse` from super-hubs return noisy, unfocused results
- `deep-research` graph expansion (GAP-07/08) is dominated by super-hub neighbors
- Evidence chains (GAP-09) pass through super-hubs and produce disconnected paths
- Average entity degree is 2.52 — super-hubs at 128× average dominate traversal

### Root Cause
- No entity degree cap during graph construction
- `remember --graph-stdin` and `ingest --mode claude-code` create edges without checking if the target/source is becoming a super-hub
- Project-level entities naturally accumulate edges: every bug, feature, and decision links to `sqlite-graphrag`
- Rules-catalog entities accumulate sub-topic edges: every rule sub-concept links to the parent rules entity

### Solution

#### Option A (behavioral rule, immediate): Cap edges per entity in future operations
- Add `--max-entity-degree N` flag to `remember`, `ingest`, `link`
- When an entity exceeds N edges, emit `tracing::warn!` and reject new edge
- Default: 50 (matches 95th percentile of current distribution)

#### Option B (LLM-assisted redistribution): Break super-hubs into sub-entities
```bash
# Example: redistribute sqlite-graphrag (128 edges) into domain sub-entities
claude -p "Entity 'sqlite-graphrag' has 128 edges: 51 uses, 30 applies_to, 29 tracked_in, 12 fixes.
Propose 3-5 sub-entities to redistribute edges. Example: sqlite-graphrag-search (recall, hybrid-search, deep-research), sqlite-graphrag-ingest (ingest modes, extraction), sqlite-graphrag-graph (link, unlink, traverse, entities)" \
  --output-format json \
  --json-schema '{"type":"object","properties":{"sub_entities":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"},"entity_type":{"type":"string"},"edges_to_move":{"type":"array","items":{"type":"string"}}},"required":["name","entity_type","edges_to_move"],"additionalProperties":false}}},"required":["sub_entities"],"additionalProperties":false}' \
  --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'
```

#### Option C (traversal improvement): Weight-based pruning in BFS
- Modify `traverse_from_memories_with_hops` to limit neighbors per entity to top-K by weight
- Add `--max-neighbors-per-hop N` flag to `graph traverse` and `related`

### Benefits
- Focused traversal: results from super-hubs are relevant, not exhaustive
- Evidence chains avoid super-hub shortcuts that produce meaningless paths
- Graph quality scales with entity count without degradation

### How to Resolve
- Step 1: Option A — add `--max-entity-degree` warning (~2h)
- Step 2: Option C — top-K pruning in BFS (~3h, in `src/graph.rs`)
- Step 3: Option B — LLM redistribution for the 3 worst super-hubs via `enrich` command (GAP-14)
- Effort: ~5h code + LLM redistribution
- Files: `src/graph.rs`, `src/commands/link.rs`, `src/commands/remember.rs`


## GAP-18 MEDIUM: 55 thin memories (body < 500 chars) — no native LLM enrichment command

- Severity: MEDIUM
- Phase: Post-acceptance graph quality audit

### Problem — Verified on Production DB (2026-05-28)
- 55/965 memories (5.7%) have body under 500 characters
- 233/965 (24.1%) have body under 1000 characters
- Distribution by type:

| Type | Count (<500 chars) | Avg body length |
|---|---|---|
| decision | 32 | 393 |
| incident | 8 | 327 |
| project | 8 | 460 |
| feedback | 3 | 384 |
| reference | 3 | 458 |
| user | 1 | 455 |

- These memories are stubs: they contain the WHAT but lack the WHY, the context, and the connections
- The user demonstrated a working workaround: bash script that loops over thin memories, sends each to `claude -p` with a domain-specific prompt, and updates via `edit --body-stdin`
- This workaround works but is brittle: requires manual prompt engineering, no resume/retry, no cost tracking, no structured output validation

### Consequences
- Thin memories produce poor embedding vectors — less text = less semantic signal
- `recall` returns thin memories with high similarity but low information value
- Entity extraction from thin memories yields few or zero entities (contributes to GAP-14's 95.4% orphan rate)
- LLM agents using `read` on thin memories get insufficient context for reasoning

### Root Cause
- Memories created by quick `remember` calls during sessions tend to be terse
- No minimum body length is enforced (any non-empty body is accepted)
- No enrichment pipeline exists to expand terse memories post-creation
- The user must write custom bash scripts for each enrichment use case

### Solution — Native `enrich` subcommand (integrated with GAP-14)

The `enrich` command proposed in GAP-14 should include a `body-enrich` operation:

```
sqlite-graphrag enrich --operation body-enrich \
  --mode claude-code \
  --filter "LENGTH(body) < 500" \
  --limit 55 \
  --prompt-template /tmp/enrich-prompt.md \
  --min-output-chars 500 \
  --max-output-chars 2000 \
  --json
```

#### Architecture for body-enrich operation
```
Phase 1 — SCAN: SELECT name, body, description, type FROM memories
                WHERE LENGTH(body) < 500 AND deleted_at IS NULL
                ORDER BY LENGTH(body) ASC
                LIMIT --limit

Phase 2 — JUDGE: For each memory:
  sqlite-graphrag read --name "$name" --json | \
    claude -p "<prompt-template with {name}, {description}, {body} placeholders>" \
      --output-format json \
      --json-schema '{"type":"object","properties":{"enriched_body":{"type":"string"}},...}' \
      --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}'

Phase 3 — VALIDATE:
  - enriched_body.len() >= --min-output-chars (default 500)
  - enriched_body.len() <= --max-output-chars (default 2000)
  - enriched_body contains ALL key terms from original body (preservation check)
  - If validation fails: skip with warning, do not overwrite original

Phase 4 — PERSIST:
  sqlite-graphrag edit --name "$name" --body-stdin --json
```

#### Key flags for `enrich --operation body-enrich`
- `--prompt-template <PATH>`: Markdown file with `{name}`, `{description}`, `{body}`, `{type}` placeholders
- `--min-output-chars <N>`: Reject enrichment shorter than N (default: 500)
- `--max-output-chars <N>`: Reject enrichment longer than N (default: 2000)
- `--preserve-check`: Verify key terms from original body appear in enrichment (default: true)
- `--filter <SQL_WHERE>`: Custom SQL filter for memory selection (default: `LENGTH(body) < 500`)
- `--resume` / `--retry-failed`: Queue DB pattern from `ingest --mode claude-code`
- `--dry-run`: Preview which memories would be enriched without calling LLM

#### Immediate workaround (bash, works today)
```bash
# List thin memories
sqlite3 graphrag.sqlite "SELECT name FROM memories WHERE namespace='global' AND deleted_at IS NULL AND LENGTH(body) < 500 ORDER BY LENGTH(body)" > /tmp/thin_memories.txt

# Enrich each via claude -p
while IFS= read -r name; do
  data=$(sqlite-graphrag read --name "$name" --json 2>/dev/null)
  body=$(echo "$data" | jaq -r '.body')
  desc=$(echo "$data" | jaq -r '.description')
  
  enriched=$(claude -p "Memory '$name' ($desc). Current body ($((${#body})) chars): $body. Expand to 500-1500 chars preserving ALL existing information. Add context about WHY this matters. Return ONLY the enriched text." \
    --output-format text \
    --max-turns 3 --no-session-persistence --dangerously-skip-permissions --settings '{"hooks":{}}' 2>/dev/null) || continue
  
  [ ${#enriched} -ge 400 ] && [ ${#enriched} -gt ${#body} ] && \
    echo "$enriched" | sqlite-graphrag edit --name "$name" --body-stdin --json 2>/dev/null && \
    echo "OK: $name (${#body}→${#enriched})"
done < /tmp/thin_memories.txt
```

### Benefits
- Thin memories become information-rich — better embeddings, better recall
- Entity extraction on enriched bodies yields more entities (reduces orphan rate)
- The `enrich` command provides a standardized, resumable, cost-tracked pipeline
- Domain-specific prompt templates allow specialized enrichment (e.g., legal, technical, personal)
- Validation prevents overwriting with lower-quality content

### How to Resolve
- Step 1: Immediate — run bash workaround for 55 thin memories (~30 min with claude -p)
- Step 2: v1.0.65 — implement `body-enrich` as operation in `enrich` command (GAP-14, ~3h additional)
- Step 3: Add `--prompt-template` and `--min-output-chars` flags
- Step 4: Add preservation check (key terms from original must appear in enrichment)
- Effort: included in GAP-14 (~3h additional for body-enrich operation)
- Files: `src/commands/enrich.rs` (within GAP-14 scope)
- Dependency: GAP-14 (enrich command skeleton)


## GAP-16 MEDIUM: Documentation says kebab-case "canonical" for relations but DB/JSON use snake_case

- Severity: MEDIUM
- Phase: Post-acceptance documentation audit

### Problem
- CLAUDE.md and `--help` list kebab-case as "canonical": `applies-to, depends-on, tracked-in`
- DB stores ALL relationships in snake_case: `applies_to, depends_on, tracked_in`
- JSON output shows snake_case: `{"relation": "applies_to"}`
- CLI normalizes kebab→snake on ALL paths via `normalize_relation()` at `src/parsers/mod.rs:174`
- **No data loss**: `--relation applies-to` and `--relation applies_to` return identical results
- **No kebab-case relations exist in the DB** (verified: `SELECT relation FROM relationships WHERE relation LIKE '%-%'` returns 0 rows)
- This is purely a documentation/expectation inconsistency, NOT a data loss bug

### Consequences
- LLMs parsing JSON output get `applies_to` but CLAUDE.md tells them to use `applies-to` — both work but the inconsistency confuses
- Scripts doing string comparison between doc values and JSON output fail (`"applies-to" != "applies_to"`)
- No functional impact — cosmetic/documentation issue only

### Root Cause
- `normalize_relation()` does `s.to_lowercase().replace('-', "_")` — storage is always snake_case
- `--help` was written with kebab-case as user-facing "canonical" format
- JSON output serializes stored snake_case directly without converting back

### Solution (Option A recommended: document the normalization)
- Add note to CLAUDE.md: "Relations accepted in kebab-case or snake_case. Storage and JSON output always use snake_case."
- Effort: 30 minutes
- Files: CLAUDE.md, docs/HOW_TO_USE.md, docs/HOW_TO_USE.pt-BR.md


## Summary Table

| ID | Severity | Gap | Status | Fixed in |
|---|---|---|---|---|
| GAP-01 | HIGH | Missing deep-research.schema.json | FIXED | v1.0.65 |
| GAP-02 | HIGH | No contract tests for deep-research | FIXED | v1.0.65 |
| GAP-03 | MEDIUM | schemas/README.md missing entry | FIXED | v1.0.65 |
| GAP-04 | MEDIUM | TESTING.md missing section | FIXED | v1.0.65 |
| GAP-05 | LOW | mentions_ratio at 0.0% (ideal) | CLOSED | N/A |
| GAP-06 | LOW | Entity name convention in test plans | CLOSED | N/A |
| GAP-07 | CRITICAL | Sub-queries cosmetic: single embedding for all | FIXED | v1.0.65 |
| GAP-08 | CRITICAL | FTS5 hardcoded score 0.5, no RRF fusion | FIXED | v1.0.65 |
| GAP-09 | HIGH | Evidence chains are flat dump, not directed paths | FIXED | v1.0.65 |
| GAP-10 | HIGH | Centroid collapse on multi-dimensional queries | FIXED | v1.0.65 (via GAP-07) |
| GAP-11 | MEDIUM | No RRF fusion between KNN and graph pools | FIXED | v1.0.65 |
| GAP-12 | LOW | Partial decomposition (FTS per-query, KNN not) | FIXED | v1.0.65 (via GAP-07, GAP-08) |
| GAP-13 | HIGH | No reclassify-relation command, applies_to at 25.4% | FIXED | v1.0.65 |
| GAP-14 | CRITICAL | No LLM-augmented enrich pipeline, 95.4% orphan memories | PARTIAL | v1.0.65 (3 of 12 ops) |
| GAP-15 | HIGH | Entity names not normalized — 301 non-conforming, 11 duplicate pairs | FIXED | v1.0.65 |
| GAP-16 | MEDIUM | Docs say kebab-case canonical for relations but DB/JSON use snake_case | FIXED | v1.0.65 |
| GAP-17 | MEDIUM | Super-hub entities (degree 30-128) distort traversal | FIXED | v1.0.65 |
| GAP-18 | MEDIUM | 55 thin memories (body < 500 chars) need LLM enrichment | FIXED | v1.0.65 (via enrich body-enrich) |

### GAP-14 Partial Status
- 3 operations implemented in v1.0.65: `memory-bindings`, `entity-descriptions`, `body-enrich`
- 9 operations deferred to future releases: `weight-calibrate`, `relation-reclassify`, `entity-connect`, `entity-type-validate`, `description-enrich`, `cross-domain-bridges`, `domain-classify`, `graph-audit`, `deep-research-synth`, `body-extract`
- Scan-only stubs exist for deferred operations; LLM judge + persist steps remain unimplemented
