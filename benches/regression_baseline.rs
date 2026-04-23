use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers de ambiente isolado
// ---------------------------------------------------------------------------

fn cargo_bin_path() -> PathBuf {
    let mut p = std::env::current_exe()
        .expect("current_exe falhou")
        .parent()
        .expect("parent do exe falhou")
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

fn sqlite_graphrag_cmd(tmp: &TempDir) -> Command {
    let mut cmd = Command::new(cargo_bin_path());
    cmd.env(
        "SQLITE_GRAPHRAG_DB_PATH",
        tmp.path().join("baseline.sqlite"),
    );
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
    let db_path = tmp.path().join("baseline.sqlite");
    let cache_path = tmp.path().join("cache");
    for i in 0..count {
        let name = format!("baseline-memoria-{i:04}");
        let body = format!(
            "Conteúdo de memória de regressão número {i}. \
             Este texto contém palavras suficientes para geração de embedding \
             e cobertura de busca semântica e full-text simultânea."
        );
        let status = Command::new(&bin)
            .args([
                "--skip-memory-guard",
                "remember",
                "--name",
                &name,
                "--type",
                "project",
                "--description",
                "Memória de baseline de regressão",
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
// Funções puras para benchmarks computacionais (sem CLI)
// ---------------------------------------------------------------------------

fn split_into_chunks(body: &str) -> Vec<String> {
    const CHARS_PER_TOKEN: usize = 4;
    const CHUNK_SIZE_CHARS: usize = 512 * CHARS_PER_TOKEN;
    const CHUNK_OVERLAP_CHARS: usize = 50 * CHARS_PER_TOKEN;

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

fn rrf_score(rank: usize, k: f64) -> f64 {
    1.0 / (k + rank as f64 + 1.0)
}

fn run_rrf_fusion(n_candidates: usize, rrf_k: f64) -> Vec<(usize, f64)> {
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
    ranked.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).expect("NaN em scores RRF"));
    ranked
}

// ---------------------------------------------------------------------------
// Baseline 1 — cold_start: invocação do binário + health check via --help
// ---------------------------------------------------------------------------

fn bench_cold_start(c: &mut Criterion) {
    let bin = cargo_bin_path();
    c.benchmark_group("baseline_cold_start")
        .sample_size(50)
        .measurement_time(Duration::from_secs(10))
        .bench_function("cold_start_help", |b| {
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
// Baseline 2 — warm_recall: segundo recall após o primeiro (cache quente)
// ---------------------------------------------------------------------------

fn bench_warm_recall(c: &mut Criterion) {
    let tmp = TempDir::new().expect("TempDir falhou");
    init_db(&tmp);
    populate_db(&tmp, 10);

    let db_path = tmp.path().join("baseline.sqlite");
    let cache_path = tmp.path().join("cache");
    let bin = cargo_bin_path();

    // Execução de aquecimento — garante que o cache de embedding está populado
    let _ = Command::new(&bin)
        .args([
            "--skip-memory-guard",
            "recall",
            "regressão baseline",
            "-k",
            "5",
        ])
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache_path)
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .output();

    c.benchmark_group("baseline_warm_recall")
        .sample_size(50)
        .measurement_time(Duration::from_secs(10))
        .bench_function("warm_recall_10_mems", |b| {
            b.iter(|| {
                let output = Command::new(&bin)
                    .args([
                        "--skip-memory-guard",
                        "recall",
                        "regressão baseline",
                        "-k",
                        "5",
                    ])
                    .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
                    .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache_path)
                    .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
                    .output()
                    .expect("recall falhou");
                let code = output.status.code().unwrap_or(1);
                criterion::black_box(code == 0 || code == 4);
            });
        });
}

// ---------------------------------------------------------------------------
// Baseline 3 — hybrid_search: 100 memórias populadas
// ---------------------------------------------------------------------------

fn bench_hybrid_search(c: &mut Criterion) {
    let tmp = TempDir::new().expect("TempDir falhou");
    init_db(&tmp);
    populate_db(&tmp, 100);

    let db_path = tmp.path().join("baseline.sqlite");
    let cache_path = tmp.path().join("cache");
    let bin = cargo_bin_path();

    c.benchmark_group("baseline_hybrid_search")
        .sample_size(50)
        .measurement_time(Duration::from_secs(10))
        .bench_function("hybrid_search_100_mems", |b| {
            b.iter(|| {
                let output = Command::new(&bin)
                    .args([
                        "--skip-memory-guard",
                        "hybrid-search",
                        "memória regressão",
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
// Baseline 4 — chunking_1k: embedder sobre texto equivalente a 1000 tokens
// ---------------------------------------------------------------------------

fn bench_chunking_1k(c: &mut Criterion) {
    // 1000 tokens × 4 chars/token ≈ 4000 chars (cabe em chunk único)
    let body_1k: String = "palavra ".repeat(500);
    // 4000 tokens × 4 chars/token ≈ 16000 chars (força múltiplos chunks)
    let body_4k: String = "palavra ".repeat(2000);

    let mut group = c.benchmark_group("baseline_chunking");
    group
        .sample_size(50)
        .measurement_time(Duration::from_secs(10));

    group.bench_with_input(
        BenchmarkId::new("split_1k_tokens", "1k"),
        &body_1k,
        |b, text| {
            b.iter(|| {
                let chunks = split_into_chunks(text);
                criterion::black_box(chunks);
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("split_4k_tokens", "4k"),
        &body_4k,
        |b, text| {
            b.iter(|| {
                let chunks = split_into_chunks(text);
                criterion::black_box(chunks);
            });
        },
    );

    group.finish();
}

// ---------------------------------------------------------------------------
// Baseline 5 — rrf_k60: fusão determinística com k=60
// ---------------------------------------------------------------------------

fn bench_rrf_k60(c: &mut Criterion) {
    let mut group = c.benchmark_group("baseline_rrf_k60");
    group
        .sample_size(50)
        .measurement_time(Duration::from_secs(10));

    for n in [10usize, 100, 1000] {
        group.bench_with_input(BenchmarkId::new("candidatos", n), &n, |b, &n| {
            b.iter_batched(
                || n,
                |n| {
                    let result = run_rrf_fusion(n, 60.0);
                    criterion::black_box(result)
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Registro dos grupos criterion
// ---------------------------------------------------------------------------

criterion_group! {
    name = baseline_computacional;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(10));
    targets = bench_cold_start, bench_chunking_1k, bench_rrf_k60
}

criterion_group! {
    name = baseline_db;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(10));
    targets = bench_warm_recall, bench_hybrid_search
}

criterion_main!(baseline_computacional, baseline_db);
