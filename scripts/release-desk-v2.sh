#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
STAMP="$(date -u +"%Y%m%d-%H%M%S")"
REPORT_DIR="${ROOT_DIR}/tmp/desk-release-${STAMP}"
REPORT_FILE="${REPORT_DIR}/report.md"

mkdir -p "${REPORT_DIR}"

{
  echo "# Desk v2 Release Gate"
  echo
  echo "**Date:** $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
  echo
} >"${REPORT_FILE}"

run_cmd() {
  local label="$1"
  shift
  local status=0
  {
    echo "## ${label}"
    echo
    echo '```'
    printf '$ %s\n' "$*"
    echo
  } >>"${REPORT_FILE}"

  if (cd "${ROOT_DIR}" && "$@" 2>&1 | tee -a "${REPORT_FILE}"); then
    echo "✅ ${label}" | tee -a "${REPORT_FILE}"
    echo '```' >>"${REPORT_FILE}"
    echo "" >>"${REPORT_FILE}"
    return 0
  else
    status=$?
    echo "❌ ${label} (exit ${status})" | tee -a "${REPORT_FILE}"
    echo '```' >>"${REPORT_FILE}"
    echo >>"${REPORT_FILE}"
    return ${status}
  fi
}

run_cmd "Rust Desk UI release gate" cargo test --manifest-path apps/dais-desk/Cargo.toml
run_cmd "Desk build verification" cargo build --manifest-path apps/dais-desk/Cargo.toml
run_cmd "Private-mode regression gate" cargo test --manifest-path core/Cargo.toml --test private_mode
run_cmd "Live conformance smoke" cargo test --manifest-path conformance/Cargo.toml -- --nocapture
run_cmd "Bluesky conformance gate" env DAIS_CONFORMANCE_ONLY=bluesky cargo test --manifest-path conformance/Cargo.toml -- --nocapture
run_cmd "Design alignment progress evidence" test -f docs/guides/DESIGN_ALIGNMENT_MATRIX.md
run_cmd "Design coverage screenshots present" bash -c '
  for shot in home home-compose-media home-inbox-notifications home-reading home-today people-find-search people-relationship people-friends people-followers people-following people-watches-sources people-audience-groups people-blocks server-health server-deliveries server-stats server-identity server-moderation server-security server-accounts; do
    path="apps/dais-desk/target/dais-desk-screenshots/${shot}.png"
    if [ ! -f "${path}" ]; then
      echo "Missing required screenshot: ${shot}.png"
      exit 1
    fi
  done
'

{
  echo "## Artifacts"
  echo
  echo "- Report: \`${REPORT_FILE}\`"
  echo
  echo "- Desk screenshots:"
  for screenshot in home home-compose-media home-inbox-notifications home-reading home-today people-find-search people-relationship people-friends people-followers people-following people-watches-sources people-audience-groups people-blocks server-health server-deliveries server-stats server-identity server-moderation server-security server-accounts; do
    path="${ROOT_DIR}/apps/dais-desk/target/dais-desk-screenshots/${screenshot}.png"
    if [ -f "${path}" ]; then
      echo "  - ✅ ${screenshot}.png"
    else
      echo "  - ⚠️  ${screenshot}.png (missing)"
    fi
  done
  echo
  echo "Release evidence complete."
} >>"${REPORT_FILE}"

cat "${REPORT_FILE}"
