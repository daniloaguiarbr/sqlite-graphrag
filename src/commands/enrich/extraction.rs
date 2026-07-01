//! LLM invocation — per-operation call helpers and result types.

use super::*;

// ---------------------------------------------------------------------------
// LLM invocation — Claude Code
// ---------------------------------------------------------------------------

/// Calls `claude -p` via the shared `claude_runner` module (G02).
///
/// Returns `(output_value, cost_usd, is_oauth)`.
pub(super) fn call_claude(
    binary: &Path,
    prompt: &str,
    json_schema: &str,
    input_text: &str,
    model: Option<&str>,
    timeout_secs: u64,
) -> Result<(serde_json::Value, f64, bool), AppError> {
    let result = crate::commands::claude_runner::run_claude(
        binary,
        prompt,
        json_schema,
        input_text,
        model,
        timeout_secs,
        7,
    )?;
    Ok((result.value, result.cost_usd, result.is_oauth))
}

/// GAP-SG-72/73 (v1.1.00): per-item failure diagnostics captured from a
/// [`crate::chat_api::ChatError`] returned by [`call_openrouter`]. The
/// `retry_class` is computed AT THE ORIGIN by `chat_api.rs` (the exact HTTP
/// status / provider code), never inferred downstream by matching the
/// formatted error string. `finish_reason` and the token counts are the raw
/// truncation diagnostics OpenRouter attached to the failing response, when
/// one was decoded.
pub(super) struct OpenRouterFailureDiagnostics {
    pub(super) retry_class: crate::retry::AttemptOutcome,
    pub(super) finish_reason: Option<String>,
    pub(super) prompt_tokens: Option<i64>,
    pub(super) completion_tokens: Option<i64>,
}

// GAP-SG-72/73: `call_openrouter` returns the same `(Value, f64, bool)` tuple
// shape as the subprocess providers (`call_claude`/`call_codex`/
// `call_opencode`) so every `call_*` helper above can keep matching on
// `mode` uniformly. That tuple has no room for `ChatError`'s typed
// `retry_class` / truncation diagnostics, so they are stashed here on
// failure and drained by the caller in `mod.rs` right after every
// `call_result` (mirrors the `ENRICH_LAST_BACKEND` accumulator in
// `postprocess.rs`). `thread_local` — NOT a process-wide `Mutex` — because
// the parallel worker loop runs one item per OS thread at a time: a
// process-wide slot would let a diagnostic from one worker's item leak into
// another worker's unrelated failure.
thread_local! {
    static LAST_OPENROUTER_FAILURE: std::cell::RefCell<Option<OpenRouterFailureDiagnostics>> =
        const { std::cell::RefCell::new(None) };
}

/// Drains the diagnostics stashed by the most recent [`call_openrouter`]
/// failure on THIS thread. Callers must invoke this unconditionally right
/// after every `call_result` (success or failure) so a diagnostic never
/// survives past the item that produced it — see the doc comment on
/// [`OpenRouterFailureDiagnostics`].
pub(super) fn take_last_openrouter_failure() -> Option<OpenRouterFailureDiagnostics> {
    LAST_OPENROUTER_FAILURE.with(|cell| cell.borrow_mut().take())
}

/// v1.0.95 (ADR-0054): route a single JUDGE turn through the OpenRouter
/// chat-completions REST API. Unlike the subprocess runners there is no
/// `binary` argument: the process-wide chat client (initialised in `run()`
/// before scan) is fetched from the singleton and driven synchronously via
/// the shared tokio runtime. Returns `(value, cost_usd, is_oauth=false)`
/// where `cost_usd` is read from the response `usage.cost`.
///
/// v1.1.00 (GAP-SG-70/72/73): `complete` now returns a typed
/// `Result<ChatCompletion, ChatError>` carrying `finish_reason` / token
/// diagnostics and an origin-computed `retry_class`. On success those
/// diagnostics are simply discarded (the item succeeded); on failure they
/// are stashed via [`take_last_openrouter_failure`] so the queue recorder in
/// `mod.rs` can call `record_item_failure_typed` with the precise verdict
/// instead of falling back to the untyped `classify_enrich_outcome` message
/// sniffing.
pub(super) fn call_openrouter(
    prompt: &str,
    json_schema: &str,
    input_text: &str,
    model: Option<&str>,
    timeout_secs: u64,
) -> Result<(serde_json::Value, f64, bool), AppError> {
    // `model` is bound into the client singleton at init; `timeout_secs` is
    // enforced by the reqwest builder. Both remain in the signature for
    // parity with the subprocess runners.
    let _ = (model, timeout_secs);
    let client = crate::embedder::openrouter_chat_client().ok_or_else(|| {
        AppError::Validation(
            "OpenRouter chat client not initialised before dispatch (internal error)".into(),
        )
    })?;
    let runtime = crate::embedder::shared_runtime()?;
    match runtime.block_on(client.complete(
        prompt,
        input_text,
        json_schema,
        Some(crate::constants::ENRICH_INITIAL_MAX_TOKENS),
    )) {
        Ok(completion) => Ok((completion.value, completion.cost_usd, false)),
        Err(chat_err) => {
            LAST_OPENROUTER_FAILURE.with(|cell| {
                *cell.borrow_mut() = Some(OpenRouterFailureDiagnostics {
                    retry_class: chat_err.retry_class,
                    finish_reason: chat_err.finish_reason.clone(),
                    prompt_tokens: chat_err.prompt_tokens.map(i64::from),
                    completion_tokens: chat_err.completion_tokens.map(i64::from),
                });
            });
            Err(chat_err.source)
        }
    }
}

// ---------------------------------------------------------------------------
// Internal result type for a single item call
// ---------------------------------------------------------------------------

