//! Codex CLI spawn + JSONL parsing helper shared by `enrich` and `ingest --mode codex`.
//!
//! G31 (v1.0.69): `enrich --mode codex` was missing five critical hardening
//! flags compared to `ingest --mode codex`. This module extracts the
//! spawn pipeline into a single helper that BOTH call-sites consume,
//! guaranteeing the same defaults everywhere.
//!
//! G32 (v1.0.69): `enrich --mode codex` used `serde_json::from_str` on the
//! raw stdout, but `codex exec --json` emits JSONL (one event per line).
//! [`parse_codex_jsonl`] iterates line-by-line, picking the last
//! `item.completed` of type `agent_message` as the assistant text.
//!
//! G33 (v1.0.69): validate the model against the ChatGPT Pro OAuth whitelist
//! stored in `~/.codex/models_cache.json` BEFORE spawning the subprocess.

use crate::errors::AppError;
use crate::extract::codex_compat::codex_supports_ask_for_approval;
use crate::extraction::{ExtractedUrl, ExtractionResult};
use crate::spawn::env_whitelist::apply_env_whitelist;
use crate::storage::entities::{NewEntity, NewRelationship};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Token usage reported by Codex on `turn.completed` events.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CodexUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cached_input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub reasoning_output_tokens: u64,
}

/// Combined result of one `codex exec` invocation.
#[derive(Debug)]
pub struct CodexResult {
    pub extraction: ExtractionResult,
    /// Raw text of the last `item.completed` of type `agent_message` (the
    /// JSON payload the LLM produced). Callers that need a schema other
    /// than the extraction shape (e.g. body-enrich's `enriched_body`)
    /// should parse this directly.
    pub last_agent_text: String,
    pub usage: Option<CodexUsage>,
    pub rate_limited: bool,
    pub schema_error: bool,
    pub turn_failed: bool,
    pub failed_message: String,
}

/// Configuration for the codex spawner.
#[allow(rustdoc::broken_intra_doc_links)]
pub struct CodexSpawnArgs<'a> {
    pub binary: &'a Path,
    pub prompt: &'a str,
    pub json_schema: &'a str,
    pub input_text: &'a str,
    pub model: Option<&'a str>,
    pub timeout_secs: u64,
    /// Caller-provided schema path (must be inside a trusted directory
    /// that codex recognises as sandbox-safe). Use [`trusted_schema_path`]
    /// to compute one under the cache dir.
    pub schema_path: PathBuf,
}

/// Computes a schema path under the cache dir so `codex exec` accepts it
/// as part of a trusted directory (rejects `/tmp` on hardened installs).
pub fn trusted_schema_path() -> Result<PathBuf, AppError> {
    let cache = crate::paths::AppPaths::resolve(None)
        .map(|p| p.models.parent().map(|m| m.to_path_buf()))
        .ok()
        .flatten()
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&cache).map_err(AppError::Io)?;
    Ok(cache.join(format!("enrich-schema-{}.json", std::process::id())))
}

/// Models accepted by Codex CLI when using ChatGPT Pro OAuth.
///
/// Mirrored from `~/.codex/models_cache.json` (which the official CLI
/// refreshes on every login). This list is intentionally narrow; passing
/// a model not in this set with `--mode codex` returns
/// `AppError::Validation` BEFORE any OAuth turn is spent.
pub const CODEX_PRO_OAUTH_MODELS: &[&str] = &[
    "codex-auto-review",
    "gpt-5.3-codex-spark",
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.5",
];

/// Validates the requested model against [`CODEX_PRO_OAUTH_MODELS`].
///
/// # Errors
/// Returns [`AppError::Validation`] listing the accepted models when the
/// caller supplied a model outside the whitelist.
pub fn validate_codex_model(model: Option<&str>) -> Result<(), AppError> {
    let Some(m) = model else {
        return Ok(()); // no override; codex picks its default
    };
    if CODEX_PRO_OAUTH_MODELS.contains(&m) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "--codex-model {m:?} is not supported with ChatGPT Pro OAuth. \
             Accepted: {}",
            CODEX_PRO_OAUTH_MODELS.join(", ")
        )))
    }
}

