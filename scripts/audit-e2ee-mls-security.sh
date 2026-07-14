#!/usr/bin/env bash
#
# Post-roadmap E2EE/MLS security evidence gate.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
REPORT_DIR="$ROOT/tmp/e2ee-mls-security-$STAMP"
REPORT="$REPORT_DIR/report.md"
RUN_TESTS="true"

usage() {
  cat <<'USAGE'
Usage:
  scripts/audit-e2ee-mls-security.sh [options]

Options:
  --report-dir DIR   Write report under DIR instead of tmp/e2ee-mls-security-*
  --check-only       Run static checks only; skip cargo tests
  -h, --help         Show this help
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --report-dir) REPORT_DIR="${2:-}"; REPORT="$REPORT_DIR/report.md"; shift 2 ;;
    --check-only) RUN_TESTS="false"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

mkdir -p "$REPORT_DIR/logs"

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 is required for E2EE/MLS security audit" >&2
    exit 2
  fi
}

require_tool rg

run_logged() {
  local label="$1"
  shift
  local log="$REPORT_DIR/logs/$(printf '%s' "$label" | tr ' /' '--' | tr -cd '[:alnum:]_-').log"
  echo "==> $label"
  if (cd "$ROOT" && "$@") >"$log" 2>&1; then
    printf -- '- PASS: %s (`%s`)\n' "$label" "$log" >> "$REPORT"
  else
    local rc=$?
    printf -- '- FAIL: %s (`%s`, exit %s)\n' "$label" "$log" "$rc" >> "$REPORT"
    return "$rc"
  fi
}

{
  echo "# E2EE/MLS Security Audit"
  echo
  echo "- generated_at_utc: $STAMP"
  echo "- run_tests: $RUN_TESTS"
  echo "- report_dir: $REPORT_DIR"
  echo
  echo "## Results"
  echo
} > "$REPORT"

static_status=0

fake_hits="$(
  cd "$ROOT"
  rg -n 'fake|dummy|test-only|fixture plaintext|fixture message' \
    core/src client/src client-core/src platforms/cloudflare/workers/router/src/e2ee.rs apps/dais-desk/src \
    --glob '!**/target/**' \
    --glob '!third_party/**' 2>/dev/null \
    | grep -Ev '(^apps/dais-desk/src/lib\.rs:[0-9]+:.*(FIXTURE|fixture|Fixture|Offline preview|test_)|^core/src/e2ee_mls\.rs:[0-9]+:.*#\[cfg\(test\)|fixture_for_tests|test_|^core/src/atproto/mst\.rs:[0-9]+:.*(bafyfake|does-not-exist))' || true
)"
if [ -n "$fake_hits" ]; then
  printf -- '- FAIL: production E2EE/MLS paths contain unclassified fake/test fixture language\n' >> "$REPORT"
  {
    echo
    echo '```text'
    printf '%s\n' "$fake_hits"
    echo '```'
  } >> "$REPORT"
  static_status=1
else
  printf -- '- PASS: no unclassified fake/test-only language in production E2EE/MLS paths\n' >> "$REPORT"
fi

for lockfile in client/Cargo.lock apps/dais-desk/Cargo.lock; do
  if awk '
    $0 == "[[package]]" { pkg="" }
    $0 == "name = \"libcrux-chacha20poly1305\"" { pkg="libcrux-chacha20poly1305" }
    pkg == "libcrux-chacha20poly1305" && $0 == "version = \"0.0.8\"" { found=1 }
    END { exit(found ? 0 : 1) }
  ' "$ROOT/$lockfile"; then
    printf -- '- PASS: %s pins libcrux-chacha20poly1305 0.0.8\n' "$lockfile" >> "$REPORT"
  else
    printf -- '- FAIL: %s does not pin libcrux-chacha20poly1305 0.0.8\n' "$lockfile" >> "$REPORT"
    static_status=1
  fi
done

if [ ! -f "$ROOT/third_party/hpke-rs-libcrux-0.6.1/DAIS_PATCH.md" ]; then
  printf -- '- FAIL: vendored hpke-rs-libcrux patch note missing\n' >> "$REPORT"
  static_status=1
elif rg -q 'Exit plan|Remove this vendored patch when upstream' "$ROOT/third_party/hpke-rs-libcrux-0.6.1/DAIS_PATCH.md"; then
  printf -- '- PASS: vendored hpke-rs-libcrux patch note includes an exit plan\n' >> "$REPORT"
else
  printf -- '- FAIL: vendored hpke-rs-libcrux patch note lacks an exit plan\n' >> "$REPORT"
  static_status=1
fi

if [ "$RUN_TESTS" = "true" ]; then
  run_logged "core MLS tests" cargo test --manifest-path core/Cargo.toml --features mls e2ee_mls -- --nocapture || static_status=1
  run_logged "client-core E2EE tests" cargo test --manifest-path client-core/Cargo.toml e2ee || static_status=1
  run_logged "Desk E2EE projection tests" cargo test --manifest-path apps/dais-desk/Cargo.toml e2ee -- --nocapture || static_status=1
fi

{
  echo
  echo "## Result"
  echo
  if [ "$static_status" -eq 0 ]; then
    echo "PASS"
  else
    echo "FAIL"
  fi
} >> "$REPORT"

cat "$REPORT"
exit "$static_status"