pub(super) enum EnrichItemResult {
    Done {
        memory_id: Option<i64>,
        entity_id: Option<i64>,
        entities: usize,
        rels: usize,
        chars_before: Option<usize>,
        chars_after: Option<usize>,
        cost: f64,
        is_oauth: bool,
    },
    Skipped {
        reason: String,
    },
    /// G29 Step 4 (v1.0.69): the LLM rewrite diverged from the original
    /// body beyond the configured `--preserve-threshold` and was rejected
    /// before persistence. The trigram-Jaccard score and threshold are
    /// emitted in the NDJSON stream for operator audit.
    PreservationFailed {
        score: f64,
        threshold: f64,
        chars_before: usize,
        chars_after: usize,
    },
}

// ---------------------------------------------------------------------------
// Per-operation call helpers (SCAN + JUDGE + PERSIST in one unit)
// ---------------------------------------------------------------------------

pub(super) fn call_memory_bindings(
    conn: &Connection,
    namespace: &str,
    memory_name: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    // Look up the memory
    let (memory_id, body): (i64, String) = conn.query_row(
        "SELECT id, COALESCE(body,'') FROM memories WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
        rusqlite::params![namespace, memory_name],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::NotFound(format!("memory '{memory_name}' not found")),
        other => AppError::Database(other),
    })?;

    if body.trim().is_empty() {
        return Ok(EnrichItemResult::Skipped {
            reason: "body is empty".to_string(),
        });
    }

    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            BINDINGS_PROMPT,
            BINDINGS_SCHEMA,
            &body,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            BINDINGS_PROMPT,
            BINDINGS_SCHEMA,
            &body,
            model,
            timeout,
        )?,
        EnrichMode::Opencode => call_opencode(
            binary,
            BINDINGS_PROMPT,
            BINDINGS_SCHEMA,
            &body,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => {
            call_openrouter(BINDINGS_PROMPT, BINDINGS_SCHEMA, &body, model, timeout)?
        }
    };

    let empty_arr = serde_json::Value::Array(vec![]);
    let entities_val = value.get("entities").unwrap_or(&empty_arr);
    let rels_val = value.get("relationships").unwrap_or(&empty_arr);

    let (ent_count, rel_count) =
        persist_memory_bindings(conn, namespace, memory_id, entities_val, rels_val)?;

    Ok(EnrichItemResult::Done {
        memory_id: Some(memory_id),
        entity_id: None,
        entities: ent_count,
        rels: rel_count,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

pub(super) fn call_entity_description(
    conn: &Connection,
    namespace: &str,
    entity_name: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (entity_id, entity_type): (i64, String) = conn
        .query_row(
            "SELECT id, type FROM entities WHERE namespace=?1 AND name=?2",
            rusqlite::params![namespace, entity_name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::EntityNotYetMaterialized {
                name: entity_name.to_string(),
                namespace: namespace.to_string(),
            },
            other => AppError::Database(other),
        })?;

    let prompt = format!(
        "{ENTITY_DESCRIPTION_PROMPT_PREFIX}{entity_name}\nEntity type: {entity_type}\n\nGenerate a description:"
    );

    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            &prompt,
            ENTITY_DESCRIPTION_SCHEMA,
            "",
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            &prompt,
            ENTITY_DESCRIPTION_SCHEMA,
            "",
            model,
            timeout,
        )?,
        EnrichMode::Opencode => call_opencode(
            binary,
            &prompt,
            ENTITY_DESCRIPTION_SCHEMA,
            "",
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => {
            call_openrouter(&prompt, ENTITY_DESCRIPTION_SCHEMA, "", model, timeout)?
        }
    };

    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("LLM result missing 'description' field".into()))?;

    persist_entity_description(conn, entity_id, description)?;

    Ok(EnrichItemResult::Done {
        memory_id: None,
        entity_id: Some(entity_id),
        entities: 0,
        rels: 0,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn call_body_enrich(
    conn: &Connection,
    namespace: &str,
    memory_name: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
    min_output_chars: usize,
    max_output_chars: usize,
    prompt_template: Option<&Path>,
    preserve_threshold: f64,
    paths: &crate::paths::AppPaths,
    llm_backend: crate::cli::LlmBackendChoice,
    embedding_backend: crate::cli::EmbeddingBackendChoice,
) -> Result<EnrichItemResult, AppError> {
    let (memory_id, body, description, memory_type): (i64, String, String, String) = conn
        .query_row(
            "SELECT id, COALESCE(body,''), COALESCE(description,''), COALESCE(type,'note') \
         FROM memories WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
            rusqlite::params![namespace, memory_name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("memory '{memory_name}' not found"))
            }
            other => AppError::Database(other),
        })?;

    let chars_before = body.chars().count();

    // G26: gather graph context for contextualized enrichment
    let linked_entities: Vec<String> = {
        let mut stmt = conn.prepare_cached(
            "SELECT e.name FROM memory_entities me \
             JOIN entities e ON e.id = me.entity_id \
             WHERE me.memory_id = ?1 LIMIT 10",
        )?;
        let result: Vec<String> = stmt
            .query_map(rusqlite::params![memory_id], |r| r.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        result
    };

    // Load custom prompt template if provided
    let prompt_prefix = if let Some(tmpl_path) = prompt_template {
        let file_size = std::fs::metadata(tmpl_path)
            .map_err(|e| {
                AppError::Io(std::io::Error::new(
                    e.kind(),
                    format!("failed to stat prompt template: {e}"),
                ))
            })?
            .len();
        if file_size > MAX_MEMORY_BODY_LEN as u64 {
            return Err(AppError::LimitExceeded(
                crate::i18n::validation::body_exceeds(MAX_MEMORY_BODY_LEN),
            ));
        }
        std::fs::read_to_string(tmpl_path).map_err(|e| {
            AppError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to read prompt template: {e}"),
            ))
        })?
    } else {
        BODY_ENRICH_PROMPT_PREFIX.to_string()
    };

    // G26: build contextualized prompt with graph data
    let context_section = if !linked_entities.is_empty() || !description.is_empty() {
        let mut ctx = String::new();
        ctx.push_str(&format!(
            "\nContext:\n- Memory name: {memory_name}\n- Type: {memory_type}\n"
        ));
        if !description.is_empty() {
            ctx.push_str(&format!("- Description: {description}\n"));
        }
        ctx.push_str(&format!("- Domain: {namespace}\n"));
        if !linked_entities.is_empty() {
            ctx.push_str(&format!(
                "- Linked entities: {}\n",
                linked_entities.join(", ")
            ));
        }
        ctx
    } else {
        String::new()
    };

    let prompt = format!(
        "{prompt_prefix}{context_section}\nTarget minimum length: {min_output_chars} characters. Maximum: {max_output_chars} characters."
    );

    // The body schema uses a free-form enriched_body field
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => {
            call_claude(binary, &prompt, BODY_ENRICH_SCHEMA, &body, model, timeout)?
        }
        EnrichMode::Codex => {
            call_codex(binary, &prompt, BODY_ENRICH_SCHEMA, &body, model, timeout)?
        }
        EnrichMode::Opencode => {
            call_opencode(binary, &prompt, BODY_ENRICH_SCHEMA, &body, model, timeout)?
        }
        EnrichMode::OpenRouter => {
            call_openrouter(&prompt, BODY_ENRICH_SCHEMA, &body, model, timeout)?
        }
    };

    let enriched_body = value
        .get("enriched_body")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("LLM result missing 'enriched_body' field".into()))?;

    let chars_after = enriched_body.chars().count();

    // G29 Passo 4 (v1.0.69): preservation check. Before persisting, run
    // a trigram-Jaccard similarity between the original body and the
    // LLM-rewritten body. When the score falls below
    // `args.preserve_threshold` (default 0.7 per the G29 gap), reject the
    // rewrite as a likely hallucination. The result is recorded in the
    // NDJSON stream so operators can audit what the LLM tried to do.
    let threshold = preserve_threshold;
    let verdict =
        crate::preservation::PreservationVerdict::evaluate(&body, enriched_body, threshold);
    if !verdict.is_accepted() {
        return Ok(EnrichItemResult::PreservationFailed {
            score: match verdict {
                crate::preservation::PreservationVerdict::Preserved { score, .. } => score,
                crate::preservation::PreservationVerdict::Rejected { score, .. } => score,
                crate::preservation::PreservationVerdict::Unchanged { .. } => 1.0,
            },
            threshold,
            chars_before,
            chars_after,
        });
    }

    // G29 Passo 5 (v1.0.69): idempotency via blake3 hash. Before persisting,
    // compare the hash of the original body against the hash of the enriched
    // body. Identical hashes mean the LLM produced a byte-for-byte identical
    // body (rare but possible) — treat as `Skipped` so re-running the batch
    // is safe and the queue does not get re-persisted entries.
    let old_hash = blake3::hash(body.as_bytes()).to_hex().to_string();
    let new_hash = blake3::hash(enriched_body.as_bytes()).to_hex().to_string();
    if old_hash == new_hash {
        return Ok(EnrichItemResult::Skipped {
            reason: format!(
                "enriched body hash matches original (blake3:{old_hash}); idempotency skip"
            ),
        });
    }

    // Only persist if the enriched body is genuinely longer
    if chars_after <= chars_before {
        return Ok(EnrichItemResult::Skipped {
            reason: format!(
                "enriched body ({chars_after} chars) not longer than original ({chars_before} chars)"
            ),
        });
    }

    persist_enriched_body(
        conn,
        namespace,
        memory_id,
        memory_name,
        enriched_body,
        paths,
        llm_backend,
        embedding_backend,
    )?;

    Ok(EnrichItemResult::Done {
        memory_id: Some(memory_id),
        entity_id: None,
        entities: 0,
        rels: 0,
        chars_before: Some(chars_before),
        chars_after: Some(chars_after),
        cost,
        is_oauth,
    })
}

