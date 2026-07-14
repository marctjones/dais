#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
STAMP="$(date -u +"%Y%m%d-%H%M%S")"
REPORT_DIR="${ROOT_DIR}/tmp/desk-release-${STAMP}"
REPORT_FILE="${REPORT_DIR}/report.md"
SCREENSHOT_DIR="${REPORT_DIR}/screenshots"
RUN_PRIVATE_MODE_LOCAL_SMOKE="${RUN_PRIVATE_MODE_LOCAL_SMOKE:-0}"
REQUIRE_FULL_RELEASE_GATES="${REQUIRE_FULL_RELEASE_GATES:-${REQUIRE_FULL:-0}}"
RELEASE_GATE_FAILURE=0

# Kept in sync with apps/dais-desk/tests/visual_smoke.rs's screenshot_for_screen()
# mapping, which is itself checked against dais_desk::expected_reachable_screens()
# every run (#373) — that Rust-side assertion is the real enforcement; this list
# is a secondary artifact-presence check for the release report.
REQUIRED_SCREENSHOTS=(
  home
  home-toolbar-tooltip
  home-row-meta-hover
  home-min-width
  home-wide
  home-compose-media
  home-compose-min-width
  home-inbox-notifications
  workflow-save-post
  home-today
  home-inbox-conversations
  home-post-thread
  home-saved
  workflow-reply-compose
  people-find-search
  people-friends
  people-followers
  people-following
  people-watches
  people-audience
  people-blocks
  workflow-follower-approve
  settings-accounts
  settings-privacy
  settings-security
  server-health
  server-deliveries
  server-moderation
  server-identity
  server-stats
)

mkdir -p "${REPORT_DIR}" "${SCREENSHOT_DIR}"

secret_status() {
  local name="$1"
  if [[ -n "${!name:-}" ]]; then
    printf 'set'
  else
    printf 'not set'
  fi
}

token_file_status() {
  local name="$1"
  local value="${!name:-}"
  if [[ -n "${value}" && -f "${value}" ]]; then
    printf 'set: file exists'
  elif [[ -n "${value}" ]]; then
    printf 'set: file missing'
  else
    printf 'not set'
  fi
}

