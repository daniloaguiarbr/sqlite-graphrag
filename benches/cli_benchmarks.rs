use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers de ambiente isolado
// ---------------------------------------------------------------------------

fn cargo_bin_path() -> PathBuf {
    // Tenta localizar o binário compilado no diretório de target
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
    // Fallback: ~/.cargo/bin/sqlite-graphrag já instalado
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/root".into()));
    let installed = home.join(".cargo/bin/sqlite-graphrag");
    if installed.exists() {
        return installed;
    }
    PathBuf::from("sqlite-graphrag")
}

fn sqlite_graphrag_cmd(tmp: &TempDir) -> Command {
    let mut cmd = Command::new(cargo_bin_path());
    cmd.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("bench.sqlite"));
    cmd.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    cmd.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    cmd.arg("--skip-memory-guard");
    cmd
}

fn init_db(tmp: &TempDir) {
    let status = sqlite_graphrag_cmd(tmp)
        .args(["init"])
        .status()
        .expect("sqlite-graphrag init falhou");
    assert!(status.success(), "init retornou {:?}", status.code());
}

fn populate_db(tmp: &TempDir, count: usize) {
    let bin = cargo_bin_path();
    let db_path = tmp.path().join("bench.sqlite");
    let cache_path = tmp.path().join("cache");
    for i in 0..count {
        let name = format!("bench-memoria-{i:04}");
        let body = format!("Conteúdo da memória de benchmark número {i}. Este texto simula dados reais com palavras suficientes para embedding e busca semântica.");
        let status = Command::new(&bin)
            .args([
                "--skip-memory-guard",
                "remember",
                "--name",
                &name,
                "--type",
                "project",
                "--description",
                "Memória de benchmark",
                "--body",
                &body,
            ])
            .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
            .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache_path)
            .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
            .status()
            .expect("remember falhou");
        assert!(
            status.success(),
            "remember {i} falhou com {:?}",
            status.code()
        );
    }
}

// ---------------------------------------------------------------------------
// Suite 9 — Benchmark 1: cold_start (--help, sem I/O de modelo)
// ---------------------------------------------------------------------------

fn bench_cold_start(c: &mut Criterion) {
    let bin = cargo_bin_path();

    c.bench_function("cold_start_help", |b| {
        b.iter(|| {
            let output = Command::new(&bin)
                .arg("--help")
                .output()
                .expect("sqlite-graphrag --help falhou");
            criterion::black_box(output.status.success());
        });
    });
}

// ---------------------------------------------------------------------------
// Suite 9 — Benchmark 2: warm_recall (DB com 10 memórias, 5 queries)
// ---------------------------------------------------------------------------

