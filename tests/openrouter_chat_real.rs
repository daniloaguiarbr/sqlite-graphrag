//! Live compatibility matrix for the OpenRouter chat JUDGE (v1.0.95).
//!
//! `#[ignore]` by default — it spends real credits on the user's
//! `OPENROUTER_API_KEY`. Run explicitly with:
//!
//! ```sh
//! cargo test --test openrouter_chat_real -- --ignored --nocapture
//! ```
//!
//! It drives the real [`OpenRouterChatClient`] against each candidate model
//! using the production `BINDINGS_SCHEMA` (entities[] + relationships[]) under
//! `strict: true`, then prints a per-model outcome matrix. The test only fails
//! if EVERY model fails — a single non-supporting model is recorded, not fatal,
//! because the matrix itself is the deliverable.

use secrecy::SecretBox;
use sqlite_graphrag::chat_api::OpenRouterChatClient;

/// Mirror of `enrich.rs::BINDINGS_SCHEMA` (the memory-bindings contract).
const BINDINGS_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "entities": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "entity_type": {
            "type": "string",
            "enum": ["project","tool","person","file","concept","incident","decision","organization","location","date"]
          }
        },
        "required": ["name", "entity_type"],
        "additionalProperties": false
      }
    },
    "relationships": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "source": { "type": "string" },
          "target": { "type": "string" },
          "relation": {
            "type": "string",
            "enum": ["applies-to","uses","depends-on","causes","fixes","contradicts","supports","follows","related","replaces","tracked-in"]
          },
          "strength": { "type": "number", "minimum": 0, "maximum": 1 }
        },
        "required": ["source","target","relation","strength"],
        "additionalProperties": false
      }
    }
  },
  "required": ["entities","relationships"],
  "additionalProperties": false
}"#;

const MODELS: &[&str] = &[
    "deepseek/deepseek-v4-flash",
    "deepseek/deepseek-v4-flash:nitro",
    "deepseek/deepseek-v4-pro",
    "google/gemini-3.1-flash-lite",
    "minimax/minimax-m3",
    "minimax/minimax-m2.7",
    "minimax/minimax-m2.7:nitro",
    "openai/gpt-oss-120b",
    "openai/gpt-oss-120b:nitro",
    "xiaomi/mimo-v2.5",
    "xiaomi/mimo-v2.5-pro",
    "z-ai/glm-5.2",
    "z-ai/glm-5.2:nitro",
];

const SYSTEM_PROMPT: &str = "You are a strict JSON extractor. From the user text, \
extract entities and relationships and return ONLY a JSON object with keys \
\"entities\" and \"relationships\" that conforms to the provided schema. Use empty \
arrays when nothing is found. Output no prose.";

const INPUT_TEXT: &str =
    "The sqlite-graphrag project is a CLI tool that depends on OpenRouter for embeddings.";

enum Outcome {
    Passed,
    FailedNoSupport,
    FailedReasoningMandatory,
    OtherError(String),
}

impl Outcome {
    /// Human-readable label that also *reads* the `OtherError` payload, so the
    /// captured error message reaches the matrix output (and the field is not
    /// dead code).
    fn label(&self) -> String {
        match self {
            Outcome::Passed => "passed".to_string(),
            Outcome::FailedNoSupport => "failed-no-support".to_string(),
            Outcome::FailedReasoningMandatory => "failed-reasoning-mandatory".to_string(),
            Outcome::OtherError(e) => format!("other-error: {e}"),
        }
    }
}

fn classify_err(msg: &str) -> Outcome {
    let lower = msg.to_lowercase();
    if lower.contains("reasoning") {
        Outcome::FailedReasoningMandatory
    } else if lower.contains("no structured content")
        || lower.contains("non-json")
        || lower.contains("json_schema")
        || lower.contains("structured output")
        || lower.contains("does not support")
    {
        Outcome::FailedNoSupport
    } else {
        Outcome::OtherError(msg.to_string())
    }
}

#[test]
#[ignore = "live API: spends OPENROUTER_API_KEY credits; run with --ignored"]
fn openrouter_chat_real_model_matrix() {
    let key = match std::env::var("OPENROUTER_API_KEY") {
        Ok(k) if !k.trim().is_empty() => k,
        _ => {
            eprintln!(
                "SKIP: OPENROUTER_API_KEY not set in environment; cannot run the live \
                 13-model compatibility matrix."
            );
            return;
        }
    };

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut results: Vec<(&str, Outcome)> = Vec::with_capacity(MODELS.len());

    for &model in MODELS {
        let client = match OpenRouterChatClient::new(
            SecretBox::new(Box::new(key.clone())),
            model.to_string(),
            300,
        ) {
            Ok(c) => c,
            Err(e) => {
                results.push((model, Outcome::OtherError(e.to_string())));
                continue;
            }
        };

        let outcome = runtime.block_on(async {
            match client
                .complete(SYSTEM_PROMPT, INPUT_TEXT, BINDINGS_SCHEMA, Some(2048))
                .await
            {
                Ok(completion) => {
                    let value = completion.value;
                    let conforms = value.get("entities").is_some_and(|v| v.is_array())
                        && value.get("relationships").is_some_and(|v| v.is_array());
                    if conforms {
                        Outcome::Passed
                    } else {
                        Outcome::FailedNoSupport
                    }
                }
                Err(e) => classify_err(&e.to_string()),
            }
        });

        results.push((model, outcome));
    }

    println!("\n=== OpenRouter chat structured-output compatibility matrix ===");
    let mut passed = 0usize;
    for (model, outcome) in &results {
        if let Outcome::Passed = outcome {
            passed += 1;
        }
        println!("{model:40}  {}", outcome.label());
    }
    println!(
        "=== {}/{} models returned schema-conforming JSON ===\n",
        passed,
        results.len()
    );

    assert!(
        passed > 0,
        "no model returned schema-conforming structured output; see matrix above"
    );
}