// GAP-SG-73: failures from `reembed_memory_vector` below reach the queue
// as bare `AppError::Embedding`, not a typed `EmbedError` — see the doc
// comment on the `AppError::Embedding` arm of `classify_enrich_outcome` in
// `queue.rs` for why the origin-typed `retry_class` is not threaded through
// here, and why Transient is the documented, deliberate safe floor.
pub(super) fn call_reembed(
    conn: &Connection,
    namespace: &str,
    memory_name: &str,
    paths: &crate::paths::AppPaths,
    llm_backend: crate::cli::LlmBackendChoice,
    embedding_backend: crate::cli::EmbeddingBackendChoice,
) -> Result<EnrichItemResult, AppError> {
    let (memory_id, body, memory_type): (i64, String, String) = conn
        .query_row(
            "SELECT id, COALESCE(body,''), COALESCE(type,'note')
             FROM memories
             WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
            rusqlite::params![namespace, memory_name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("memory '{memory_name}' not found"))
            }
            other => AppError::Database(other),
        })?;

    if body.trim().is_empty() {
        return Ok(EnrichItemResult::Skipped {
            reason: "body is empty".to_string(),
        });
    }

    reembed_memory_vector(
        conn,
        namespace,
        memory_id,
        memory_name,
        &memory_type,
        &body,
        paths,
        llm_backend,
        embedding_backend,
    )?;

    Ok(EnrichItemResult::Done {
        memory_id: Some(memory_id),
        entity_id: None,
        entities: 0,
        rels: 0,
        chars_before: Some(body.chars().count()),
        chars_after: Some(body.chars().count()),
        cost: 0.0,
        is_oauth: true,
    })
}

