#!/usr/bin/env bash
#
# Restore one or more backup archives with an isolated HOME/DAIS_HOME and record
# disaster-recovery evidence without printing secrets.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
REPORT_DIR="$ROOT/tmp/disaster-recovery-$STAMP"
REPORT="$REPORT_DIR/report.md"
KEEP_TEMP="false"
SELF_TEST="false"
ARCHIVES=()

usage() {
  cat <<'USAGE'
Usage:
  scripts/disaster-recovery-drill.sh [options]

Options:
  --production-archive FILE   Production backup archive to restore
  --skpt-archive FILE         skpt backup archive to restore
  --archive LABEL=FILE        Additional labelled archive to restore
  --report-dir DIR            Write report under DIR instead of tmp/disaster-recovery-*
  --self-test                 Run verify-backup-restore built-in fixture first
  --keep-temp                 Keep the isolated temporary HOME/DAIS_HOME
  -h, --help                  Show this help

Encrypted archives use DAIS_BACKUP_PASSPHRASE or DAIS_BACKUP_PASSPHRASE_FILE.
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --production-archive) ARCHIVES+=("production=${2:-}"); shift 2 ;;
    --skpt-archive) ARCHIVES+=("skpt=${2:-}"); shift 2 ;;
    --archive) ARCHIVES+=("${2:-}"); shift 2 ;;
    --report-dir) REPORT_DIR="${2:-}"; REPORT="$REPORT_DIR/report.md"; shift 2 ;;
    --self-test) SELF_TEST="true"; shift ;;
    --keep-temp) KEEP_TEMP="true"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ "$SELF_TEST" != "true" ] && [ "${#ARCHIVES[@]}" -eq 0 ]; then
  usage >&2
  exit 2
fi

mkdir -p "$REPORT_DIR/logs"
ISOLATED_ROOT="$(mktemp -d)"
mkdir -p "$ISOLATED_ROOT/home" "$ISOLATED_ROOT/dais-home"
if [ "$KEEP_TEMP" = "true" ]; then
  trap 'echo "isolated_restore_root='"$ISOLATED_ROOT"'"' EXIT
else
  trap 'rm -rf "$ISOLATED_ROOT"' EXIT
fi

{
  echo "# Dais Disaster-Recovery Drill"
  echo
  echo "- generated_at_utc: $STAMP"
  echo "- report_dir: $REPORT_DIR"
  echo "- isolated_home: $ISOLATED_ROOT/home"
  echo "- isolated_dais_home: $ISOLATED_ROOT/dais-home"
  echo
  echo "This drill sets isolated HOME and DAIS_HOME values so restore validation"
  echo "does not depend on the operator's normal local Dais state."
  echo
  echo "## Results"
  echo
} > "$REPORT"

status=0

run_restore() {
  local label="$1"
  local archive="$2"
  local log="$REPORT_DIR/logs/${label}.log"
  if [ ! -f "$archive" ]; then
    printf -- '- FAIL: %s archive missing: `%s`\n' "$label" "$archive" >> "$REPORT"
    status=1
    return
  fi
  if (
    cd "$ROOT"
    HOME="$ISOLATED_ROOT/home" \
    DAIS_HOME="$ISOLATED_ROOT/dais-home" \
      scripts/verify-backup-restore.sh "$archive"
  ) >"$log" 2>&1; then
    printf -- '- PASS: %s restore (`%s`)\n' "$label" "$log" >> "$REPORT"
  else
    local rc=$?
    printf -- '- FAIL: %s restore (`%s`, exit %s)\n' "$label" "$log" "$rc" >> "$REPORT"
    status=1
  fi
}

if [ "$SELF_TEST" = "true" ]; then
  if (
    cd "$ROOT"
    HOME="$ISOLATED_ROOT/home-self-test" \
    DAIS_HOME="$ISOLATED_ROOT/dais-home-self-test" \
      scripts/verify-backup-restore.sh --self-test
  ) >"$REPORT_DIR/logs/self-test.log" 2>&1; then
    echo "- PASS: restore harness self-test (\`$REPORT_DIR/logs/self-test.log\`)" >> "$REPORT"
  else
    rc=$?
    echo "- FAIL: restore harness self-test (\`$REPORT_DIR/logs/self-test.log\`, exit $rc)" >> "$REPORT"
    status=1
  fi
fi

for entry in "${ARCHIVES[@]}"; do
  label="${entry%%=*}"
  archive="${entry#*=}"
  if [ "$label" = "$archive" ] || [ -z "$label" ] || [ -z "$archive" ]; then
    echo "Invalid --archive entry: $entry (expected LABEL=FILE)" >&2
    exit 2
  fi
  run_restore "$label" "$archive"
done

{
  echo
  echo "## Result"
  echo
  if [ "$status" -eq 0 ]; then
    echo "PASS"
  else
    echo "FAIL"
  fi
} >> "$REPORT"

cat "$REPORT"
exit "$status"