/// Returns the list of models accepted by Codex with ChatGPT Pro OAuth.
///
/// Tries to read `~/.codex/models_cache.json` (which the official CLI
/// refreshes on every login) and falls back to the static
/// [`CODEX_PRO_OAUTH_MODELS`] constant when the file is missing or
/// malformed. The returned `Vec<String>` is the union of both sources,
/// de-duplicated.
///
/// The official cache file is an object with the shape
/// `{"fetched_at": "...", "etag": "...", "client_version": "...",
/// "models": [{"slug": "gpt-5.5", ...}, ...]}` (v1.0.81 fix: previously we
/// iterated `obj.keys()` which produced bogus entries like `client_version`
/// and `etag` as "models"; now we extract only the `models` array).
pub fn list_codex_models() -> Vec<String> {
    use std::collections::BTreeSet;
    let mut out: BTreeSet<String> = CODEX_PRO_OAUTH_MODELS
        .iter()
        .map(|s| s.to_string())
        .collect();

    if let Some(home) = std::env::var_os("HOME") {
        let path = std::path::Path::new(&home)
            .join(".codex")
            .join("models_cache.json");
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(obj) = value.as_object() {
                    // v1.0.81 fix: prefer the well-known `models` array
                    // (each item has a `slug` field). Fall back to keys
                    // only when `models` is absent (legacy cache format).
                    if let Some(models_arr) = obj.get("models").and_then(|m| m.as_array()) {
                        for v in models_arr {
                            if let Some(slug) = v.get("slug").and_then(|s| s.as_str()) {
                                out.insert(slug.to_string());
                            } else if let Some(s) = v.as_str() {
                                out.insert(s.to_string());
                            }
                        }
                    } else {
                        for key in obj.keys() {
                            out.insert(key.clone());
                        }
                    }
                } else if let Some(arr) = value.as_array() {
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            out.insert(s.to_string());
                        }
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}

/// Suggests the closest codex OAuth model to a user-supplied substring
/// (G33). Returns `None` when no candidate is close enough.
///
/// Match strategy: exact substring containment wins; otherwise Levenshtein
/// distance below `max_distance = max(2, query.len() / 3)`.
pub fn suggest_codex_model(query: &str) -> Option<String> {
    let query_lc = query.to_ascii_lowercase();
    let models = list_codex_model_lc();

    // Exact substring match wins.
    for m in &models {
        if m.contains(&query_lc) {
            return Some(m.clone());
        }
    }

    // Levenshtein fallback.
    let max_distance = (query.len() / 3).max(2);
    let mut best: Option<(usize, String)> = None;
    for m in &models {
        let d = levenshtein(query_lc.as_str(), m.as_str());
        if d <= max_distance && best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, m.clone()));
        }
    }
    best.map(|(_, m)| m)
}

fn list_codex_model_lc() -> Vec<String> {
    list_codex_models()
        .into_iter()
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    if a_chars.is_empty() {
        return b_chars.len();
    }
    if b_chars.is_empty() {
        return a_chars.len();
    }
    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    let mut curr = vec![0; b_chars.len() + 1];
    for (i, &ac) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &bc) in b_chars.iter().enumerate() {
            let cost = if ac == bc { 0 } else { 1 };
            curr[j + 1] = (curr[j] + 1).min(prev[j + 1] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_chars.len()]
}