{
  echo "# Desk v2 Release Gate"
  echo
  echo "**Date:** $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
  echo
  echo "## Private-Mode Fixture Inputs"
  echo
  echo "- \`DAIS_OWNER_TOKEN\`: $(secret_status DAIS_OWNER_TOKEN)"
  echo "- \`DAIS_OWNER_TOKEN_FILE\`: $(token_file_status DAIS_OWNER_TOKEN_FILE)"
  echo "- \`DAIS_OWNER_READ_TOKEN\`: $(secret_status DAIS_OWNER_READ_TOKEN)"
  echo "- \`DAIS_OWNER_READ_TOKEN_FILE\`: $(token_file_status DAIS_OWNER_READ_TOKEN_FILE)"
  echo "- \`DAIS_MASTODON_BEARER_TOKEN\`: $(secret_status DAIS_MASTODON_BEARER_TOKEN)"
  echo "- \`RUN_PRIVATE_MODE_LOCAL_SMOKE\`: ${RUN_PRIVATE_MODE_LOCAL_SMOKE}"
  echo "- \`REQUIRE_FULL_RELEASE_GATES\`: ${REQUIRE_FULL_RELEASE_GATES}"
  echo
  echo "Credential-gated live fixtures are reported as \`SKIP\` or \`INFO\` by the conformance harness when required secrets are absent."
  echo "Set \`REQUIRE_FULL_RELEASE_GATES=1\` to fail this release gate when credential-gated fixtures are skipped or informational rather than verified."
  echo "The local private-mode smoke requires a running local server; set \`RUN_PRIVATE_MODE_LOCAL_SMOKE=1\` and optionally \`BASE_URL\`, \`ACTOR\`, and \`ACTOR_URL\` to run it."
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

run_cmd "Rust Desk UI release gate" env SLINT_BACKEND=software DAIS_DESK_SCREENSHOT_DIR="${SCREENSHOT_DIR}" cargo test --manifest-path apps/dais-desk/Cargo.toml
run_cmd "Desk build verification" cargo build --manifest-path apps/dais-desk/Cargo.toml
run_cmd "Private-mode regression gate" cargo test --manifest-path core/Cargo.toml --test private_mode
if [[ "${RUN_PRIVATE_MODE_LOCAL_SMOKE}" == "1" ]]; then
  run_cmd "Local private-mode smoke" bash scripts/test-private-mode-local.sh
else
  {
    echo "## Local private-mode smoke"
    echo
    echo "SKIP: set \`RUN_PRIVATE_MODE_LOCAL_SMOKE=1\` with a running local server to execute \`scripts/test-private-mode-local.sh\`."
    echo "Required local inputs: \`BASE_URL\` defaults to \`http://localhost:8790\`; \`ACTOR\` defaults to \`social\`; \`ACTOR_URL\` defaults to \`https://social.dais.social/users/social\`."
    echo
  } >>"${REPORT_FILE}"
fi
if [[ -n "${DAIS_DESK_SETTINGS:-}" ]]; then
  # #371: verify Desk against a real production/skpt account through real
  # mode_nav()/screen_nav() click-through navigation, not select_screen
  # shortcuts — this is what would have caught #365/#366-shaped regressions
  # (whole screens silently unreachable while their own tests still passed).
  run_cmd "Live Desk GUI navigation gate (production/skpt)" env DAIS_DESK_LIVE_SMOKE=1 DAIS_DESK_SCREENSHOT_DIR="${SCREENSHOT_DIR}" cargo test --manifest-path apps/dais-desk/Cargo.toml --test live_smoke
else
  {
    echo "## Live Desk GUI navigation gate (production/skpt)"
    echo
    echo "SKIP: set \`DAIS_DESK_SETTINGS\` to a real production or skpt.cl owner-settings.json path to run this against a live account."
    echo "Exercises every mode_nav()/screen_nav() button by real click and fails if any screen falls back to placeholder data (see #359, #371)."
    echo
  } >>"${REPORT_FILE}"
fi
run_cmd "Live conformance smoke" cargo test --manifest-path conformance/Cargo.toml -- --nocapture
run_cmd "Bluesky conformance gate" env DAIS_CONFORMANCE_ONLY=bluesky cargo test --manifest-path conformance/Cargo.toml -- --nocapture
run_cmd "Design alignment progress evidence" test -f docs/guides/DESIGN_ALIGNMENT_MATRIX.md
run_cmd "Desk product completeness audit evidence" test -f docs/guides/DESK_PRODUCT_COMPLETENESS_AUDIT.md
run_cmd "Design coverage screenshots present" bash -c '
  screenshot_dir="$1"
  shift
  for shot in "$@"; do
    path="${screenshot_dir}/${shot}.png"
    if [ ! -f "${path}" ]; then
      echo "Missing required screenshot: ${shot}.png"
      exit 1
    fi
  done
' -- "${SCREENSHOT_DIR}" "${REQUIRED_SCREENSHOTS[@]}"

{
  echo "## Credential-Gated Fixture Summary"
  echo
  if grep -E '^(SKIP|INFO)([[:space:]:]|$)' "${REPORT_FILE}" >/dev/null; then
    grep -E '^(SKIP|INFO)([[:space:]:]|$)' "${REPORT_FILE}" \
      | sed 's/^/- /'
    if [[ "${REQUIRE_FULL_RELEASE_GATES}" == "1" ]]; then
      echo
      echo "FAIL: strict release mode requires these fixtures to be verified, not skipped or informational."
      RELEASE_GATE_FAILURE=1
    fi
  else
    echo "No skipped or informational credential-gated fixture rows were reported."
  fi
  echo
} >>"${REPORT_FILE}"

{
  echo "## Artifacts"
  echo
  echo "- Report: \`${REPORT_FILE}\`"
  echo "- Screenshot directory: \`${SCREENSHOT_DIR}\`"
  echo
  echo "- Desk screenshots:"
  for screenshot in "${REQUIRED_SCREENSHOTS[@]}"; do
    path="${SCREENSHOT_DIR}/${screenshot}.png"
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

if [[ "${RELEASE_GATE_FAILURE}" != "0" ]]; then
  exit "${RELEASE_GATE_FAILURE}"
fi
