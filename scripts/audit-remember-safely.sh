#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  printf 'usage: %s <corpus-dir>\n' "$0" >&2
  exit 2
fi

if ! command -v systemd-run >/dev/null 2>&1; then
  printf 'error: systemd-run not found\n' >&2
  exit 1
fi

BIN="${BIN:-sqlite-graphrag}"
CORPUS_DIR="$1"
MEMORY_MAX="${MEMORY_MAX:-4G}"
SWAP_MAX="${SWAP_MAX:-0}"
AUDIT_TIMEOUT_SECS="${AUDIT_TIMEOUT_SECS:-1800}"

if [[ -n "${WORK_DIR:-}" ]]; then
  mkdir -p "$WORK_DIR"
else
  WORK_DIR="$(mktemp -d /tmp/sqlite-graphrag-safe-audit.XXXXXX)"
fi

DB_PATH="$WORK_DIR/graphrag.sqlite"
CACHE_DIR="$WORK_DIR/cache"

cleanup() {
  /usr/bin/timeout -k 30 "$AUDIT_TIMEOUT_SECS" \
    env SQLITE_GRAPHRAG_CACHE_DIR="$CACHE_DIR" "$BIN" daemon --stop >/dev/null 2>&1 || true
}
trap cleanup EXIT

rm -f "$DB_PATH"

printf '==> inicializando banco com %s\n' "$BIN"
/usr/bin/timeout -k 30 "$AUDIT_TIMEOUT_SECS" \
  env SQLITE_GRAPHRAG_CACHE_DIR="$CACHE_DIR" "$BIN" init --db "$DB_PATH" --json >/dev/null
/usr/bin/timeout -k 30 "$AUDIT_TIMEOUT_SECS" \
  env SQLITE_GRAPHRAG_CACHE_DIR="$CACHE_DIR" "$BIN" health --db "$DB_PATH" --json >/dev/null

run_case() {
  local label="$1"
  local file_name="$2"
  local file_path="$CORPUS_DIR/$file_name"

  if [[ ! -f "$file_path" ]]; then
    printf 'SKIP  %-18s %s (arquivo ausente)\n' "$label" "$file_name"
    return 0
  fi

  printf 'RUN   %-18s %s\n' "$label" "$file_name"
  local output
  set +e
  output="$({ /usr/bin/timeout -k 30 "$AUDIT_TIMEOUT_SECS" systemd-run --user --scope -p "MemoryMax=$MEMORY_MAX" -p "MemorySwapMax=$SWAP_MAX" env SQLITE_GRAPHRAG_CACHE_DIR="$CACHE_DIR" "$BIN" remember --db "$DB_PATH" --name "audit-$label" --type reference --description audit --body-file "$file_path" --json; } 2>&1)"
  local status=$?
  set -e
  printf '%s\n' "$output"
  printf 'EXIT  %-18s %s\n' "$label" "$status"
}

run_case pass paperclip_executa_claude_como_subprocesso.md
run_case threshold paperclip_status_tarefa_ciclo_operacional_4_agents_01.md
run_case fail paperclip_agent_services_01.md

SYNTHETIC="$WORK_DIR/long-words-under-guard.txt"
: > "$SYNTHETIC"
for _ in $(seq 1 190); do
  printf 'superpalavramuitolonga ' >> "$SYNTHETIC"
done
printf '\nRUN   %-18s %s\n' synthetic "$(basename "$SYNTHETIC")"
set +e
SYNTHETIC_OUTPUT="$({ /usr/bin/timeout -k 30 "$AUDIT_TIMEOUT_SECS" systemd-run --user --scope -p "MemoryMax=$MEMORY_MAX" -p "MemorySwapMax=$SWAP_MAX" env SQLITE_GRAPHRAG_CACHE_DIR="$CACHE_DIR" "$BIN" remember --db "$DB_PATH" --name audit-synthetic --type reference --description audit --body-file "$SYNTHETIC" --json; } 2>&1)"
SYNTHETIC_STATUS=$?
set -e
printf '%s\n' "$SYNTHETIC_OUTPUT"
printf 'EXIT  %-18s %s\n' synthetic "$SYNTHETIC_STATUS"