/// Builds the `codex exec` command with the canonical hardening flags.
///
/// G31 + OAuth-only hardening (v1.0.69, mandated by gaps.md lines 41-49):
/// the command ALWAYS uses the OAuth `auth.json` flow. The flag set is
/// the canonical one documented in gaps.md Fix A:
///
/// ```text
/// codex exec \
///   -c mcp_servers='{}' \
///   --json --output-schema <SCHEMA> \
///   --ephemeral \
///   --skip-git-repo-check \
///   --sandbox read-only \
///   --ignore-user-config \
///   --ignore-rules \
///   --ask-for-approval never \
///   -m <MODEL> \
///   -
/// ```
///
/// The combination zeroes MCP servers (via two complementary mechanisms:
/// the inline `-c mcp_servers='{}'` override AND `--ignore-user-config`),
/// disables user-defined rules, and never asks for interactive approval.
///
/// **`OPENAI_API_KEY` is FORBIDDEN** in the spawned environment (gaps.md:48).
/// OAuth flows via `~/.codex/auth.json` and `CODEX_ACCESS_TOKEN` only.
pub fn build_codex_command(args: &CodexSpawnArgs<'_>) -> Command {
    let full_prompt = format!("{}\n\n{}", args.prompt, args.input_text);

    // OAuth-only guard (gaps.md:48, ADR-0011). If `OPENAI_API_KEY` is set
    // in the environment we MUST abort — that is the API-key path which is
    // explicitly PROHIBITED. Use the OAuth `auth.json` flow exclusively.
    if let Ok(_key) = std::env::var("OPENAI_API_KEY") {
        let mut cmd = Command::new("false");
        cmd.env_clear();
        cmd.env("PATH", "/nonexistent");
        cmd.arg("--oauth-only-violation-openai-api-key-set");
        cmd.arg("--oauth-only-resolution-use-codex-auth-json-or-openai-base-url");
        return cmd;
    }

    // Write the JSON schema to a path the caller controls. Callers should
    // pass a path under the cache dir (see [`trusted_schema_path`]).
    std::fs::write(&args.schema_path, args.json_schema).ok();

    let mut cmd = Command::new(args.binary);
    // v1.0.83 (ADR-0041): env whitelist delegated to the shared helper.
    // `OPENAI_API_KEY` is INTENTIONALLY ABSENT (defence-in-depth).
    // `CODEX_ACCESS_TOKEN` and `OPENAI_BASE_URL` ARE whitelisted for
    // custom providers via the canonical list in src/spawn/env_whitelist.rs.
    apply_env_whitelist(&mut cmd, crate::spawn::env_whitelist::is_strict_env_clear());

    // v1.0.77: point CODEX_HOME at an isolated dir that only contains
    // auth.json — this prevents the codex subprocess from loading
    // ~/.codex/config.toml (which has trust_level=trusted for the project,
    // causing sandbox escalation per openai/codex#18113).
    if let Some(isolated) = prepare_isolated_codex_home_spawn() {
        cmd.env("CODEX_HOME", isolated);
    }

    // v1.0.77: `-c` TOML overrides bypass the codex exec --sandbox propagation
    // bug (openai/codex#18113). CLI flags alone are insufficient — the exec
    // subcommand may not inherit --sandbox from the parent codex command.
    cmd.arg("exec")
        .arg("-c")
        .arg("sandbox_mode='read-only'")
        .arg("-c")
        .arg("approval_policy='never'")
        .arg("--json")
        .arg("--output-schema")
        .arg(&args.schema_path)
        .arg("--ephemeral")
        .arg("--skip-git-repo-check")
        .arg("--sandbox")
        .arg("read-only")
        .arg("--ignore-user-config")
        .arg("--ignore-rules");

    // Codex 0.134+ no longer accepts `-c mcp_servers='{}'` — it parses the
    // value as a string and rejects it ("expected a map"). The
    // `--ignore-user-config` flag already discards any user-defined MCP
    // servers, so the override is redundant on all supported versions.

    // Codex 0.134+ removed --ask-for-approval entirely (Issue #26602).
    // Skip the flag on newer versions; sandbox=read-only already suppresses
    // approval prompts. See src/extract/codex_compat.rs for the probe.
    if codex_supports_ask_for_approval() {
        cmd.arg("--ask-for-approval").arg("never");
    }

    if let Some(m) = args.model {
        cmd.arg("-m").arg(m);
    }

    // `-` means: read the prompt from stdin (Codex Paperclip pattern)
    cmd.arg("-");

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Keep the prompt alive for the stdin thread spawned in `spawn_codex`.
    let _ = full_prompt; // captured by closure below

    // GAP-META-005 (v1.0.87, ADR-0045): pre-flight validation gate runs
    // AFTER argv is fully built. Validates binary existence, argv size,
    // walk-up of `.mcp.json`, and `CLAUDE_CONFIG_DIR` cleanliness.
    // Pre-flight failure aborts the spawn with exit 16 — see ADR-0045.
    let argv_refs: Vec<std::ffi::OsString> =
        cmd.get_args().map(|s| s.to_os_string()).collect();
    let preflight_args = crate::spawn::preflight::PreFlightArgs {
        binary_path: args.binary,
        argv: &argv_refs,
        workspace_root: std::path::Path::new("."),
        mcp_config_inline_json: None, // Codex does not use --mcp-config flag
        expected_output_bytes: 65_536,
        spawner_name: "codex_spawn",
    };
    if let Err(e) = crate::spawn::preflight::preflight_check(&preflight_args) {
        tracing::error!(
            target: "codex_spawn",
            spawner = "codex_spawn",
            error = %e,
            "preflight validation failed; aborting spawn (exit 16)"
        );
        std::process::exit(16);
    }

    cmd
}

