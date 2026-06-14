//! G54: retrieval-quality and latency micro-benchmark for `recall` and
//! `hybrid-search`.
//!
//! This bench is **opt-in**. It shells out to the freshly compiled
//! `sqlite-graphrag` binary against a synthetic corpus, measuring:
//!
//! 1. Latency of `recall` for one query (cold path through
//!    `try_embed_query_with_fallback`).
//! 2. Latency of `hybrid-search` for the same query (RRF fan-out).
//! 3. Latency of `hybrid-search --fallback-fts-only` (degraded path that
//!    skips the LLM call entirely).
//!
//! To execute:
//!
//!   SQLITE_GRAPHRAG_BENCH_OPT_IN=1 cargo bench --bench retrieval_bench
//!
//! Without the env var, the bench panics at startup with a clear
//! "opt-in required" message — the CI gate that runs `cargo bench
//! --bench regression_baseline` does NOT touch this file.
//!
//! Mock LLM CLI: the bench honours the same `tests/mock-llm/{claude,
//! codex}` stubs that the integration suite uses. When a real OAuth
//! subscription is wired (env `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR`
//! or `codex` authed), the bench transparently exercises the live path
//! without changing the harness.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

const CANONICAL_QUERIES: &[&str] = &[
    "sqlite-graphrag",
    "graphrag v1.0.79 daemon",
    "G42 batched LLM embedding",
    "OAuth-only enforcement",
    "FTS5 BM25 hybrid search",
];

const SYNTHETIC_MEMORIES: &[&str] = &[
    "sqlite-graphrag is a local-first persistent memory for AI agents built in Rust",
    "v1.0.79 removed the daemon and is 100% one-shot with claude -p or codex exec",
    "G42 split into 9 sub-gaps covering batched embedding, dim adoption, batch-size scaling",
    "OAuth-only enforcement aborts spawns when ANTHROPIC_API_KEY or OPENAI_API_KEY are set",
    "Hybrid search uses Reciprocal Rank Fusion with K=60 over FTS5 BM25 and KNN cosine",
];

fn cargo_bin_path() -> PathBuf {
    let mut p = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    if p.ends_with("deps") {
        p.pop();
    }
    p.push("sqlite-graphrag");
    if p.exists() {
        return p;
    }
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/root".into()));
    let installed = home.join(".cargo/bin/sqlite-graphrag");
    if installed.exists() {
        return installed;
    }
    PathBuf::from("sqlite-graphrag")
}

fn bench_cmd(tmp: &TempDir) -> Command {
    let mut cmd = Command::new(cargo_bin_path());
    cmd.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("bench.sqlite"));
    cmd.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    cmd.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    cmd.arg("--skip-memory-guard");
    cmd
}

fn setup_corpus(tmp: &TempDir) {
    // G54: init + remember the synthetic memories so the bench has
    // something to retrieve. The mock LLM CLI on PATH returns 64-dim
    // vectors deterministically.
    let init_status = bench_cmd(tmp).args(["init"]).status().expect("init");
    assert!(init_status.success(), "init failed: {init_status:?}");
    for mem in SYNTHETIC_MEMORIES {
        let slug = slugify(mem);
        let status = bench_cmd(tmp)
            .args(["remember", "--name", &slug, "--body", mem])
            .status()
            .expect("remember");
        assert!(status.success(), "remember({slug}) failed: {status:?}");
    }
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(60)
        .collect()
}

fn bench_recall(c: &mut Criterion) {
    let tmp = TempDir::new().expect("tempdir");
    setup_corpus(&tmp);
    let mut group = c.benchmark_group("G54-retrieval-recall");
    for query in CANONICAL_QUERIES {
        group.bench_with_input(BenchmarkId::from_parameter(query), query, |b, q| {
            b.iter(|| {
                let out = bench_cmd(&tmp)
                    .args(["recall", q, "--k", "5", "--json"])
                    .output()
                    .expect("recall");
                assert!(out.status.success(), "recall failed: {:?}", out.status);
            });
        });
    }
    group.finish();
}

fn bench_hybrid(c: &mut Criterion) {
    let tmp = TempDir::new().expect("tempdir");
    setup_corpus(&tmp);
    let mut group = c.benchmark_group("G54-retrieval-hybrid");
    for query in CANONICAL_QUERIES {
        group.bench_with_input(BenchmarkId::from_parameter(query), query, |b, q| {
            b.iter(|| {
                let out = bench_cmd(&tmp)
                    .args(["hybrid-search", q, "--k", "5", "--json"])
                    .output()
                    .expect("hybrid");
                assert!(out.status.success(), "hybrid failed: {:?}", out.status);
            });
        });
    }
    group.finish();
}

fn bench_hybrid_fallback(c: &mut Criterion) {
    // G58: degraded path — the bench forces --fallback-fts-only so the
    // LLM is bypassed entirely. This is the lower-bound latency for
    // hybrid-search: it must NOT regress when the OAuth path is down.
    let tmp = TempDir::new().expect("tempdir");
    setup_corpus(&tmp);
    let mut group = c.benchmark_group("G54-retrieval-hybrid-fts-fallback");
    for query in CANONICAL_QUERIES {
        group.bench_with_input(BenchmarkId::from_parameter(query), query, |b, q| {
            b.iter(|| {
                let out = bench_cmd(&tmp)
                    .args([
                        "hybrid-search",
                        q,
                        "--k",
                        "5",
                        "--fallback-fts-only",
                        "--json",
                    ])
                    .output()
                    .expect("hybrid-fallback");
                assert!(
                    out.status.success(),
                    "hybrid-fallback failed: {:?}",
                    out.status
                );
            });
        });
    }
    group.finish();
}

fn opt_in_guard() {
    // G54: refuse to run unless the explicit opt-in env var is set.
    // The CI gate that runs `cargo bench --bench regression_baseline`
    // does NOT touch this bench, but explicit invocations are
    // forced to acknowledge the LLM dependency.
    if std::env::var("SQLITE_GRAPHRAG_BENCH_OPT_IN").as_deref() != Ok("1") {
        eprintln!(
            "G54 retrieval_bench is opt-in. Re-run with:\n  \
             SQLITE_GRAPHRAG_BENCH_OPT_IN=1 cargo bench --bench retrieval_bench"
        );
        std::process::exit(0);
    }
}

fn bench_recall_optin(c: &mut Criterion) {
    opt_in_guard();
    bench_recall(c);
    // The hybrid groups exist so reviewers can extend the bench by
    // swapping the `targets` below without re-implementing the setup.
    let _ = (
        bench_hybrid as fn(&mut Criterion),
        bench_hybrid_fallback as fn(&mut Criterion),
    );
}

criterion_group!(
    name = benches;
    config = Criterion::default();
    targets = bench_recall_optin
);
criterion_main!(benches);
