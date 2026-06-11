# ADR-0023: Removal of `tokenizers` Crate (v1.0.76)

- Status: Accepted (2026-06-07)
- Update (v1.0.79): the `embedding-legacy` escape hatch mentioned below was removed ahead of the v1.1.0 schedule; the transition window is closed
- Deciders: Danilo Aguiar
- Scope: src/tokenizer.rs, src/chunking.rs, src/commands/ingest.rs, src/commands/remember.rs, src/commands/enrich.rs, src/commands/ingest_claude.rs, Cargo.toml

## Context

`tokenizers` 0.22 (Hugging Face) was used for three things in v1.0.74:

1. Counting tokens in a memory body to decide whether to chunk it.
2. Producing byte-offset pairs `(start, end)` for each token in the
   body, used by the chunker to align chunk boundaries with token
   boundaries.
3. Loading the multilingual-e5 tokenizer config from
   `tokenizer_config.json` to discover `model_max_length`.

In v1.0.76, the fastembed pipeline is gone, so the
`multilingual-e5-small` tokenizer is no longer used to embed anything.
The `tokenizers` crate still needed to be present for the chunker to
work correctly with the v1.0.74 `text-splitter` API (which takes a
`Tokenizer` for its `with_sizer`).

## Decision

The `tokenizers` crate is REMOVED from the default build. The chunker
and tokenizer are simplified:

- `token_count_approx` is now a char/word heuristic:
  `(words * 3) / 2` rounded up. This is conservative for the
  multilingual-e5 SentencePiece family and matches the calibration
  that the rest of the crate uses (`CHARS_PER_TOKEN = 2`).
- `passage_token_offsets` now returns whitespace-delimited word
  boundaries instead of true sub-word offsets. The LLM-side
  extraction doesn't need sub-word granularity; the prompt goes to
  the LLM, which handles tokenization on its side.
- `get_model_max_length` now returns the
  `crate::constants::EMBEDDING_MAX_TOKENS` constant (512). The
  operator can override via the `SQLITE_GRAPHRAG_EMBEDDING_MAX_TOKENS`
  env var.

The `text-splitter` crate is kept but the `with_sizer` call is
replaced with a char-count heuristic (the default `ChunkConfig::new`
sizer in `text-splitter` 0.30.1).

## Consequences

### Positive

- ~50 MB of compiled code removed from the binary (tokenizers +
  onig + the embedded BPE vocab files).
- The token count is now deterministic and reproducible without
  needing to load a 1 MB vocab file from disk.
- The chunker and tokenizer can be unit-tested without a network
  or filesystem dependency.

### Negative

- Token counts are approximate. For very long bodies, the
  approximation may undercount or overcount by 10-20%. This is
  acceptable for the chunking decision (a 512-token ceiling is
  checked before each LLM invocation; the LLM itself enforces the
  hard cap).
- The `text-splitter` crate's char-count sizer doesn't respect
  Markdown semantic boundaries as cleanly as the previous
  `tokenizer`-based sizer. Operators who need exact Markdown-aware
  chunking should enable the `embedding-legacy` feature and use
  the v1.0.74 path.

## Verification

- `cargo test --lib tokenizers` (the new whitespace-based
  tokenizer tests): 6 unit tests cover empty string, single word,
  multi-word, leading/trailing whitespace, and the `passage_offsets`
  boundary cases.
- `cargo test --lib chunking`: all tests green.
- `cargo test --lib`: 711 tests green.