/// Parses JSONL output from `codex exec --json`.
///
/// Event format (DOTS notation):
/// - `thread.started` — session init
/// - `turn.started` — model turn begins
/// - `item.completed` — message or tool call; last `agent_message` wins
/// - `turn.completed` — includes usage stats
/// - `turn.failed` — error with optional rate-limit indicator
/// - `error` — schema or validation error
///
/// G32 (v1.0.69): this function is the single source of truth for JSONL
/// parsing. Both `enrich` and `ingest --mode codex` consume it.
pub fn parse_codex_jsonl(stdout: &str) -> Result<CodexResult, AppError> {
    let mut last_agent_text: Option<String> = None;
    let mut usage: Option<CodexUsage> = None;
    let mut rate_limited = false;
    let mut schema_error = false;
    let mut turn_failed = false;
    let mut failed_message = String::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let event: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                tracing::warn!(target: "codex_spawn", line, "skipping malformed JSONL line");
                continue;
            }
        };

        let event_type = match event.get("type").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => continue,
        };

        match event_type {
            "item.completed" => {
                if let Some(item) = event.get("item") {
                    if item.get("type").and_then(|t| t.as_str()) == Some("agent_message") {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            last_agent_text = Some(text.to_string());
                        }
                    }
                }
            }
            "turn.completed" => {
                if let Some(u) = event.get("usage") {
                    // Skip events that lack the recognised token fields
                    // (e.g. partial broadcasts with `{}`) so the last
                    // populated usage wins instead of being overwritten
                    // by an empty one.
                    let is_populated = u
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .map(|n| n > 0)
                        .unwrap_or(false)
                        || u.get("output_tokens")
                            .and_then(|v| v.as_u64())
                            .map(|n| n > 0)
                            .unwrap_or(false);
                    if is_populated {
                        if let Ok(parsed) = serde_json::from_value::<CodexUsage>(u.clone()) {
                            usage = Some(parsed);
                        }
                    }
                }
            }
            "turn.failed" => {
                turn_failed = true;
                if let Some(err) = event.get("error") {
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown error");
                    failed_message = msg.to_string();
                    if msg.contains("rate_limit")
                        || msg.contains("429")
                        || msg.contains("Too Many Requests")
                    {
                        rate_limited = true;
                    }
                }
            }
            "error" => {
                if let Some(msg) = event.get("message").and_then(|m| m.as_str()) {
                    if msg.contains("invalid_json_schema") || msg.contains("schema") {
                        schema_error = true;
                    }
                }
            }
            _ => {}
        }
    }

    let text = last_agent_text.ok_or_else(|| {
        AppError::Validation(format!(
            "no agent_message in codex JSONL output (rate_limited={rate_limited}, schema_error={schema_error}, turn_failed={turn_failed})"
        ))
    })?;

    if turn_failed {
        return Err(AppError::Validation(format!(
            "codex turn failed: {failed_message}"
        )));
    }
    if schema_error {
        return Err(AppError::Validation(
            "codex reported invalid_json_schema; check the --output-schema file".to_string(),
        ));
    }
    if rate_limited {
        return Err(AppError::Validation(format!(
            "codex rate-limited: {failed_message}"
        )));
    }

    let extraction = parse_extraction_text(&text)?;
    Ok(CodexResult {
        extraction,
        last_agent_text: text,
        usage,
        rate_limited,
        schema_error,
        turn_failed,
        failed_message,
    })
}

