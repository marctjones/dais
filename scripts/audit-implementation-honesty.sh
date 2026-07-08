#!/usr/bin/env bash
#
# Audit the repository for placeholder/dummy/shortcut implementation signals.
# This is a release evidence tool, not a backlog. Findings that require work
# should become GitHub issues under the active milestone.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
REPORT_DIR="$ROOT/tmp/implementation-honesty-$STAMP"
REPORT="$REPORT_DIR/report.md"
STRICT="true"

usage() {
  cat <<'USAGE'
Usage:
  scripts/audit-implementation-honesty.sh [options]

Options:
  --report-dir DIR   Write report under DIR instead of tmp/implementation-honesty-*
  --no-strict        Do not fail when unclassified attention hits are found
  -h, --help         Show this help
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --report-dir) REPORT_DIR="${2:-}"; REPORT="$REPORT_DIR/report.md"; shift 2 ;;
    --no-strict) STRICT="false"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

mkdir -p "$REPORT_DIR"

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 is required for implementation honesty audit" >&2
    exit 2
  fi
}

require_tool rg

run_rg() {
  (cd "$ROOT" && rg -n "$@" 2>/dev/null) || true
}

review_pattern='TODO|FIXME|todo!|unimplemented!|panic!\([^)]*(not implemented|TODO|FIXME)|not implemented yet|dummy|fake|stubbed|compatibility stub|placeholder'

review_hits="$(run_rg "$review_pattern" \
  core client client-core apps platforms scripts conformance \
  --glob '!**/target/**' \
  --glob '!apps/dais-desk/ui/app.slint' \
  --glob '!apps/dais-desk/tests/**' \
  --glob '!**/tests/**' \
  --glob '!third_party/**' \
  --glob '!scripts/seed-*' \
  --glob '!scripts/audit-implementation-honesty.sh' \
  --glob '!scripts/audit-e2ee-mls-security.sh')"

allow_pattern='(^core/src/(sql|traits|migrations)|^platforms/cloudflare/bindings/src/(d1|lib|queues)\.rs:|^conformance/src/lib\.rs:|^scripts/(backup|verify-backup-restore|release-desk-v2|release-server|managed-health-check|smoke-).*\.sh:|^platforms/cloudflare/workers/(actor|webfinger)/src/lib\.rs:|^platforms/cloudflare/workers/router/src/mastodon_api\.rs:|^apps/dais-desk/src/lib\.rs:[0-9]+:.*(fixture|Fixture|Offline preview|placeholder language|supported_primary_secondary_actions|row_has_placeholder_language|\["not implemented", "coming soon", "placeholder", "stub"\]|primary workflow screen .*placeholder row|contains\("placeholder"\)|product_completeness_primary_workflows_are_not_placeholders|row\.detail\.contains\("not implemented yet"\)|generic placeholder))'
unclassified_hits="$(printf '%s\n' "$review_hits" | grep -Ev "$allow_pattern" || true)"

compatibility_hits="$(run_rg 'owner-token-required|OAuth compatibility|compatibility shape|Compatibility shape' \
  docs/reference/MASTODON_API_PARITY.md conformance/src/lib.rs platforms/cloudflare/workers/router/src/mastodon_api.rs)"
unsupported_hits="$(run_rg 'UNKNOWN|unsupported|fake zero|unknown depth|skip-cloud placeholder' \
  scripts/managed-health-check.sh scripts/verify-backup-restore.sh platforms/cloudflare/bindings/src/queues.rs docs/guides/OPERATIONAL_RUNBOOK.md docs/guides/BACKUP_RESTORE.md)"
test_fixture_hits="$(run_rg 'fixture|mock|sample|fake_rng|Offline preview|placeholder language' \
  apps/dais-desk/src/lib.rs apps/dais-desk/tests third_party/hpke-rs-libcrux-0.6.1/src/lib.rs scripts/seed-local-db.sh scripts/seed-container-db.sh)"

{
  echo "# Implementation Honesty Audit"
  echo
  echo "- generated_at_utc: $STAMP"
  echo "- strict: $STRICT"
  echo "- report_dir: $REPORT_DIR"
  echo
  echo "## Commands"
  echo
  echo '```bash'
  echo "scripts/audit-implementation-honesty.sh${REPORT_DIR:+ --report-dir $REPORT_DIR}"
  echo "rg -n '$review_pattern' core client client-core apps platforms scripts conformance"
  echo '```'
  echo
  echo "## Unclassified Attention Hits"
  echo
  if [ -n "$unclassified_hits" ]; then
    echo '```text'
    printf '%s\n' "$unclassified_hits"
    echo '```'
  else
    echo "None."
  fi
  echo
  echo "## Classified Intentional Compatibility Surfaces"
  echo
  echo "These are documented compatibility or safety shapes, not hidden dummy behavior."
  echo
  echo '```text'
  printf '%s\n' "${compatibility_hits:-none}"
  echo '```'
  echo
  echo "## Classified Explicit Unsupported Operations"
  echo
  echo "These paths fail closed or report UNKNOWN rather than fabricating success."
  echo
  echo '```text'
  printf '%s\n' "${unsupported_hits:-none}"
  echo '```'
  echo
  echo "## Classified Test Fixtures"
  echo
  echo "These hits are constrained to tests, fixtures, seed data, or vendored test RNG code."
  echo
  echo '```text'
  printf '%s\n' "${test_fixture_hits:-none}"
  echo '```'
  echo
  echo "## Result"
  echo
  if [ -n "$unclassified_hits" ]; then
    echo "FAIL: unclassified implementation-honesty hits require fixes or GitHub issues."
  else
    echo "PASS: no unclassified placeholder/dummy/unimplemented implementation hits."
  fi
} > "$REPORT"

cat "$REPORT"

if [ "$STRICT" = "true" ] && [ -n "$unclassified_hits" ]; then
  exit 1
fi
