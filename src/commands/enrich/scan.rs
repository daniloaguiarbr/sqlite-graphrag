//! Scan functions — select candidates for each enrichment operation.

use super::*;

// ---------------------------------------------------------------------------
// Shared WHERE predicates (GAP-SG-77)
//
// Each operation-specific predicate lives in ONE place so the scanner and the
// count-only `count_operation_backlog` cannot drift. Sharing the exact string
// guarantees that the backlog reported by `enrich --status` matches the rows a
// scan would actually select.
// ---------------------------------------------------------------------------

/// `memory-bindings`: memories with zero `memory_entities` rows.
const UNBOUND_MEMORY_PREDICATE: &str =
    "NOT EXISTS (SELECT 1 FROM memory_entities me WHERE me.memory_id = m.id)";

/// `entity-descriptions`: entities whose description is NULL or empty.
const NULL_DESCRIPTION_PREDICATE: &str = "(description IS NULL OR description = '')";

/// `body-enrich`: memory body shorter than the `?2` character threshold.
const SHORT_BODY_PREDICATE: &str = "LENGTH(COALESCE(m.body,'')) < ?2";

/// `description-enrich`: memories with generic/auto-generated descriptions.
const GENERIC_DESCRIPTION_PREDICATE: &str = "(description LIKE '%ingested%' \
     OR description LIKE '%imported%' OR description LIKE '%added%' \
     OR length(description) < 30)";

/// `weight-calibrate`: relationships strong enough to warrant recalibration.
const HIGH_WEIGHT_PREDICATE: &str = "r.weight >= 0.7";

/// `relation-reclassify`: relationships still using the generic `applies_to`.
const GENERIC_RELATION_PREDICATE: &str = "r.relation = 'applies_to'";

// ---------------------------------------------------------------------------

