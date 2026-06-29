//! JSON repair for malformed LLM responses (v1.0.97).
//!
//! OpenRouter chat models — notably `deepseek/deepseek-v4-flash:nitro`, which
//! does not reliably honour `json_schema` strict mode — frequently wrap their
//! output in markdown code fences, leave trailing commas, or omit quotes around
//! keys. This module parses such payloads defensively: a strict `serde_json`
//! pass runs first so well-formed responses pay zero repair cost, and only on
//! failure does the `llm_json` repair pass (a Rust port of the Python
//! `json_repair` library) run before a second parse attempt.

use llm_json::{loads, RepairOptions};
use serde_json::Value;

/// Parse `input` into a [`serde_json::Value`], repairing common LLM JSON
/// defects when a strict parse fails.
///
/// Strategy:
/// 1. Try `serde_json::from_str` directly — the fast path for valid JSON.
/// 2. On failure, run `llm_json::loads`, which repairs the string (markdown
///    fences, trailing commas, unquoted keys, missing brackets) and parses it
///    to a `Value` in a single pass.
/// 3. Return an error only when `llm_json` itself fails (an I/O or UTF-8
///    fault). `llm_json` coerces aggressively — arbitrary text becomes a JSON
///    string, empty input becomes `{}`, and a lone delimiter becomes `null` —
///    so callers MUST validate the returned `Value`'s shape rather than
///    relying on `Err` for semantically-wrong-but-parseable input.
pub fn repair_to_value(input: &str) -> anyhow::Result<Value> {
    match serde_json::from_str::<Value>(input) {
        Ok(value) => Ok(value),
        Err(strict_err) => loads(input, &RepairOptions::default()).map_err(|repair_err| {
            anyhow::anyhow!(
                "failed to parse JSON even after repair: strict error = {strict_err}; \
                 repair error = {repair_err}"
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_already_valid_json_unchanged() {
        let value = repair_to_value(r#"{"name":"qwen","dim":384}"#).unwrap();
        assert_eq!(value["name"], "qwen");
        assert_eq!(value["dim"], 384);
    }

    #[test]
    fn repairs_unquoted_keys_and_trailing_comma() {
        // Typical LLM defect: single-quoted strings, unquoted key, trailing comma.
        let value = repair_to_value(r#"{name: 'John', age: 30,}"#).unwrap();
        assert_eq!(value["name"], "John");
        assert_eq!(value["age"], 30);
    }

    #[test]
    fn repairs_markdown_fenced_payload() {
        // Models often wrap JSON in a ```json code fence.
        let fenced = "```json\n{\"entities\": [\"rust\", \"sqlite\"]}\n```";
        let value = repair_to_value(fenced).unwrap();
        assert_eq!(value["entities"][0], "rust");
        assert_eq!(value["entities"][1], "sqlite");
    }

    #[test]
    fn coerces_non_json_text_into_a_value() {
        // `llm_json` repairs aggressively: free text becomes a JSON string and
        // empty input becomes an empty object. `repair_to_value` therefore
        // returns a `Value` instead of erroring, so callers must validate shape.
        let text = repair_to_value("this is not json at all <<<").unwrap();
        assert_eq!(
            text,
            Value::String("this is not json at all <<<".to_string())
        );

        let empty = repair_to_value("").unwrap();
        assert_eq!(empty, serde_json::json!({}));
    }
}