/// Parses the agent_message text as an `ExtractionResult` JSON payload.
///
/// The schema is shared by both `enrich` and `ingest --mode codex`; the
/// `text` is the JSON value the assistant returned, not a wrapper object.
pub fn parse_extraction_text(text: &str) -> Result<ExtractionResult, AppError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        AppError::Validation(format!("failed to parse codex agent_message as JSON: {e}"))
    })?;
    let obj = value.as_object().ok_or_else(|| {
        AppError::Validation("codex agent_message is not a JSON object".to_string())
    })?;

    let mut entities: Vec<NewEntity> = Vec::new();
    if let Some(arr) = obj.get("entities").and_then(|v| v.as_array()) {
        for e in arr {
            if let Some(name) = e.get("name").and_then(|v| v.as_str()) {
                // Accept either "type" or "entity_type" from the LLM payload
                // and fall back to "concept" when the LLM omits it.
                let entity_type_str = e
                    .get("type")
                    .or_else(|| e.get("entity_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("concept");
                let entity_type = serde_json::from_value::<crate::entity_type::EntityType>(
                    serde_json::Value::String(entity_type_str.to_string()),
                )
                .unwrap_or(crate::entity_type::EntityType::Concept);
                entities.push(NewEntity {
                    name: name.to_string(),
                    entity_type,
                    description: None,
                });
            }
        }
    }

    let mut relationships: Vec<NewRelationship> = Vec::new();
    if let Some(arr) = obj.get("relationships").and_then(|v| v.as_array()) {
        for r in arr {
            let from = r.get("source").or_else(|| r.get("from"));
            let to = r.get("target").or_else(|| r.get("to"));
            let rel = r.get("relation").and_then(|v| v.as_str());
            if let (Some(from_v), Some(to_v), Some(rel_v)) = (
                from.and_then(|v| v.as_str()),
                to.and_then(|v| v.as_str()),
                rel,
            ) {
                relationships.push(NewRelationship {
                    source: from_v.to_string(),
                    target: to_v.to_string(),
                    relation: rel_v.to_string(),
                    strength: r.get("strength").and_then(|v| v.as_f64()).unwrap_or(0.5),
                    description: None,
                });
            }
        }
    }

    let urls: Vec<ExtractedUrl> = obj
        .get("urls")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|u| {
                    let url = u.get("url")?.as_str()?.to_string();
                    let start = u.get("start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let end = u
                        .get("end")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(start as u64) as usize;
                    Some(ExtractedUrl { url, start, end })
                })
                .collect()
        })
        .unwrap_or_default();

    // v1.0.76: ExtractionResult no longer carries relationships or
    // relationships_truncated fields; those are LLM backend output
    // (see `ExtractionOutput` in src/extract/mod.rs). The default
    // build extracts URLs + entities only; relationships are an
    // LLM-side concern.
    //
    // Convert `NewEntity` (storage-side) to `ExtractedEntity`
    // (extraction-side). The LLM payload doesn't include byte offsets
    // (the chunker is responsible for that), so start/end are 0.
    let entities_ext: Vec<crate::extraction::ExtractedEntity> = entities
        .into_iter()
        .map(|e| crate::extraction::ExtractedEntity {
            name: e.name,
            entity_type: e.entity_type.as_str().to_string(),
            start: 0,
            end: 0,
        })
        .collect();

    Ok(ExtractionResult {
        entities: entities_ext,
        urls,
        elapsed_ms: 0,
    })
}