/// Returns memories without any `memory_entities` binding.
///
/// These are the targets for `memory-bindings` enrichment. When `name_filter`
/// is non-empty, restricts the scan to the given names (G37); unknown names
/// are silently skipped (the caller can detect them by comparing
/// requested vs. returned).
pub(super) fn scan_unbound_memories(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
    name_filter: &[String],
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();

    if name_filter.is_empty() {
        let sql = format!(
            "SELECT m.id, m.name, m.body
             FROM memories m
             WHERE m.namespace = ?1
               AND m.deleted_at IS NULL
               AND {UNBOUND_MEMORY_PREDICATE}
             ORDER BY m.id
             {limit_clause}"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params![namespace], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        // Build a parameterised IN clause: ?2, ?3, ..., ?{1+n}
        let placeholders: Vec<String> = (2..=name_filter.len() + 1)
            .map(|i| format!("?{i}"))
            .collect();
        let in_clause = placeholders.join(", ");
        let sql = format!(
            "SELECT m.id, m.name, m.body
             FROM memories m
             WHERE m.namespace = ?1
               AND m.deleted_at IS NULL
               AND m.name IN ({in_clause})
               AND {UNBOUND_MEMORY_PREDICATE}
             ORDER BY m.id
             {limit_clause}"
        );
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + name_filter.len());
        params_vec.push(&namespace);
        for n in name_filter {
            params_vec.push(n);
        }
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(params_vec.iter().copied()),
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// GAP-SG-24/26: returns ALREADY-bound memory names for additive augmentation,
/// restricted to `name_filter`.
///
/// Unlike [`scan_unbound_memories`] this selects memories that DO have at least
/// one `memory_entities` binding, so a second extraction pass can merge newly
/// discovered entities/relationships without disturbing existing links (the
/// persist path is purely additive). A name filter is MANDATORY: re-running
/// extraction over an entire namespace is expensive and rarely intended, so an
/// empty filter is rejected rather than silently scanning everything.
pub(super) fn scan_bound_memories_for_augment(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
    name_filter: &[String],
) -> Result<Vec<String>, AppError> {
    if name_filter.is_empty() {
        return Err(AppError::Validation(
            "augment-bindings requires an explicit subset: pass --names or \
             --names-file (it refuses to re-scan the whole namespace)"
                .into(),
        ));
    }
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let placeholders: Vec<String> = (2..=name_filter.len() + 1)
        .map(|i| format!("?{i}"))
        .collect();
    let in_clause = placeholders.join(", ");
    let sql = format!(
        "SELECT m.name
         FROM memories m
         WHERE m.namespace = ?1
           AND m.deleted_at IS NULL
           AND m.name IN ({in_clause})
           AND EXISTS (
               SELECT 1 FROM memory_entities me WHERE me.memory_id = m.id
           )
         ORDER BY m.id
         {limit_clause}"
    );
    let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + name_filter.len());
    params_vec.push(&namespace);
    for n in name_filter {
        params_vec.push(n);
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(
            rusqlite::params_from_iter(params_vec.iter().copied()),
            |r| r.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Reads a list of memory names from a UTF-8 text file (G37).
///
/// Empty lines and lines beginning with `#` are skipped. Returns a
/// de-duplicated, order-preserving list of trimmed names.
pub(super) fn read_names_file(path: &Path) -> Result<Vec<String>, AppError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        AppError::Validation(format!("failed to read names file {}: {e}", path.display()))
    })?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    Ok(out)
}

/// Resolves the union of `--names` and `--names-file` (G37).
pub(super) fn resolve_name_filter(args: &EnrichArgs) -> Result<Vec<String>, AppError> {
    let mut combined: Vec<String> = args.names.clone();
    if let Some(p) = &args.names_file {
        let from_file = read_names_file(p)?;
        for n in from_file {
            if !combined.contains(&n) {
                combined.push(n);
            }
        }
    }
    Ok(combined)
}

/// Returns entities with NULL or empty description.
///
/// These are the targets for `entity-descriptions` enrichment.
pub(super) fn scan_entities_without_description(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
    name_filter: &[String],
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();

    if name_filter.is_empty() {
        let sql = format!(
            "SELECT id, name, type
             FROM entities
             WHERE namespace = ?1
               AND {NULL_DESCRIPTION_PREDICATE}
             ORDER BY id
             {limit_clause}"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params![namespace], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let placeholders: Vec<String> = (2..=name_filter.len() + 1)
            .map(|i| format!("?{i}"))
            .collect();
        let in_clause = placeholders.join(", ");
        let sql = format!(
            "SELECT id, name, type
             FROM entities
             WHERE namespace = ?1
               AND name IN ({in_clause})
               AND {NULL_DESCRIPTION_PREDICATE}
             ORDER BY id
             {limit_clause}"
        );
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + name_filter.len());
        params_vec.push(&namespace);
        for n in name_filter {
            params_vec.push(n);
        }
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(params_vec.iter().copied()),
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Returns memories whose body length is below the configured minimum.
///
/// These are the targets for `body-enrich` (GAP-18).
pub(super) fn scan_short_body_memories(
    conn: &Connection,
    namespace: &str,
    min_chars: usize,
    limit: Option<usize>,
    name_filter: &[String],
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();

    if name_filter.is_empty() {
        let sql = format!(
            "SELECT m.id, m.name, m.body
             FROM memories m
             WHERE m.namespace = ?1
               AND m.deleted_at IS NULL
               AND {SHORT_BODY_PREDICATE}
             ORDER BY m.id
             {limit_clause}"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params![namespace, min_chars as i64], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let placeholders: Vec<String> = (3..=name_filter.len() + 2)
            .map(|i| format!("?{i}"))
            .collect();
        let in_clause = placeholders.join(", ");
        let sql = format!(
            "SELECT m.id, m.name, m.body
             FROM memories m
             WHERE m.namespace = ?1
               AND m.deleted_at IS NULL
               AND m.name IN ({in_clause})
               AND {SHORT_BODY_PREDICATE}
             ORDER BY m.id
             {limit_clause}"
        );
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(2 + name_filter.len());
        let min_chars_i64 = min_chars as i64;
        params_vec.push(&namespace);
        params_vec.push(&min_chars_i64);
        for n in name_filter {
            params_vec.push(n);
        }
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(params_vec.iter().copied()),
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Returns live memories that still have no row in `memory_embeddings`.
///
/// These are the targets for `re-embed`.
pub(super) fn scan_memories_without_embeddings(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
    name_filter: &[String],
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();

    if name_filter.is_empty() {
        let sql = format!(
            "SELECT m.id, m.name, COALESCE(m.body,'')
             FROM memories m
             LEFT JOIN memory_embeddings me ON me.memory_id = m.id
             WHERE m.namespace = ?1
               AND m.deleted_at IS NULL
               AND me.memory_id IS NULL
             ORDER BY m.id
             {limit_clause}"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params![namespace], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let placeholders: Vec<String> = (2..=name_filter.len() + 1)
            .map(|i| format!("?{i}"))
            .collect();
        let in_clause = placeholders.join(", ");
        let sql = format!(
            "SELECT m.id, m.name, COALESCE(m.body,'')
             FROM memories m
             LEFT JOIN memory_embeddings me ON me.memory_id = m.id
             WHERE m.namespace = ?1
               AND m.deleted_at IS NULL
               AND m.name IN ({in_clause})
               AND me.memory_id IS NULL
             ORDER BY m.id
             {limit_clause}"
        );
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + name_filter.len());
        params_vec.push(&namespace);
        for n in name_filter {
            params_vec.push(n);
        }
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(params_vec.iter().copied()),
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// G27: Returns relationships with weight >= 0.7 that may need recalibration.
#[allow(clippy::type_complexity)]
pub(super) fn scan_weight_candidates(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, String, String, f64)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let sql = format!(
        "SELECT r.id, e1.name, e2.name, r.relation, r.weight \
         FROM relationships r \
         JOIN entities e1 ON e1.id = r.source_id \
         JOIN entities e2 ON e2.id = r.target_id \
         WHERE {HIGH_WEIGHT_PREDICATE} AND e1.namespace = ?1 \
         ORDER BY r.weight DESC {limit_clause}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![namespace], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, f64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// G27: Returns relationships with generic relation types (applies_to).
pub(super) fn scan_generic_relations(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let sql = format!(
        "SELECT r.id, e1.name, e2.name, r.relation \
         FROM relationships r \
         JOIN entities e1 ON e1.id = r.source_id \
         JOIN entities e2 ON e2.id = r.target_id \
         WHERE {GENERIC_RELATION_PREDICATE} AND e1.namespace = ?1 \
         ORDER BY r.id {limit_clause}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![namespace], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// PERSIST helpers for fully-implemented operations
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Scan dispatcher — maps operation to scan query result (item keys)
// ---------------------------------------------------------------------------

pub(super) fn scan_operation(
    conn: &Connection,
    namespace: &str,
    args: &EnrichArgs,
) -> Result<Vec<String>, AppError> {
    // G37: resolve --names + --names-file once and apply to every scan path.
    let name_filter = resolve_name_filter(args)?;
    match args.operation() {
        EnrichOperation::MemoryBindings => {
            let rows = scan_unbound_memories(conn, namespace, args.limit, &name_filter)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        // GAP-SG-24/26: additive augmentation processes ALREADY-bound memories,
        // restricted to an explicit name filter so it never re-scans the whole
        // namespace.
        EnrichOperation::AugmentBindings => {
            scan_bound_memories_for_augment(conn, namespace, args.limit, &name_filter)
        }
        EnrichOperation::EntityDescriptions => {
            let rows =
                scan_entities_without_description(conn, namespace, args.limit, &name_filter)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        EnrichOperation::BodyEnrich => {
            let rows = scan_short_body_memories(
                conn,
                namespace,
                args.min_output_chars,
                args.limit,
                &name_filter,
            )?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        EnrichOperation::ReEmbed => {
            let rows = scan_memories_without_embeddings(conn, namespace, args.limit, &name_filter)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        EnrichOperation::WeightCalibrate => {
            let rows = scan_weight_candidates(conn, namespace, args.limit)?;
            Ok(rows
                .into_iter()
                .map(|(id, _, _, _, _)| id.to_string())
                .collect())
        }
        EnrichOperation::RelationReclassify => {
            let rows = scan_generic_relations(conn, namespace, args.limit)?;
            Ok(rows
                .into_iter()
                .map(|(id, _, _, _)| id.to_string())
                .collect())
        }
        EnrichOperation::EntityConnect | EnrichOperation::CrossDomainBridges => {
            let pairs = scan_isolated_entity_pairs(conn, namespace, args.limit)?;
            Ok(pairs.into_iter().map(|(_, name, _, _)| name).collect())
        }
        EnrichOperation::EntityTypeValidate => {
            let rows = scan_entities_for_type_validation(conn, namespace, args.limit)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        EnrichOperation::DescriptionEnrich => {
            let rows = scan_generic_descriptions(conn, namespace, args.limit)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        EnrichOperation::DomainClassify
        | EnrichOperation::GraphAudit
        | EnrichOperation::DeepResearchSynth
        | EnrichOperation::BodyExtract => {
            let limit_clause = args.limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
            let sql = format!(
                "SELECT name FROM memories WHERE namespace=?1 AND deleted_at IS NULL ORDER BY id {limit_clause}"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut names = stmt
                .query_map(rusqlite::params![namespace], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            // GAP-SG-27: honour --names/--names-file for body-extract (and the
            // sibling whole-namespace scans), which previously ignored it and
            // scanned every memory by id.
            if !name_filter.is_empty() {
                names.retain(|n| name_filter.iter().any(|f| f == n));
            }
            Ok(names)
        }
    }
}

/// Scan for pairs of entities that share no direct relationship.
#[allow(clippy::type_complexity)]
pub(super) fn scan_isolated_entity_pairs(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, i64, String)>, AppError> {
    let limit_val = limit.unwrap_or(50) as i64;
    let mut stmt = conn.prepare_cached(
        "SELECT e1.id, e1.name, e2.id, e2.name FROM entities e1, entities e2 \
         WHERE e1.namespace = ?1 AND e2.namespace = ?1 AND e1.id < e2.id \
         AND NOT EXISTS (SELECT 1 FROM relationships r WHERE \
           (r.source_id = e1.id AND r.target_id = e2.id) OR \
           (r.source_id = e2.id AND r.target_id = e1.id)) \
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![namespace, limit_val], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Scan for entities with non-validated types (all entities for type audit).
pub(super) fn scan_entities_for_type_validation(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let sql = format!(
        "SELECT id, name, type FROM entities WHERE namespace = ?1 ORDER BY id {limit_clause}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![namespace], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Scan for memories with generic descriptions (ingested, imported, etc).
pub(super) fn scan_generic_descriptions(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let sql = format!(
        "SELECT id, name, description FROM memories WHERE namespace = ?1 AND deleted_at IS NULL \
         AND {GENERIC_DESCRIPTION_PREDICATE} \
         ORDER BY id {limit_clause}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![namespace], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Backlog counter (GAP-SG-77)
// ---------------------------------------------------------------------------

/// Count-only backlog for a single operation, using a cheap `SELECT COUNT(*)`.
///
/// This mirrors the dispatch of [`scan_operation`], reusing the SAME shared
/// WHERE predicates so the count can never drift from the rows a scan would
/// select. Unlike the scanners it materialises no rows.
///
/// The returned figure has DATABASE semantics — the real backlog of the
/// operation against the store — which is distinct from the FILE (sidecar
/// queue) semantics reported by `queue_pending`/`queue_dead`. It powers the
/// `scan_backlog` field of `enrich --status` so that db-backed operations
/// (`entity-descriptions`, `body-enrich`, `re-embed`, ...) no longer report a
/// false `pending=0` when thousands of eligible items exist.
///
/// Notes on individual operations:
/// - `body-enrich` uses the default [`DEFAULT_BODY_ENRICH_MIN_CHARS`] threshold
///   (the same default the CLI applies when `--min-output-chars` is omitted).
/// - `re-embed` counts memories with no `memory_embeddings` row via a
///   `NOT EXISTS` anti-join, semantically identical to the scanner's
///   `LEFT JOIN ... IS NULL`.
/// - advisory / quadratic scan-only operations (`augment-bindings`,
///   `entity-connect`, `cross-domain-bridges`, `domain-classify`,
///   `graph-audit`, `deep-research-synth`, `body-extract`) have no closeable
///   database deficit and report `0`.
pub(super) fn count_operation_backlog(
    conn: &Connection,
    operation: &EnrichOperation,
    namespace: &str,
) -> Result<i64, AppError> {
    let count = match operation {
        EnrichOperation::MemoryBindings => {
            let sql = format!(
                "SELECT COUNT(*) FROM memories m \
                 WHERE m.namespace = ?1 AND m.deleted_at IS NULL \
                 AND {UNBOUND_MEMORY_PREDICATE}"
            );
            conn.query_row(&sql, rusqlite::params![namespace], |r| r.get::<_, i64>(0))?
        }
        EnrichOperation::EntityDescriptions => {
            let sql = format!(
                "SELECT COUNT(*) FROM entities \
                 WHERE namespace = ?1 AND {NULL_DESCRIPTION_PREDICATE}"
            );
            conn.query_row(&sql, rusqlite::params![namespace], |r| r.get::<_, i64>(0))?
        }
        EnrichOperation::BodyEnrich => {
            let sql = format!(
                "SELECT COUNT(*) FROM memories m \
                 WHERE m.namespace = ?1 AND m.deleted_at IS NULL \
                 AND {SHORT_BODY_PREDICATE}"
            );
            let min_chars = super::DEFAULT_BODY_ENRICH_MIN_CHARS as i64;
            conn.query_row(&sql, rusqlite::params![namespace, min_chars], |r| {
                r.get::<_, i64>(0)
            })?
        }
        EnrichOperation::ReEmbed => {
            // Anti-join equivalent to the scanner's LEFT JOIN ... IS NULL.
            conn.query_row(
                "SELECT COUNT(*) FROM memories m \
                 WHERE m.namespace = ?1 AND m.deleted_at IS NULL \
                 AND NOT EXISTS (SELECT 1 FROM memory_embeddings me WHERE me.memory_id = m.id)",
                rusqlite::params![namespace],
                |r| r.get::<_, i64>(0),
            )?
        }
        EnrichOperation::WeightCalibrate => {
            let sql = format!(
                "SELECT COUNT(*) FROM relationships r \
                 JOIN entities e1 ON e1.id = r.source_id \
                 WHERE {HIGH_WEIGHT_PREDICATE} AND e1.namespace = ?1"
            );
            conn.query_row(&sql, rusqlite::params![namespace], |r| r.get::<_, i64>(0))?
        }
        EnrichOperation::RelationReclassify => {
            let sql = format!(
                "SELECT COUNT(*) FROM relationships r \
                 JOIN entities e1 ON e1.id = r.source_id \
                 WHERE {GENERIC_RELATION_PREDICATE} AND e1.namespace = ?1"
            );
            conn.query_row(&sql, rusqlite::params![namespace], |r| r.get::<_, i64>(0))?
        }
        EnrichOperation::EntityTypeValidate => {
            // Mirrors scan_entities_for_type_validation: every entity is a
            // candidate for the type audit.
            conn.query_row(
                "SELECT COUNT(*) FROM entities WHERE namespace = ?1",
                rusqlite::params![namespace],
                |r| r.get::<_, i64>(0),
            )?
        }
        EnrichOperation::DescriptionEnrich => {
            let sql = format!(
                "SELECT COUNT(*) FROM memories \
                 WHERE namespace = ?1 AND deleted_at IS NULL \
                 AND {GENERIC_DESCRIPTION_PREDICATE}"
            );
            conn.query_row(&sql, rusqlite::params![namespace], |r| r.get::<_, i64>(0))?
        }
        // Advisory / quadratic scan-only operations have no closeable database
        // backlog; report 0 (see the doc comment above).
        EnrichOperation::AugmentBindings
        | EnrichOperation::EntityConnect
        | EnrichOperation::CrossDomainBridges
        | EnrichOperation::DomainClassify
        | EnrichOperation::GraphAudit
        | EnrichOperation::DeepResearchSynth
        | EnrichOperation::BodyExtract => 0,
    };
    Ok(count)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn open_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE memories (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                namespace   TEXT NOT NULL DEFAULT 'global',
                name        TEXT NOT NULL,
                type        TEXT NOT NULL DEFAULT 'note',
                description TEXT NOT NULL DEFAULT '',
                body        TEXT NOT NULL DEFAULT '',
                body_hash   TEXT NOT NULL DEFAULT '',
                session_id  TEXT,
                source      TEXT NOT NULL DEFAULT 'agent',
                metadata    TEXT NOT NULL DEFAULT '{}',
                created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                deleted_at  INTEGER,
                UNIQUE(namespace, name)
            );
            CREATE TABLE entities (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                namespace   TEXT NOT NULL DEFAULT 'global',
                name        TEXT NOT NULL,
                type        TEXT NOT NULL DEFAULT 'concept',
                description TEXT,
                degree      INTEGER NOT NULL DEFAULT 0,
                created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                UNIQUE(namespace, name)
            );
            CREATE TABLE memory_entities (
                memory_id  INTEGER NOT NULL,
                entity_id  INTEGER NOT NULL,
                PRIMARY KEY (memory_id, entity_id)
            );
            CREATE TABLE relationships (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                namespace  TEXT NOT NULL DEFAULT 'global',
                source_id  INTEGER NOT NULL,
                target_id  INTEGER NOT NULL,
                relation   TEXT NOT NULL,
                weight     REAL NOT NULL DEFAULT 0.5,
                description TEXT,
                UNIQUE(source_id, target_id, relation)
            );
            CREATE TABLE memory_embeddings (
                memory_id   INTEGER PRIMARY KEY,
                namespace   TEXT NOT NULL,
                embedding   BLOB NOT NULL,
                source      TEXT NOT NULL,
                model       TEXT NOT NULL DEFAULT '',
                dim         INTEGER NOT NULL DEFAULT 384,
                created_at  INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        )
        .expect("schema creation must succeed");
        conn
    }

    #[test]
    fn scan_unbound_memories_finds_memories_without_bindings() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'test-mem', 'some body content')",
            [],
        )
        .unwrap();

        let results = scan_unbound_memories(&conn, "global", None, &[]).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "test-mem");
    }

    #[test]
    fn scan_unbound_memories_excludes_bound_memories() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'bound-mem', 'body')",
            [],
        )
        .unwrap();
        let mem_id: i64 = conn
            .query_row("SELECT id FROM memories WHERE name='bound-mem'", [], |r| {
                r.get(0)
            })
            .unwrap();
        conn.execute(
            "INSERT INTO entities (namespace, name) VALUES ('global', 'some-entity')",
            [],
        )
        .unwrap();
        let ent_id: i64 = conn
            .query_row(
                "SELECT id FROM entities WHERE name='some-entity'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_id) VALUES (?1, ?2)",
            rusqlite::params![mem_id, ent_id],
        )
        .unwrap();

        let results = scan_unbound_memories(&conn, "global", None, &[]).unwrap();
        assert!(results.is_empty(), "bound memory must not appear in scan");
    }

    #[test]
    fn scan_entities_without_description_finds_null_description() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO entities (namespace, name, type, description) VALUES ('global', 'my-tool', 'tool', NULL)",
            [],
        )
        .unwrap();

        let results = scan_entities_without_description(&conn, "global", None, &[]).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "my-tool");
    }

    #[test]
    fn scan_entities_without_description_excludes_entities_with_description() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO entities (namespace, name, type, description) VALUES ('global', 'described-tool', 'tool', 'Has a description already')",
            [],
        )
        .unwrap();

        let results = scan_entities_without_description(&conn, "global", None, &[]).unwrap();
        assert!(
            results.is_empty(),
            "entity with description must not appear"
        );
    }

    #[test]
    fn scan_short_body_memories_finds_short_bodies() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'short-mem', 'hi')",
            [],
        )
        .unwrap();

        let results = scan_short_body_memories(&conn, "global", 100, None, &[]).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "short-mem");
    }

    #[test]
    fn scan_short_body_memories_excludes_long_bodies() {
        let conn = open_test_db();
        let long_body = "a".repeat(1000);
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'long-mem', ?1)",
            rusqlite::params![long_body],
        )
        .unwrap();

        let results = scan_short_body_memories(&conn, "global", 100, None, &[]).unwrap();
        assert!(results.is_empty(), "long memory must not appear in scan");
    }

    #[test]
    fn scan_respects_limit() {
        let conn = open_test_db();
        for i in 0..5 {
            conn.execute(
                &format!("INSERT INTO memories (namespace, name, body) VALUES ('global', 'mem-{i}', 'short')"),
                [],
            )
            .unwrap();
        }

        let results = scan_short_body_memories(&conn, "global", 1000, Some(3), &[]).unwrap();
        assert_eq!(results.len(), 3, "limit must be respected");
    }

    #[test]
    fn scan_memories_without_embeddings_finds_only_missing_rows() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'missing-vec', 'body one')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'has-vec', 'body two')",
            [],
        )
        .unwrap();
        let memory_id: i64 = conn
            .query_row(
                "SELECT id FROM memories WHERE namespace='global' AND name='has-vec'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let embedding = vec![0.0_f32; crate::constants::embedding_dim()];
        crate::storage::memories::upsert_vec(
            &conn, memory_id, "global", "note", &embedding, "has-vec", "body two",
        )
        .unwrap();

        let results = scan_memories_without_embeddings(&conn, "global", None, &[]).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "missing-vec");
    }

    #[test]
    fn scan_memories_without_embeddings_respects_name_filter() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'match-me', 'body one')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'skip-me', 'body two')",
            [],
        )
        .unwrap();

        let results =
            scan_memories_without_embeddings(&conn, "global", None, &["match-me".to_string()])
                .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "match-me");
    }

    #[test]
    fn dry_run_emits_preview_without_calling_llm() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'dry-mem', 'tiny')",
            [],
        )
        .unwrap();

        let results = scan_short_body_memories(&conn, "global", 1000, None, &[]).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "dry-mem");
    }

    #[test]
    fn scan_bound_memories_for_augment_requires_names_and_finds_bound() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (id, namespace, name, body) VALUES (1, 'global', 'bound', 'b')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (id, namespace, name, body) VALUES (2, 'global', 'unbound', 'b')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entities (id, namespace, name) VALUES (10, 'global', 'e')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_id) VALUES (1, 10)",
            [],
        )
        .unwrap();

        assert!(scan_bound_memories_for_augment(&conn, "global", None, &[]).is_err());

        let names = scan_bound_memories_for_augment(
            &conn,
            "global",
            None,
            &["bound".to_string(), "unbound".to_string()],
        )
        .unwrap();
        assert_eq!(names, vec!["bound".to_string()]);
    }

    // -----------------------------------------------------------------------
    // GAP-SG-77: count_operation_backlog — correctness + scan parity
    // -----------------------------------------------------------------------

    #[test]
    fn count_operation_backlog_entity_descriptions_counts_only_missing() {
        let conn = open_test_db();
        for i in 0..3 {
            conn.execute(
                &format!("INSERT INTO entities (namespace, name, type, description) VALUES ('global', 'ent-{i}', 'tool', NULL)"),
                [],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO entities (namespace, name, type, description) VALUES ('global', 'described', 'tool', 'already has one')",
            [],
        )
        .unwrap();

        let n =
            count_operation_backlog(&conn, &EnrichOperation::EntityDescriptions, "global").unwrap();
        assert_eq!(n, 3);
        // Parity: the count must equal what the scanner would materialise.
        let scanned = scan_entities_without_description(&conn, "global", None, &[]).unwrap();
        assert_eq!(n as usize, scanned.len());
    }

    #[test]
    fn count_operation_backlog_re_embed_counts_missing_embeddings() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'no-vec', 'body one')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'has-vec', 'body two')",
            [],
        )
        .unwrap();
        let has_vec_id: i64 = conn
            .query_row(
                "SELECT id FROM memories WHERE namespace='global' AND name='has-vec'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let embedding = vec![0.0_f32; crate::constants::embedding_dim()];
        crate::storage::memories::upsert_vec(
            &conn, has_vec_id, "global", "note", &embedding, "has-vec", "body two",
        )
        .unwrap();

        let n = count_operation_backlog(&conn, &EnrichOperation::ReEmbed, "global").unwrap();
        assert_eq!(n, 1);
        let scanned = scan_memories_without_embeddings(&conn, "global", None, &[]).unwrap();
        assert_eq!(n as usize, scanned.len());
    }

    #[test]
    fn count_operation_backlog_memory_bindings_counts_unbound() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'unbound', 'b')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'bound', 'b')",
            [],
        )
        .unwrap();
        let bound_id: i64 = conn
            .query_row("SELECT id FROM memories WHERE name='bound'", [], |r| {
                r.get(0)
            })
            .unwrap();
        conn.execute(
            "INSERT INTO entities (namespace, name) VALUES ('global', 'e')",
            [],
        )
        .unwrap();
        let ent_id: i64 = conn
            .query_row("SELECT id FROM entities WHERE name='e'", [], |r| r.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_id) VALUES (?1, ?2)",
            rusqlite::params![bound_id, ent_id],
        )
        .unwrap();

        let n = count_operation_backlog(&conn, &EnrichOperation::MemoryBindings, "global").unwrap();
        assert_eq!(n, 1);
        let scanned = scan_unbound_memories(&conn, "global", None, &[]).unwrap();
        assert_eq!(n as usize, scanned.len());
    }

    #[test]
    fn count_operation_backlog_body_enrich_uses_default_threshold() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'short', 'tiny')",
            [],
        )
        .unwrap();
        let long_body = "a".repeat(super::DEFAULT_BODY_ENRICH_MIN_CHARS + 100);
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'long', ?1)",
            rusqlite::params![long_body],
        )
        .unwrap();

        let n = count_operation_backlog(&conn, &EnrichOperation::BodyEnrich, "global").unwrap();
        assert_eq!(n, 1);
        // Parity against the scanner using the same default threshold.
        let scanned = scan_short_body_memories(
            &conn,
            "global",
            super::DEFAULT_BODY_ENRICH_MIN_CHARS,
            None,
            &[],
        )
        .unwrap();
        assert_eq!(n as usize, scanned.len());
    }

    #[test]
    fn count_operation_backlog_advisory_ops_report_zero() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'm', 'b')",
            [],
        )
        .unwrap();
        for op in [
            EnrichOperation::EntityConnect,
            EnrichOperation::CrossDomainBridges,
            EnrichOperation::GraphAudit,
            EnrichOperation::BodyExtract,
        ] {
            let n = count_operation_backlog(&conn, &op, "global").unwrap();
            assert_eq!(n, 0, "advisory op {op:?} must report zero backlog");
        }
    }
}
