#!/usr/bin/env bash
# scripts/test-loom.sh — invocação canônica de testes loom serializados
# Motivação: prevenir livelock térmico observado em 2026-04-19 no i9-14900KF
# Uso manual somente — CI tem seu próprio job dedicado em .github/workflows/ci.yml

set -euo pipefail

LOOM_MAX_PREEMPTIONS="${LOOM_MAX_PREEMPTIONS:-2}"
LOOM_MAX_BRANCHES="${LOOM_MAX_BRANCHES:-500}"
RUST_TEST_THREADS="${RUST_TEST_THREADS:-1}"
LOOM_TIMEOUT_SECS="${LOOM_TIMEOUT_SECS:-3600}"

export LOOM_MAX_PREEMPTIONS LOOM_MAX_BRANCHES RUST_TEST_THREADS LOOM_TIMEOUT_SECS
export RUSTFLAGS="${RUSTFLAGS:-} --cfg loom"

echo "[test-loom] LOOM_MAX_PREEMPTIONS=$LOOM_MAX_PREEMPTIONS"
echo "[test-loom] LOOM_MAX_BRANCHES=$LOOM_MAX_BRANCHES"
echo "[test-loom] RUST_TEST_THREADS=$RUST_TEST_THREADS"
echo "[test-loom] LOOM_TIMEOUT_SECS=$LOOM_TIMEOUT_SECS"
echo "[test-loom] RUSTFLAGS=$RUSTFLAGS"
echo "[test-loom] Release mode obrigatório — debug pode rodar por horas"

START="$(date +%s)"

if command -v cargo-nextest >/dev/null 2>&1; then
  echo "[test-loom] usando cargo-nextest profile heavy"
  /usr/bin/timeout -k 30 "$LOOM_TIMEOUT_SECS" cargo nextest run --profile heavy -E 'test(/^loom_/)' --release
else
  echo "[test-loom] fallback para cargo test (nextest não instalado)"
  /usr/bin/timeout -k 30 "$LOOM_TIMEOUT_SECS" cargo test --test loom_lock_slots --release -- --test-threads=1
fi

END="$(date +%s)"
echo "[test-loom] concluído em $((END - START))s"
