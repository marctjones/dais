#!/usr/bin/env bash
#
# Server release gate for the active Cloudflare-backed dais server.
#
# This script is intentionally boring: it runs the same commands maintainers
# would run by hand, records pass/fail/skip evidence, and exits non-zero unless
# every required gate passes. It does not deploy by default.
#
# Usage:
#   scripts/release-server.sh [OPTIONS]
#
# Options:
#   --plan                 Print the gate plan without running commands
#   --deploy               Deploy production and skpt after build gates pass
#   --skip-live            Skip live smoke tests unless strict mode is enabled
#   --conformance          Run all conformance suites
#   --bluesky-conformance  Run only Bluesky conformance
#   --mastodon-conformance Run only Mastodon API conformance
#   --strict               Fail closed on missing live prerequisites
#   --report-dir DIR       Write gate logs/report under DIR
#   -h, --help             Show this help
#
# Environment:
#   REQUIRE_FULL_RELEASE_GATES=1  Same as --strict
#   DAIS_CONFORMANCE_STRICT=1     Fail conformance runs when credential-gated
#                                  fixtures report INFO/SKIP
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"

PLAN_ONLY="false"
DEPLOY="false"
SKIP_LIVE="false"
STRICT="${REQUIRE_FULL_RELEASE_GATES:-0}"
RUN_CONFORMANCE="false"
RUN_BLUESKY_CONFORMANCE="false"
RUN_MASTODON_CONFORMANCE="false"
REPORT_DIR="$ROOT/tmp/server-release-$STAMP"

usage() { sed -n '2,32p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
  case "$1" in
    --plan) PLAN_ONLY="true"; shift ;;
    --deploy) DEPLOY="true"; shift ;;
    --skip-live) SKIP_LIVE="true"; shift ;;
    --conformance) RUN_CONFORMANCE="true"; shift ;;
    --bluesky-conformance) RUN_BLUESKY_CONFORMANCE="true"; shift ;;
    --mastodon-conformance) RUN_MASTODON_CONFORMANCE="true"; shift ;;
    --strict) STRICT="1"; shift ;;
    --report-dir) REPORT_DIR="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; echo >&2; usage >&2; exit 2 ;;
  esac
done

if [ "$STRICT" = "1" ] && [ "$SKIP_LIVE" = "true" ]; then
  echo "--skip-live is not allowed with --strict / REQUIRE_FULL_RELEASE_GATES=1" >&2
  exit 2
fi

REPORT="$REPORT_DIR/report.md"
LOG_DIR="$REPORT_DIR/logs"

declare -a NAMES=()
declare -a COMMANDS=()
declare -a ENVS=()

add_gate() {
  NAMES+=("$1")
  COMMANDS+=("$2")
  ENVS+=("${3:-}")
}

conformance_env() {
  local base="${1:-}"
  if [ "$STRICT" = "1" ]; then
    if [ -n "$base" ]; then
      printf "%s DAIS_CONFORMANCE_STRICT=1" "$base"
    else
      printf "DAIS_CONFORMANCE_STRICT=1"
    fi
  else
    printf "%s" "$base"
  fi
}

add_gate "core tests" "cargo test --manifest-path core/Cargo.toml"
add_gate "router tests" "cargo test --manifest-path platforms/cloudflare/workers/router/Cargo.toml"
add_gate "bindings tests" "cargo test --manifest-path platforms/cloudflare/bindings/Cargo.toml"
add_gate "production worker build" "scripts/deploy.sh build --env production"
add_gate "skpt worker build" "scripts/deploy.sh build --env skpt"

if [ "$RUN_CONFORMANCE" = "true" ]; then
  add_gate "all conformance tests" "cargo test --manifest-path conformance/Cargo.toml -- --nocapture" "$(conformance_env)"
fi
if [ "$RUN_BLUESKY_CONFORMANCE" = "true" ]; then
  add_gate "Bluesky conformance tests" "cargo test --manifest-path conformance/Cargo.toml -- --nocapture" "$(conformance_env "DAIS_CONFORMANCE_ONLY=bluesky")"
fi
if [ "$RUN_MASTODON_CONFORMANCE" = "true" ]; then
  add_gate "Mastodon API conformance tests" "cargo test --manifest-path conformance/Cargo.toml -- --nocapture" "$(conformance_env "DAIS_CONFORMANCE_ONLY=mastodon-api")"
fi

if [ "$DEPLOY" = "true" ]; then
  add_gate "production deploy" "scripts/deploy.sh deploy --env production --yes"
  add_gate "skpt deploy" "scripts/deploy.sh deploy --env skpt --yes"
fi

if [ "$SKIP_LIVE" != "true" ]; then
  if [ "$STRICT" = "1" ]; then
    add_gate "skpt live smoke" "scripts/smoke-skpt-instance.sh" "REQUIRE_FULL=1"
    add_gate "cross-instance E2EE live smoke" "scripts/smoke-cross-instance-e2ee.sh" "REQUIRE_FULL=1"
    add_gate "cross-instance MLS live smoke" "scripts/smoke-cross-instance-mls.sh" "REQUIRE_FULL=1"
  else
    add_gate "skpt live smoke" "scripts/smoke-skpt-instance.sh"
    add_gate "cross-instance E2EE live smoke" "scripts/smoke-cross-instance-e2ee.sh"
    add_gate "cross-instance MLS live smoke" "scripts/smoke-cross-instance-mls.sh"
  fi
fi

print_plan() {
  echo "Server release gate plan:"
  for i in "${!NAMES[@]}"; do
    if [ -n "${ENVS[$i]}" ]; then
      printf "  - %s: %s %s\n" "${NAMES[$i]}" "${ENVS[$i]}" "${COMMANDS[$i]}"
    else
      printf "  - %s: %s\n" "${NAMES[$i]}" "${COMMANDS[$i]}"
    fi
  done
  echo
  echo "report_dir=$REPORT_DIR"
}

if [ "$PLAN_ONLY" = "true" ]; then
  print_plan
  exit 0
fi

mkdir -p "$LOG_DIR"

{
  echo "# Server Release Gate"
  echo
  echo "- started_at_utc: $STAMP"
  echo "- strict: $STRICT"
  echo "- deploy: $DEPLOY"
  echo "- report_dir: $REPORT_DIR"
  echo
  echo "## Results"
  echo
} > "$REPORT"

status=0
for i in "${!NAMES[@]}"; do
  name="${NAMES[$i]}"
  command="${COMMANDS[$i]}"
  env_prefix="${ENVS[$i]}"
  log_file="$LOG_DIR/$(printf "%02d" "$((i + 1))")-$(printf "%s" "$name" | tr ' /' '--' | tr -cd '[:alnum:]_-').log"

  echo "==> $name"
  if [ -n "$env_prefix" ]; then
    echo "    $env_prefix $command"
  else
    echo "    $command"
  fi

  set +e
  if [ -n "$env_prefix" ]; then
    (cd "$ROOT" && env $env_prefix bash -lc "$command") >"$log_file" 2>&1
  else
    (cd "$ROOT" && bash -lc "$command") >"$log_file" 2>&1
  fi
  rc=$?
  set -e

  if [ "$rc" -eq 0 ]; then
    echo "- PASS: $name (\`$log_file\`)" >> "$REPORT"
  else
    echo "- FAIL: $name (\`$log_file\`, exit $rc)" >> "$REPORT"
    status=1
    break
  fi
done

{
  echo
  echo "## Final Status"
  echo
  if [ "$status" -eq 0 ]; then
    echo "PASS"
  else
    echo "FAIL"
  fi
} >> "$REPORT"

echo
echo "Report: $REPORT"
exit "$status"