// scan_operation moved to scan.rs

// ---------------------------------------------------------------------------
// Codex stub provider
// ---------------------------------------------------------------------------

/// Locates the Codex CLI binary.
pub(super) fn find_codex_binary(explicit: Option<&Path>) -> Result<PathBuf, AppError> {
    if let Some(p) = explicit {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        return Err(AppError::Validation(format!(
            "Codex binary not found at explicit path: {}",
            p.display()
        )));
    }

    if let Ok(env_path) = std::env::var("SQLITE_GRAPHRAG_CODEX_BINARY") {
        let p = PathBuf::from(&env_path);
        if p.exists() {
            return Ok(p);
        }
    }

    let name = if cfg!(windows) { "codex.exe" } else { "codex" };
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Ok(crate::extract::llm_embedding::resolve_real_binary(
                    &candidate,
                ));
            }
        }
    }

    Err(AppError::Validation(
        "Codex CLI binary not found in PATH. Install it or specify --codex-binary".to_string(),
    ))
}

/// G27: Calibrate weight of a single relationship via LLM.
pub(super) fn call_weight_calibrate(
    conn: &Connection,
    _namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let rel_id: i64 = item_key
        .parse()
        .map_err(|_| AppError::Validation(format!("invalid relationship id: {item_key}")))?;
    let (source_name, target_name, relation, current_weight): (String, String, String, f64) = conn
        .query_row(
            "SELECT e1.name, e2.name, r.relation, r.weight \
             FROM relationships r \
             JOIN entities e1 ON e1.id = r.source_id \
             JOIN entities e2 ON e2.id = r.target_id \
             WHERE r.id = ?1",
            rusqlite::params![rel_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|_| AppError::NotFound(format!("relationship {rel_id} not found")))?;

    let input_text = format!(
        "Source: {source_name}\nTarget: {target_name}\nRelation: {relation}\nCurrent weight: {current_weight}"
    );
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            WEIGHT_CALIBRATE_PROMPT,
            WEIGHT_CALIBRATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            WEIGHT_CALIBRATE_PROMPT,
            WEIGHT_CALIBRATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Opencode => call_opencode(
            binary,
            WEIGHT_CALIBRATE_PROMPT,
            WEIGHT_CALIBRATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
            WEIGHT_CALIBRATE_PROMPT,
            WEIGHT_CALIBRATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };

    let calibrated = value
        .get("calibrated_weight")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| AppError::Validation("LLM result missing 'calibrated_weight'".into()))?;

    conn.execute(
        "UPDATE relationships SET weight = ?1 WHERE id = ?2",
        rusqlite::params![calibrated, rel_id],
    )?;

    Ok(EnrichItemResult::Done {
        memory_id: None,
        entity_id: None,
        entities: 0,
        rels: 1,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27: Reclassify a generic relationship type via LLM.
pub(super) fn call_relation_reclassify(
    conn: &Connection,
    _namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let rel_id: i64 = item_key
        .parse()
        .map_err(|_| AppError::Validation(format!("invalid relationship id: {item_key}")))?;
    let (source_name, target_name, current_relation): (String, String, String) = conn
        .query_row(
            "SELECT e1.name, e2.name, r.relation \
             FROM relationships r \
             JOIN entities e1 ON e1.id = r.source_id \
             JOIN entities e2 ON e2.id = r.target_id \
             WHERE r.id = ?1",
            rusqlite::params![rel_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("relationship {rel_id} not found")))?;

    let input_text = format!(
        "Source entity: {source_name}\nTarget entity: {target_name}\nCurrent relation: {current_relation}"
    );
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            RELATION_RECLASSIFY_PROMPT,
            RELATION_RECLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            RELATION_RECLASSIFY_PROMPT,
            RELATION_RECLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Opencode => call_opencode(
            binary,
            RELATION_RECLASSIFY_PROMPT,
            RELATION_RECLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
            RELATION_RECLASSIFY_PROMPT,
            RELATION_RECLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };

    let new_relation = value
        .get("relation")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("LLM result missing 'relation'".into()))?;
    let new_strength = value
        .get("strength")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);

    conn.execute(
        "UPDATE relationships SET relation = ?1, weight = ?2 WHERE id = ?3",
        rusqlite::params![new_relation, new_strength, rel_id],
    )?;

    Ok(EnrichItemResult::Done {
        memory_id: None,
        entity_id: None,
        entities: 0,
        rels: 1,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27 P2: Connect isolated entities via LLM-suggested relationship.
pub(super) fn call_entity_connect(
    conn: &Connection,
    namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let pairs = scan_isolated_entity_pairs(conn, namespace, Some(1))?;
    let (e1_id, e1_name, e2_id, e2_name) =
        match pairs.into_iter().find(|(_, n, _, _)| n == item_key) {
            Some(p) => p,
            None => {
                return Ok(EnrichItemResult::Skipped {
                    reason: "pair no longer isolated".into(),
                })
            }
        };
    let input_text = format!("Entity A: {e1_name}\nEntity B: {e2_name}");
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            ENTITY_CONNECT_PROMPT,
            ENTITY_CONNECT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            ENTITY_CONNECT_PROMPT,
            ENTITY_CONNECT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Opencode => call_opencode(
            binary,
            ENTITY_CONNECT_PROMPT,
            ENTITY_CONNECT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
            ENTITY_CONNECT_PROMPT,
            ENTITY_CONNECT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let relation = value
        .get("relation")
        .and_then(|v| v.as_str())
        .unwrap_or("none");
    if relation == "none" {
        return Ok(EnrichItemResult::Skipped {
            reason: "LLM determined no relationship".into(),
        });
    }
    let strength = value
        .get("strength")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);
    conn.execute(
        "INSERT OR IGNORE INTO relationships (namespace, source_id, target_id, relation, weight) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![namespace, e1_id, e2_id, relation, strength],
    )?;
    Ok(EnrichItemResult::Done {
        memory_id: None,
        entity_id: None,
        entities: 0,
        rels: 1,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27 P2: Validate entity type assignment via LLM.
pub(super) fn call_entity_type_validate(
    conn: &Connection,
    namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (ent_id, ent_name, ent_type): (i64, String, String) = conn
        .query_row(
            "SELECT id, name, type FROM entities WHERE namespace = ?1 AND name = ?2",
            rusqlite::params![namespace, item_key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::EntityNotYetMaterialized {
                name: item_key.to_string(),
                namespace: namespace.to_string(),
            },
            other => AppError::Database(other),
        })?;
    let input_text = format!("Entity: {ent_name}\nCurrent type: {ent_type}");
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            ENTITY_TYPE_VALIDATE_PROMPT,
            ENTITY_TYPE_VALIDATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            ENTITY_TYPE_VALIDATE_PROMPT,
            ENTITY_TYPE_VALIDATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Opencode => call_opencode(
            binary,
            ENTITY_TYPE_VALIDATE_PROMPT,
            ENTITY_TYPE_VALIDATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
            ENTITY_TYPE_VALIDATE_PROMPT,
            ENTITY_TYPE_VALIDATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let validated_type = value
        .get("validated_type")
        .and_then(|v| v.as_str())
        .unwrap_or(&ent_type);
    let was_correct = value
        .get("was_correct")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if !was_correct {
        conn.execute(
            "UPDATE entities SET type = ?1 WHERE id = ?2",
            rusqlite::params![validated_type, ent_id],
        )?;
    }
    Ok(EnrichItemResult::Done {
        memory_id: None,
        entity_id: Some(ent_id),
        entities: 1,
        rels: 0,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27 P2: Enrich generic memory description via LLM.
pub(super) fn call_description_enrich(
    conn: &Connection,
    _namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (mem_id, body, old_desc): (i64, String, String) = conn
        .query_row(
            "SELECT id, body, description FROM memories WHERE name = ?1 AND deleted_at IS NULL",
            rusqlite::params![item_key],
            |r| Ok((r.get(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("memory '{item_key}' not found")))?;
    let snippet: String = body.chars().take(500).collect();
    let input_text = format!(
        "Memory name: {item_key}\nCurrent description: {old_desc}\nBody preview: {snippet}"
    );
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            DESCRIPTION_ENRICH_PROMPT,
            DESCRIPTION_ENRICH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            DESCRIPTION_ENRICH_PROMPT,
            DESCRIPTION_ENRICH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Opencode => call_opencode(
            binary,
            DESCRIPTION_ENRICH_PROMPT,
            DESCRIPTION_ENRICH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
            DESCRIPTION_ENRICH_PROMPT,
            DESCRIPTION_ENRICH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let new_desc = value
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or(&old_desc);
    let old_name: String = conn.query_row(
        "SELECT name FROM memories WHERE id = ?1",
        rusqlite::params![mem_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE memories SET description = ?1 WHERE id = ?2",
        rusqlite::params![new_desc, mem_id],
    )?;
    memories::sync_fts_after_update(
        conn, mem_id, &old_name, &old_desc, &body, &old_name, new_desc, &body,
    )?;
    Ok(EnrichItemResult::Done {
        memory_id: Some(mem_id),
        entity_id: None,
        entities: 0,
        rels: 0,
        chars_before: Some(old_desc.len()),
        chars_after: Some(new_desc.len()),
        cost,
        is_oauth,
    })
}

/// G27 P2: Classify memory into domain category via LLM.
pub(super) fn call_domain_classify(
    conn: &Connection,
    _namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (mem_id, body, desc): (i64, String, String) = conn
        .query_row(
            "SELECT id, body, description FROM memories WHERE name = ?1 AND deleted_at IS NULL",
            rusqlite::params![item_key],
            |r| Ok((r.get(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("memory '{item_key}' not found")))?;
    let snippet: String = body.chars().take(500).collect();
    let input_text = format!("Memory: {item_key}\nDescription: {desc}\nBody preview: {snippet}");
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            DOMAIN_CLASSIFY_PROMPT,
            DOMAIN_CLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            DOMAIN_CLASSIFY_PROMPT,
            DOMAIN_CLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Opencode => call_opencode(
            binary,
            DOMAIN_CLASSIFY_PROMPT,
            DOMAIN_CLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
            DOMAIN_CLASSIFY_PROMPT,
            DOMAIN_CLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let domain = value
        .get("domain")
        .and_then(|v| v.as_str())
        .unwrap_or("uncategorized");
    let metadata = format!(r#"{{"domain":"{}"}}"#, domain.replace('"', "\\\""));
    conn.execute(
        "UPDATE memories SET metadata = ?1 WHERE id = ?2",
        rusqlite::params![metadata, mem_id],
    )?;
    Ok(EnrichItemResult::Done {
        memory_id: Some(mem_id),
        entity_id: None,
        entities: 0,
        rels: 0,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27 P2: Audit memory graph quality via LLM.
pub(super) fn call_graph_audit(
    conn: &Connection,
    _namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (mem_id, body, desc): (i64, String, String) = conn
        .query_row(
            "SELECT id, body, description FROM memories WHERE name = ?1 AND deleted_at IS NULL",
            rusqlite::params![item_key],
            |r| Ok((r.get(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("memory '{item_key}' not found")))?;
    let snippet: String = body.chars().take(500).collect();
    let ent_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_entities WHERE memory_id = ?1",
            rusqlite::params![mem_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let input_text = format!("Memory: {item_key}\nDescription: {desc}\nEntity bindings: {ent_count}\nBody preview: {snippet}");
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            GRAPH_AUDIT_PROMPT,
            GRAPH_AUDIT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            GRAPH_AUDIT_PROMPT,
            GRAPH_AUDIT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Opencode => call_opencode(
            binary,
            GRAPH_AUDIT_PROMPT,
            GRAPH_AUDIT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
            GRAPH_AUDIT_PROMPT,
            GRAPH_AUDIT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let issues = value
        .get("issues")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    Ok(EnrichItemResult::Done {
        memory_id: Some(mem_id),
        entity_id: None,
        entities: 0,
        rels: issues,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27 P2: Synthesize research findings into graph entities/relationships via LLM.
pub(super) fn call_deep_research_synth(
    conn: &Connection,
    namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (mem_id, body): (i64, String) = conn
        .query_row(
            "SELECT id, body FROM memories WHERE name = ?1 AND deleted_at IS NULL",
            rusqlite::params![item_key],
            |r| Ok((r.get(0)?, r.get::<_, String>(1)?)),
        )
        .map_err(|_| AppError::NotFound(format!("memory '{item_key}' not found")))?;
    let snippet: String = body.chars().take(2000).collect();
    let input_text = format!("Memory: {item_key}\nBody:\n{snippet}");
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            DEEP_RESEARCH_SYNTH_PROMPT,
            DEEP_RESEARCH_SYNTH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            DEEP_RESEARCH_SYNTH_PROMPT,
            DEEP_RESEARCH_SYNTH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Opencode => call_opencode(
            binary,
            DEEP_RESEARCH_SYNTH_PROMPT,
            DEEP_RESEARCH_SYNTH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
            DEEP_RESEARCH_SYNTH_PROMPT,
            DEEP_RESEARCH_SYNTH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let mut ent_count = 0usize;
    let mut rel_count = 0usize;
    if let Some(ents) = value.get("entities").and_then(|v| v.as_array()) {
        for e in ents {
            let name = e.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let etype_str = e
                .get("entity_type")
                .and_then(|v| v.as_str())
                .unwrap_or("concept");
            let etype: EntityType = etype_str.parse().unwrap_or(EntityType::Concept);
            if name.len() >= 2 {
                let ne = NewEntity {
                    name: name.to_string(),
                    entity_type: etype,
                    description: None,
                };
                let _ = entities::upsert_entity(conn, namespace, &ne);
                ent_count += 1;
            }
        }
    }
    if let Some(rels) = value.get("relationships").and_then(|v| v.as_array()) {
        for r in rels {
            let src = r.get("source").and_then(|v| v.as_str()).unwrap_or_default();
            let tgt = r.get("target").and_then(|v| v.as_str()).unwrap_or_default();
            if src.is_empty() || tgt.is_empty() {
                continue;
            }
            let rel = r
                .get("relation")
                .and_then(|v| v.as_str())
                .unwrap_or("related");
            let str_ = r.get("strength").and_then(|v| v.as_f64()).unwrap_or(0.5);
            if let (Some(sid), Some(tid)) = (
                entities::find_entity_id(conn, namespace, src)?,
                entities::find_entity_id(conn, namespace, tgt)?,
            ) {
                let _ = entities::create_or_fetch_relationship(
                    conn, namespace, sid, tid, rel, str_, None,
                );
                rel_count += 1;
            }
        }
    }
    Ok(EnrichItemResult::Done {
        memory_id: Some(mem_id),
        entity_id: None,
        entities: ent_count,
        rels: rel_count,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27 P2: Extract structured body from unstructured text via LLM.
///
/// GAP-SG-28: when `graph_only` is set, the memory body is left UNTOUCHED and
/// the extraction instead pulls entities/relationships into the graph (additive,
/// via the same upsert path as `memory-bindings`). This is the read-only mode —
/// it never rewrites or truncates the stored body.
#[allow(clippy::too_many_arguments)]
pub(super) fn call_body_extract(
    conn: &Connection,
    namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
    graph_only: bool,
) -> Result<EnrichItemResult, AppError> {
    // GAP-SG-28: read-only graph extraction. Reuse the bindings prompt/schema
    // and the additive persist path; the body is never modified.
    if graph_only {
        let (memory_id, body): (i64, String) = conn
            .query_row(
                "SELECT id, COALESCE(body,'') FROM memories WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
                rusqlite::params![namespace, item_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("memory '{item_key}' not found"))
                }
                other => AppError::Database(other),
            })?;
        if body.trim().is_empty() {
            return Ok(EnrichItemResult::Skipped {
                reason: "body is empty".to_string(),
            });
        }
        let (value, cost, is_oauth) = match mode {
            EnrichMode::ClaudeCode => call_claude(
                binary,
                BINDINGS_PROMPT,
                BINDINGS_SCHEMA,
                &body,
                model,
                timeout,
            )?,
            EnrichMode::Codex => call_codex(
                binary,
                BINDINGS_PROMPT,
                BINDINGS_SCHEMA,
                &body,
                model,
                timeout,
            )?,
            EnrichMode::Opencode => call_opencode(
                binary,
                BINDINGS_PROMPT,
                BINDINGS_SCHEMA,
                &body,
                model,
                timeout,
            )?,
            EnrichMode::OpenRouter => {
                call_openrouter(BINDINGS_PROMPT, BINDINGS_SCHEMA, &body, model, timeout)?
            }
        };
        let empty_arr = serde_json::Value::Array(vec![]);
        let entities_val = value.get("entities").unwrap_or(&empty_arr);
        let rels_val = value.get("relationships").unwrap_or(&empty_arr);
        let (ent_count, rel_count) =
            persist_memory_bindings(conn, namespace, memory_id, entities_val, rels_val)?;
        return Ok(EnrichItemResult::Done {
            memory_id: Some(memory_id),
            entity_id: None,
            entities: ent_count,
            rels: rel_count,
            chars_before: None,
            chars_after: None,
            cost,
            is_oauth,
        });
    }

    let (mem_id, body, old_desc): (i64, String, String) = conn
        .query_row(
            "SELECT id, body, description FROM memories WHERE name = ?1 AND deleted_at IS NULL",
            rusqlite::params![item_key],
            |r| Ok((r.get(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("memory '{item_key}' not found")))?;
    let old_name: String = conn.query_row(
        "SELECT name FROM memories WHERE id = ?1",
        rusqlite::params![mem_id],
        |r| r.get(0),
    )?;
    let input_text = format!("Memory: {item_key}\nBody:\n{body}");
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            BODY_EXTRACT_PROMPT,
            BODY_EXTRACT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            BODY_EXTRACT_PROMPT,
            BODY_EXTRACT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Opencode => call_opencode(
            binary,
            BODY_EXTRACT_PROMPT,
            BODY_EXTRACT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
            BODY_EXTRACT_PROMPT,
            BODY_EXTRACT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let restructured = value
        .get("restructured_body")
        .and_then(|v| v.as_str())
        .unwrap_or(&body);
    let chars_before = body.len();
    let chars_after = restructured.len();
    let new_hash = blake3::hash(restructured.as_bytes()).to_hex().to_string();
    conn.execute(
        "UPDATE memories SET body = ?1, body_hash = ?2, updated_at = unixepoch() WHERE id = ?3",
        rusqlite::params![restructured, new_hash, mem_id],
    )?;
    memories::sync_fts_after_update(
        conn,
        mem_id,
        &old_name,
        &old_desc,
        &body,
        &old_name,
        &old_desc,
        restructured,
    )?;
    Ok(EnrichItemResult::Done {
        memory_id: Some(mem_id),
        entity_id: None,
        entities: 0,
        rels: 0,
        chars_before: Some(chars_before),
        chars_after: Some(chars_after),
        cost,
        is_oauth,
    })
}

// scan_isolated_entity_pairs, scan_entities_for_type_validation, scan_generic_descriptions moved to scan.rs

/// Calls the Codex CLI for a single enrichment item.
///
/// Follows the same contract as `call_claude`: returns `(value, cost_usd, is_oauth=false)`.
pub(super) fn call_codex(
    binary: &Path,
    prompt: &str,
    json_schema: &str,
    input_text: &str,
    model: Option<&str>,
    timeout_secs: u64,
) -> Result<(serde_json::Value, f64, bool), AppError> {
    use wait_timeout::ChildExt;

    // G31+G32+G33 (v1.0.69): validate the model BEFORE spawn, write the
    // schema to a trusted cache path (not /tmp), and reuse the
    // consolidated JSONL parser. See `codex_spawn.rs` for the canonical
    // hardening rationale.
    crate::commands::codex_spawn::validate_codex_model(model)?;
    let schema_file = crate::commands::codex_spawn::trusted_schema_path()?;

    let args = crate::commands::codex_spawn::CodexSpawnArgs {
        binary,
        prompt,
        json_schema,
        input_text,
        model,
        timeout_secs,
        schema_path: schema_file.clone(),
    };
    let mut cmd = crate::commands::codex_spawn::build_codex_command(&args)?;

    let mut child =
        crate::commands::claude_runner::spawn_with_memory_limit(&mut cmd).map_err(|e| {
            AppError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to spawn codex: {e}"),
            ))
        })?;

    let full_prompt = format!("{prompt}\n\n{input_text}");
    let stdin_bytes = full_prompt.into_bytes();
    let mut child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::Validation("failed to open codex stdin".into()))?;
    let stdin_thread = std::thread::spawn(move || -> Result<(), std::io::Error> {
        child_stdin.write_all(&stdin_bytes)?;
        drop(child_stdin);
        Ok(())
    });

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let status = child.wait_timeout(timeout).map_err(AppError::Io)?;
    let _ = std::fs::remove_file(&schema_file);

    match status {
        Some(exit_status) => {
            stdin_thread
                .join()
                .map_err(|_| AppError::Validation("stdin thread panicked".into()))?
                .map_err(AppError::Io)?;

            tracing::debug!(
                target: "process",
                exit_code = ?exit_status.code(),
                elapsed_ms = start.elapsed().as_millis() as u64,
                "external process completed"
            );

            let mut stdout_buf = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                std::io::Read::read_to_end(&mut out, &mut stdout_buf).map_err(AppError::Io)?;
            }
            if !exit_status.success() {
                let mut stderr_buf = Vec::new();
                if let Some(mut err) = child.stderr.take() {
                    std::io::Read::read_to_end(&mut err, &mut stderr_buf).map_err(AppError::Io)?;
                }
                let stderr_str = String::from_utf8_lossy(&stderr_buf);
                tracing::warn!(
                    target: "enrich",
                    exit_code = ?exit_status.code(),
                    stderr = %stderr_str.trim(),
                    "codex process failed"
                );
                return Err(AppError::Validation(format!(
                    "codex exited with code {:?}: {}",
                    exit_status.code(),
                    stderr_str.trim()
                )));
            }
            let stdout_str = String::from_utf8(stdout_buf)
                .map_err(|_| AppError::Validation("codex stdout is not valid UTF-8".into()))?;
            // G32: use the JSONL parser, NOT serde_json::from_str on the
            // entire stdout (codex emits one event per line).
            let result = crate::commands::codex_spawn::parse_codex_jsonl(&stdout_str)?;
            // Return the raw agent_message text parsed as JSON. Different
            // operations (memory-bindings, body-enrich) use different
            // output schemas, so we let the caller pick which fields to
            // extract. The previous implementation hardcoded
            // `{entities, urls}` which broke body-enrich.
            let value: serde_json::Value =
                serde_json::from_str(&result.last_agent_text).map_err(|e| {
                    AppError::Validation(format!(
                        "codex agent_message is not valid JSON: {e}; raw={}",
                        result.last_agent_text
                    ))
                })?;
            Ok((value, 0.0, false))
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdin_thread.join();
            Err(AppError::Validation(format!(
                "codex timed out after {timeout_secs} seconds"
            )))
        }
    }
}

pub(super) fn call_opencode(
    binary: &Path,
    prompt: &str,
    json_schema: &str,
    input_text: &str,
    model: Option<&str>,
    timeout_secs: u64,
) -> Result<(serde_json::Value, f64, bool), AppError> {
    use wait_timeout::ChildExt;

    let resolved_model = crate::commands::opencode_runner::resolve_opencode_model(model);

    let augmented_prompt = if json_schema.is_empty() {
        prompt.to_string()
    } else {
        format!(
            "{prompt}\n\nIMPORTANT: You MUST respond with ONLY valid JSON (no markdown, no explanation, no code fences). \
             The JSON MUST match this schema:\n{json_schema}"
        )
    };

    let mut cmd = crate::commands::opencode_runner::build_opencode_command_sync(
        binary,
        &resolved_model,
        &augmented_prompt,
        input_text,
    )?;

    let mut child = crate::commands::opencode_runner::spawn_opencode(&mut cmd).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to spawn opencode: {e}"),
        ))
    })?;

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let status = child.wait_timeout(timeout).map_err(AppError::Io)?;

    match status {
        Some(exit_status) => {
            tracing::debug!(
                target: "process",
                exit_code = ?exit_status.code(),
                elapsed_ms = start.elapsed().as_millis() as u64,
                "opencode process completed"
            );

            let mut stdout_buf = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                std::io::Read::read_to_end(&mut out, &mut stdout_buf).map_err(AppError::Io)?;
            }
            if !exit_status.success() {
                let mut stderr_buf = Vec::new();
                if let Some(mut err) = child.stderr.take() {
                    std::io::Read::read_to_end(&mut err, &mut stderr_buf).map_err(AppError::Io)?;
                }
                let stderr_str = String::from_utf8_lossy(&stderr_buf);
                tracing::warn!(
                    target: "enrich",
                    exit_code = ?exit_status.code(),
                    stderr = %stderr_str.trim(),
                    "opencode process failed"
                );
                return Err(AppError::Validation(format!(
                    "opencode exited with code {:?}: {}",
                    exit_status.code(),
                    stderr_str.trim()
                )));
            }
            let stdout_str = String::from_utf8(stdout_buf)
                .map_err(|_| AppError::Validation("opencode stdout is not valid UTF-8".into()))?;
            let (text, cost, _tokens) =
                crate::commands::opencode_runner::parse_opencode_output(&stdout_str)?;
            let value: serde_json::Value =
                crate::commands::opencode_runner::parse_json_from_opencode_text(&text).map_err(
                    |e| AppError::Validation(format!("opencode response is not valid JSON: {e}")),
                )?;
            Ok((value, cost, false))
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            Err(AppError::Validation(format!(
                "opencode timed out after {timeout_secs} seconds"
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn parse_claude_output_valid_bindings() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":false,"total_cost_usd":0.01,
             "structured_output":{"entities":[{"name":"rust-lang","entity_type":"tool"}],"relationships":[]}}
        ]"#;
        let result = crate::commands::claude_runner::parse_claude_output(output)
            .expect("must parse successfully");
        assert!(result.value.get("entities").is_some());
        assert!((result.cost_usd - 0.01).abs() < f64::EPSILON);
        assert!(!result.is_oauth);
    }

    #[test]
    fn parse_claude_output_detects_oauth() {
        let output = r#"[
            {"type":"system","subtype":"init","apiKeySource":"none"},
            {"type":"result","is_error":false,"total_cost_usd":0.0,
             "structured_output":{"entities":[],"relationships":[]}}
        ]"#;
        let result = crate::commands::claude_runner::parse_claude_output(output).unwrap();
        assert!(result.is_oauth);
    }

    #[test]
    fn parse_claude_output_rate_limit_returns_error() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":true,"error":"rate_limit exceeded"}
        ]"#;
        let err = crate::commands::claude_runner::parse_claude_output(output).unwrap_err();
        assert!(matches!(err, AppError::RateLimited { .. }));
    }

    #[test]
    fn parse_claude_output_auth_error() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":true,"error":"authentication failed"}
        ]"#;
        let err = crate::commands::claude_runner::parse_claude_output(output).unwrap_err();
        assert!(format!("{err}").contains("authentication failed"));
    }

    #[cfg(unix)]
    #[test]
    fn call_codex_returns_raw_json_for_body_enrich_schema() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let binary = tmp.path().join("codex-mock");
        std::fs::write(
            &binary,
            r#"#!/usr/bin/env bash
set -euo pipefail
cat <<'JSONL'
{"type":"thread.started","thread_id":"mock-thread-0"}
{"type":"item.completed","item":{"type":"agent_message","text":"{\"enriched_body\":\"expanded body\"}"}}
{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}
JSONL
"#,
        )
        .expect("mock codex write");
        let mut perms = std::fs::metadata(&binary).expect("metadata").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&binary, perms).expect("chmod");

        let (value, cost, is_oauth) =
            call_codex(&binary, "prompt", BODY_ENRICH_SCHEMA, "body", None, 5)
                .expect("call_codex must accept body-enrich payload");

        assert_eq!(value["enriched_body"], "expanded body");
        assert_eq!(cost, 0.0);
        assert!(!is_oauth);
    }
}
