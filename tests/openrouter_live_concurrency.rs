//! Live OpenRouter embedding concurrency regression (v1.0.96).
//!
//! GAP-OPENROUTER-REST-CONCURRENCY: proves the bounded `JoinSet` fan-out
//! (k>1, >32 texts) returns vectors aligned by index to the serial path
//! (k=1) — i.e. chunk order survives out-of-order task completion. This
//! hits the REAL OpenRouter REST API using the repo's `docs/*.md` corpus.
//!
//! Order is verified by cosine similarity per index, NOT byte identity:
//! the serial path embeds all texts in one batch while the fan-out splits
//! them into batches of 32, so the hosted model emits benign ~1e-3 float
//! jitter from batch-composition differences. A genuine order swap moves
//! an index to a semantically different line (cosine well below 0.99),
//! so the per-index threshold cleanly separates jitter from a swap.
//!
//! `#[ignore]` by default. Run explicitly with a resolvable OpenRouter key
//! (env `OPENROUTER_API_KEY`, `config.toml`, or `--openrouter-api-key`):
//!   cargo test --test openrouter_live_concurrency -- --ignored --nocapture

use sqlite_graphrag::cli::{EmbeddingBackendChoice, LlmBackendChoice};

/// Harvests substantial distinct lines from the repo `docs/*.md` corpus so
/// the embedding input exceeds 32 texts and forces the fan-out path.
fn docs_markdown_lines(min: usize) -> Vec<String> {
    let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&docs)
        .expect("docs dir readable")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
        .collect();
    files.sort();

    let mut out: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for f in files {
        let txt = std::fs::read_to_string(&f).unwrap_or_default();
        for line in txt.lines() {
            let t = line.trim();
            if t.len() >= 24 && !t.starts_with("```") && seen.insert(t.to_string()) {
                out.push(t.to_string());
                if out.len() >= min {
                    return out;
                }
            }
        }
    }
    out
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

#[test]
#[ignore = "hits live OpenRouter REST; run with --ignored and a valid key"]
fn fanout_aligns_with_serial_on_live_network() {
    let dim = 384usize;
    let key = match sqlite_graphrag::config::resolve_api_key("openrouter", None) {
        Some(k) => k.value,
        None => {
            eprintln!("SKIP: no OpenRouter API key resolved (env/config/flag)");
            return;
        }
    };
    sqlite_graphrag::embedder::get_openrouter_embedder(key, "qwen/qwen3-embedding-8b", dim)
        .expect("initialise OpenRouter embedding client");

    let texts = docs_markdown_lines(64);
    assert!(
        texts.len() > 32,
        "need >32 texts to exercise the JoinSet fan-out, got {}",
        texts.len()
    );

    // `models_dir` is unused on the OpenRouter path; `batch_size` is ignored
    // there too. k=1 stays serial; k=8 forces the bounded fan-out.
    let models = std::env::temp_dir();
    let serial = sqlite_graphrag::embedder::embed_passages_parallel_with_embedding_choice(
        &models,
        &texts,
        1,
        32,
        EmbeddingBackendChoice::Openrouter,
        LlmBackendChoice::None,
    )
    .expect("serial (k=1) embed");
    let concurrent = sqlite_graphrag::embedder::embed_passages_parallel_with_embedding_choice(
        &models,
        &texts,
        8,
        32,
        EmbeddingBackendChoice::Openrouter,
        LlmBackendChoice::None,
    )
    .expect("concurrent (k=8) embed");

    let n = texts.len();
    assert_eq!(serial.len(), n, "serial length mismatch");
    assert_eq!(concurrent.len(), n, "concurrent length mismatch");

    // Per-index correspondence: cos(serial[i], concurrent[i]) must be ~1.0.
    // A swap would make concurrent[i] a different line, dropping cosine far
    // below the threshold. We also track the worst off-diagonal cosine to
    // prove the diagonal genuinely dominates (lines are distinct enough).
    let mut diag_min = f32::INFINITY;
    let mut offdiag_max = f32::NEG_INFINITY;
    let mut argmax_correct = 0usize;
    for (i, s_i) in serial.iter().enumerate() {
        let mut best_j = 0usize;
        let mut best = f32::NEG_INFINITY;
        for (j, c_j) in concurrent.iter().enumerate() {
            let c = cosine(s_i, c_j);
            if c > best {
                best = c;
                best_j = j;
            }
            if i == j {
                diag_min = diag_min.min(c);
            } else {
                offdiag_max = offdiag_max.max(c);
            }
        }
        if best_j == i {
            argmax_correct += 1;
        }
    }

    assert!(
        diag_min > 0.99,
        "order not preserved: min diagonal cosine {diag_min:.4} <= 0.99 \
         (off-diagonal max {offdiag_max:.4})"
    );
    assert_eq!(
        argmax_correct, n,
        "order not preserved: only {argmax_correct}/{n} indices had the \
         diagonal as the nearest match (diag_min={diag_min:.4}, \
         offdiag_max={offdiag_max:.4})"
    );
    eprintln!(
        "OK: {n} texts, dim={}, k=1 vs k=8 order preserved | diag_min={diag_min:.5} \
         offdiag_max={offdiag_max:.5} argmax_correct={argmax_correct}/{n}",
        serial.first().map(|v| v.len()).unwrap_or(0)
    );
}