fn bench_warm_recall(c: &mut Criterion) {
    let tmp = TempDir::new().expect("TempDir falhou");
    init_db(&tmp);
    populate_db(&tmp, 10);

    let db_path = tmp.path().join("bench.sqlite");
    let cache_path = tmp.path().join("cache");
    let bin = cargo_bin_path();

    let queries = [
        "memória de benchmark",
        "conteúdo projeto",
        "dados embedding",
        "palavras reais",
        "simulação texto",
    ];

    c.bench_function("warm_recall_10_mems", |b| {
        b.iter(|| {
            for q in &queries {
                let output = Command::new(&bin)
                    .args(["--skip-memory-guard", "recall", q, "-k", "5"])
                    .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
                    .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache_path)
                    .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
                    .output()
                    .expect("recall falhou");
                // Aceita 0 (encontrou) ou 4 (not found) como válidos
                let code = output.status.code().unwrap_or(1);
                criterion::black_box(code == 0 || code == 4);
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Suite 9 — Benchmark 3: hybrid_search (DB com 50 memórias)
// ---------------------------------------------------------------------------

fn bench_hybrid_search(c: &mut Criterion) {
    let tmp = TempDir::new().expect("TempDir falhou");
    init_db(&tmp);
    populate_db(&tmp, 50);

    let db_path = tmp.path().join("bench.sqlite");
    let cache_path = tmp.path().join("cache");
    let bin = cargo_bin_path();

    c.bench_function("hybrid_search_50_mems", |b| {
        b.iter(|| {
            let output = Command::new(&bin)
                .args([
                    "--skip-memory-guard",
                    "hybrid-search",
                    "benchmark",
                    "-k",
                    "10",
                ])
                .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
                .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache_path)
                .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
                .output()
                .expect("hybrid-search falhou");
            let code = output.status.code().unwrap_or(1);
            criterion::black_box(code == 0 || code == 4);
        });
    });
}

// ---------------------------------------------------------------------------
// Suite 9 — Benchmark 4: chunking puro (sem CLI, cálculo em memória)
// ---------------------------------------------------------------------------

/// Aproximação da lógica de chunking sem depender da lib (evita erros de compilação
/// de outros módulos em WIP). Replica a lógica de `chunking.rs`.
fn split_into_chunks_local(body: &str) -> Vec<String> {
    const CHARS_PER_TOKEN: usize = 4;
    const CHUNK_SIZE_TOKENS: usize = 512;
    const CHUNK_OVERLAP_TOKENS: usize = 50;
    const CHUNK_SIZE_CHARS: usize = CHUNK_SIZE_TOKENS * CHARS_PER_TOKEN;
    const CHUNK_OVERLAP_CHARS: usize = CHUNK_OVERLAP_TOKENS * CHARS_PER_TOKEN;

    if body.len() <= CHUNK_SIZE_CHARS {
        return vec![body.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < body.len() {
        let end = (start + CHUNK_SIZE_CHARS).min(body.len());
        chunks.push(body[start..end].to_string());
        if end == body.len() {
            break;
        }
        start = end.saturating_sub(CHUNK_OVERLAP_CHARS);
    }
    chunks
}

fn bench_chunking_1k_tokens(c: &mut Criterion) {
    // 1000 tokens × 4 chars/token ≈ 4000 chars (abaixo do chunk size 512×4=2048)
    let body_1k: String = "palavra ".repeat(500);
    // 4000 tokens × 4 chars/token ≈ 16000 chars (força múltiplos chunks)
    let body_4k: String = "palavra ".repeat(2000);

    let mut group = c.benchmark_group("chunking");

    group.bench_with_input(
        BenchmarkId::new("split_1k_tokens", "1k"),
        &body_1k,
        |b, text| {
            b.iter(|| {
                let chunks = split_into_chunks_local(text);
                criterion::black_box(chunks);
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("split_4k_tokens", "4k"),
        &body_4k,
        |b, text| {
            b.iter(|| {
                let chunks = split_into_chunks_local(text);
                criterion::black_box(chunks);
            });
        },
    );

    group.finish();
}

// ---------------------------------------------------------------------------
// Suite 9 — Benchmark 5: RRF k=60 (cálculo puro de ranking)
// ---------------------------------------------------------------------------

fn rrf_score(rank: usize, k: f64) -> f64 {
    1.0 / (k + rank as f64 + 1.0)
}

fn run_rrf_ranking(n_candidates: usize, rrf_k: f64) -> Vec<(usize, f64)> {
    // Simula dois rankings com ordens opostas (pior caso de fusão)
    let vec_results: Vec<usize> = (0..n_candidates).collect();
    let fts_results: Vec<usize> = (0..n_candidates).rev().collect();

    let mut scores = vec![0.0f64; n_candidates];
    for (rank, &id) in vec_results.iter().enumerate() {
        scores[id] += rrf_score(rank, rrf_k);
    }
    for (rank, &id) in fts_results.iter().enumerate() {
        scores[id] += rrf_score(rank, rrf_k);
    }

    let mut ranked: Vec<(usize, f64)> = scores.into_iter().enumerate().collect();
    ranked.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    ranked
}

fn bench_rrf_k60(c: &mut Criterion) {
    let mut group = c.benchmark_group("rrf_k60");

    for n in [10usize, 100, 1000] {
        group.bench_with_input(BenchmarkId::new("candidatos", n), &n, |b, &n| {
            b.iter_batched(
                || n,
                |n| {
                    let result = run_rrf_ranking(n, 60.0);
                    criterion::black_box(result)
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Registro dos grupos criterion (measurement_time = 10s por bench)
// ---------------------------------------------------------------------------

criterion_group! {
    name = benchmarks_computacionais;
    config = Criterion::default()
        .measurement_time(std::time::Duration::from_secs(10));
    targets = bench_cold_start, bench_rrf_k60, bench_chunking_1k_tokens
}

criterion_group! {
    name = benchmarks_db;
    config = Criterion::default()
        .measurement_time(std::time::Duration::from_secs(10));
    targets = bench_warm_recall, bench_hybrid_search
}

criterion_main!(benchmarks_computacionais, benchmarks_db);