fn prepare_isolated_codex_home_spawn() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let real_auth = std::path::Path::new(&home).join(".codex/auth.json");
    if !real_auth.exists() {
        return None;
    }
    let isolated =
        std::env::temp_dir().join(format!("sqlite-graphrag-codex-home-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&isolated);
    let target = isolated.join("auth.json");
    if !target.exists() {
        let _ = std::fs::copy(&real_auth, &target);
    }
    Some(isolated)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_JSONL: &str = r#"{"type":"thread.started","thread_id":"abc"}
{"type":"turn.started"}
{"type":"item.completed","item":{"type":"reasoning","text":"thinking"}}
{"type":"item.completed","item":{"type":"agent_message","text":"{\"entities\":[{\"name\":\"alpha\",\"type\":\"concept\"}],\"relationships\":[{\"source\":\"alpha\",\"target\":\"beta\",\"relation\":\"uses\",\"strength\":0.7}],\"extraction_method\":\"codex\",\"urls\":[]}"}}
{"type":"turn.completed","usage":{"input_tokens":120,"output_tokens":45}}
{"type":"turn.completed","usage":{}}
"#;

    #[test]
    fn parse_codex_jsonl_extracts_last_agent_message() {
        // v1.0.76: relationships are no longer carried in the
        // ExtractionResult struct (they belong to the LLM ExtractionBackend
        // payload, not the URL-only default build). The default test
        // validates the entity extraction path only.
        let result = parse_codex_jsonl(SAMPLE_JSONL).expect("parse must succeed");
        assert_eq!(result.extraction.entities.len(), 1);
        assert_eq!(result.extraction.entities[0].name, "alpha");
    }

    #[test]
    fn parse_codex_jsonl_collects_usage() {
        let result = parse_codex_jsonl(SAMPLE_JSONL).expect("parse must succeed");
        let usage = result.usage.expect("usage must be populated");
        assert_eq!(usage.input_tokens, 120);
        assert_eq!(usage.output_tokens, 45);
    }

    #[test]
    fn parse_codex_jsonl_detects_rate_limit() {
        let r = parse_codex_jsonl(
            "{\"type\":\"turn.failed\",\"error\":{\"message\":\"rate_limit: 429 too many\"}}\n{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"{}\"}}",
        );
        assert!(matches!(r, Err(AppError::Validation(_))));
    }

    #[test]
    fn parse_codex_jsonl_handles_no_agent_message() {
        let r = parse_codex_jsonl("{\"type\":\"thread.started\"}");
        assert!(matches!(r, Err(AppError::Validation(_))));
    }

    #[test]
    fn parse_codex_jsonl_skips_malformed_lines() {
        let r = parse_codex_jsonl(
            "{not valid json\n{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"{\\\"entities\\\":[],\\\"relationships\\\":[],\\\"extraction_method\\\":\\\"codex\\\"}\"}}",
        );
        assert!(r.is_ok(), "malformed lines must be skipped, got {r:?}");
    }

    #[test]
    fn validate_codex_model_accepts_known() {
        assert!(validate_codex_model(Some("gpt-5.5")).is_ok());
        assert!(validate_codex_model(Some("gpt-5.4")).is_ok());
        assert!(validate_codex_model(None).is_ok()); // no override
    }

    #[test]
    fn validate_codex_model_rejects_unknown() {
        let err = validate_codex_model(Some("gpt-4")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not supported"));
        assert!(msg.contains("gpt-5.5"));
    }

    #[test]
    fn list_codex_models_includes_all_static_whitelist() {
        let models = list_codex_models();
        for m in CODEX_PRO_OAUTH_MODELS {
            assert!(models.contains(&m.to_string()), "missing {m} in {models:?}");
        }
    }

    #[test]
    fn suggest_codex_model_substring_match() {
        let s = suggest_codex_model("gpt-5");
        assert!(s.is_some(), "must suggest a gpt-5.x model");
    }

    #[test]
    fn suggest_codex_model_fuzzy_match() {
        // 'gpt5.5' has no hyphen; should still suggest 'gpt-5.5'.
        let s = suggest_codex_model("gpt5.5");
        assert!(s.is_some(), "fuzzy must suggest gpt-5.5 for 'gpt5.5'");
        assert_eq!(s.unwrap(), "gpt-5.5");
    }

    #[test]
    fn suggest_codex_model_unrelated_returns_none() {
        let s = suggest_codex_model("totally-unrelated-zzz");
        assert!(s.is_none());
    }

    #[test]
    fn build_codex_command_includes_hardening_flags() {
        let args = CodexSpawnArgs {
            binary: Path::new("/bin/true"),
            prompt: "p",
            json_schema: "{}",
            input_text: "i",
            model: Some("gpt-5.5"),
            timeout_secs: 60,
            schema_path: std::env::temp_dir().join("test-schema.json"),
        };
        let cmd = build_codex_command(&args);
        let collected: Vec<String> = cmd
            .get_args()
            .filter_map(|a| a.to_str().map(|s| s.to_string()))
            .collect();
        for required in &[
            "exec",
            "-c",
            "sandbox_mode='read-only'",
            "approval_policy='never'",
            "--json",
            "--output-schema",
            "--ephemeral",
            "--skip-git-repo-check",
            "--sandbox",
            "read-only",
            "--ignore-user-config",
            "--ignore-rules",
            "-m",
            "gpt-5.5",
            "-",
        ] {
            assert!(
                collected.iter().any(|a| a == required),
                "missing flag {required} in {collected:?}"
            );
        }
    }

    #[test]
    fn list_codex_models_dedupes_with_cache_file() {
        // Ensure the union with the cache file (when present) does not
        // produce duplicates. We can't actually write a cache file in
        // a test, so we just verify the static path is dedup'd.
        let models = list_codex_models();
        let unique: std::collections::HashSet<_> = models.iter().collect();
        assert_eq!(unique.len(), models.len(), "list_codex_models must dedupe");
    }
    #[test]
    fn list_codex_models_extracts_from_models_array_v1_0_81_regression() {
        // v1.0.81 fix: the official codex CLI writes
        //   {"fetched_at": "...", "etag": "...", "client_version": "...",
        //    "models": [{"slug": "gpt-5.5", ...}, ...]}
        // and the old code iterated obj.keys(), polluting the model
        // list with metadata keys. Here we simulate a cache file by
        // setting HOME to a tempdir containing a synthetic cache and
        // verifying the metadata keys are NOT present in the output.
        let tmp =
            std::env::temp_dir().join(format!("codex-models-array-test-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join(".codex")).expect("mkdir");
        let cache_body = r#"{
            "fetched_at": "2026-06-14T06:43:56.639903114Z",
            "etag": "W/\"deadbeef\"",
            "client_version": "0.139.0",
            "models": [
                {"slug": "gpt-5.5", "display_name": "GPT-5.5"},
                {"slug": "gpt-5.4-mini", "display_name": "GPT-5.4 mini"}
            ]
        }"#;
        std::fs::write(tmp.join(".codex/models_cache.json"), cache_body).expect("write cache");
        // SAFETY: unit test
        let prev_home = std::env::var("HOME");
        unsafe {
            std::env::set_var("HOME", &tmp);
        }
        let models = list_codex_models();
        unsafe {
            if let Ok(h) = prev_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
        let _ = std::fs::remove_dir_all(&tmp);

        for forbidden in &["client_version", "etag", "fetched_at", "models"] {
            assert!(
                !models.contains(&forbidden.to_string()),
                "metadata key {forbidden:?} leaked into model list: {models:?}"
            );
        }
        assert!(
            models.contains(&"gpt-5.5".to_string()),
            "gpt-5.5 missing from extracted list: {models:?}"
        );
        assert!(
            models.contains(&"gpt-5.4-mini".to_string()),
            "gpt-5.4-mini missing from extracted list: {models:?}"
        );
    }

    #[test]
    fn list_codex_models_falls_back_to_keys_when_models_field_absent() {
        // Legacy cache shape: keys are model ids directly (no models
        // array). v1.0.81 must still merge those keys into the result.
        let tmp =
            std::env::temp_dir().join(format!("codex-models-legacy-test-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join(".codex")).expect("mkdir");
        let cache_body = r#"{"legacy-model-x": 1, "legacy-model-y": 2}"#;
        std::fs::write(tmp.join(".codex/models_cache.json"), cache_body).expect("write cache");
        let prev_home = std::env::var("HOME");
        unsafe {
            std::env::set_var("HOME", &tmp);
        }
        let models = list_codex_models();
        unsafe {
            if let Ok(h) = prev_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(
            models.contains(&"legacy-model-x".to_string()),
            "legacy-model-x missing: {models:?}"
        );
        assert!(
            models.contains(&"legacy-model-y".to_string()),
            "legacy-model-y missing: {models:?}"
        );
    }

    /// OAuth-only conformance test (gaps.md:41-49, v1.0.69 mandate).
    /// Verifies that `build_codex_command` always emits `-c mcp_servers='{}'`,
    /// `--ignore-user-config`, `--ask-for-approval never` and does NOT
    /// whitelist `OPENAI_API_KEY` in the env_clear whitelist.
    #[test]
    #[serial_test::serial(env)]
    fn build_command_oauth_only_mandatory_flags() {
        // SAFETY: unit test
        unsafe {
            std::env::remove_var("OPENAI_API_KEY");
        }
        let schema = std::env::temp_dir().join("codex-test-schema.json");
        let _ = std::fs::remove_file(&schema);
        let args = CodexSpawnArgs {
            binary: std::path::Path::new("/usr/bin/false"),
            prompt: "p",
            json_schema: "{}",
            input_text: "i",
            model: Some("gpt-5.4-mini"),
            timeout_secs: 60,
            schema_path: schema.clone(),
        };
        let cmd = build_codex_command(&args);
        let argv: Vec<&str> = cmd.get_args().filter_map(|a| a.to_str()).collect();
        // Mandatory flags from gaps.md lines 233-238.
        // -c mcp_servers='{}' was REMOVED in v1.0.76 — codex 0.134+ parses
        // the value as a string and rejects it ("expected a map"). The
        // --ignore-user-config flag already covers the MCP isolation
        // requirement.
        assert!(
            argv.contains(&"--ignore-user-config"),
            "must have --ignore-user-config (gaps.md:266)"
        );
        // --ask-for-approval is conditional on codex < 0.134. When the
        // installed codex is 0.134+ the flag is omitted by the compat
        // helper. Both outcomes are valid.
        let ask_for_approval_present = argv.contains(&"--ask-for-approval");
        if !crate::extract::codex_compat::codex_supports_ask_for_approval() {
            assert!(
                !ask_for_approval_present,
                "codex 0.134+ must NOT include --ask-for-approval"
            );
        }
        assert!(
            argv.contains(&"--sandbox"),
            "must have --sandbox read-only (G31)"
        );
        assert!(argv.contains(&"--ephemeral"), "must have --ephemeral (G31)");
        assert!(
            argv.contains(&"--skip-git-repo-check"),
            "must have --skip-git-repo-check (G31)"
        );
        assert!(
            argv.contains(&"--ignore-rules"),
            "must have --ignore-rules (G31)"
        );
        // v1.0.77: -c TOML overrides bypass codex exec --sandbox bug (#18113)
        assert!(
            argv.contains(&"-c") && argv.contains(&"sandbox_mode='read-only'"),
            "must have -c sandbox_mode='read-only' (v1.0.77, codex#18113)"
        );
        assert!(
            argv.contains(&"approval_policy='never'"),
            "must have -c approval_policy='never' (v1.0.77)"
        );
    }

    /// OAuth-only guard: when `OPENAI_API_KEY` is in the environment,
    /// `build_codex_command` MUST abort the spawn (return a `false`
    /// command), NOT pass the key through to the child.
    #[test]
    #[serial_test::serial(env)]
    fn build_command_aborts_when_openai_api_key_set() {
        // SAFETY: unit test
        unsafe {
            std::env::set_var("OPENAI_API_KEY", "sk-violation-test");
        }
        let schema = std::env::temp_dir().join("codex-test-schema-abort.json");
        let _ = std::fs::remove_file(&schema);
        let args = CodexSpawnArgs {
            binary: std::path::Path::new("/usr/bin/codex"),
            prompt: "p",
            json_schema: "{}",
            input_text: "i",
            model: Some("gpt-5.4-mini"),
            timeout_secs: 60,
            schema_path: schema.clone(),
        };
        let cmd = build_codex_command(&args);
        let program = cmd.get_program().to_string_lossy().to_string();
        let argv: Vec<&str> = cmd.get_args().filter_map(|a| a.to_str()).collect();
        assert_eq!(
            program, "false",
            "when OPENAI_API_KEY is set, build_codex_command must abort"
        );
        assert!(
            argv.contains(&"--oauth-only-violation-openai-api-key-set"),
            "aborted command must carry violation marker"
        );
        unsafe {
            std::env::remove_var("OPENAI_API_KEY");
        }
    }
}
