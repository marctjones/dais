#!/usr/bin/env bash
#
# Validate one managed Dais instance after provisioning/deploy.
#
set -euo pipefail

DOMAIN=""
ACTIVITYPUB_DOMAIN=""
PDS_DOMAIN=""
OWNER_TOKEN_FILE=""
OWNER_TOKEN="${OWNER_TOKEN:-}"

usage() {
  cat <<'USAGE'
Usage:
  scripts/smoke-managed-instance.sh --domain DOMAIN --activitypub-domain DOMAIN --pds-domain DOMAIN [options]

Options:
  --owner-token-file FILE   File containing owner API bearer token
  --owner-token TOKEN       Owner API bearer token (not printed)
  -h, --help                Show this help
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --domain) DOMAIN="${2:-}"; shift 2 ;;
    --activitypub-domain) ACTIVITYPUB_DOMAIN="${2:-}"; shift 2 ;;
    --pds-domain) PDS_DOMAIN="${2:-}"; shift 2 ;;
    --owner-token-file) OWNER_TOKEN_FILE="${2:-}"; shift 2 ;;
    --owner-token) OWNER_TOKEN="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ -z "$DOMAIN" ] || [ -z "$ACTIVITYPUB_DOMAIN" ] || [ -z "$PDS_DOMAIN" ]; then
  usage >&2
  exit 2
fi
if [ -n "$OWNER_TOKEN_FILE" ]; then
  [ -f "$OWNER_TOKEN_FILE" ] || { echo "Owner token file not found: $OWNER_TOKEN_FILE" >&2; exit 2; }
  OWNER_TOKEN="$(tr -d '\n' < "$OWNER_TOKEN_FILE")"
fi

ok() {
  printf 'OK   %s\n' "$1"
}

fail() {
  printf 'FAIL %s\n' "$1" >&2
  exit 1
}

check_contains() {
  local label="$1"
  local url="$2"
  local needle="$3"
  local body
  body="$(curl -fsS --max-time 20 "$url")" || fail "$label: request failed"
  if ! printf '%s' "$body" | grep -Fq "$needle"; then
    fail "$label: missing '$needle'"
  fi
  ok "$label"
}

check_status() {
  local label="$1"
  local expected="$2"
  local url="$3"
  shift 3
  local status
  status="$(curl -sS -o /dev/null -w "%{http_code}" --max-time 20 "$@" "$url")" || fail "$label: request failed"
  if [ "$status" != "$expected" ]; then
    fail "$label: expected HTTP $expected, got $status"
  fi
  ok "$label"
}

check_contains \
  "ActivityPub actor" \
  "https://$ACTIVITYPUB_DOMAIN/users/social?format=json" \
  "\"preferredUsername\""

check_contains \
  "ActivityPub E2EE metadata" \
  "https://$ACTIVITYPUB_DOMAIN/users/social?format=json" \
  "daisE2ee"

check_contains \
  "apex WebFinger" \
  "https://$DOMAIN/.well-known/webfinger?resource=acct:social@$DOMAIN" \
  "https://$ACTIVITYPUB_DOMAIN/users/social"

check_contains \
  "PDS describeServer" \
  "https://$PDS_DOMAIN/xrpc/com.atproto.server.describeServer" \
  "did:web:$ACTIVITYPUB_DOMAIN"

check_status \
  "owner API requires bearer" \
  "401" \
  "https://$ACTIVITYPUB_DOMAIN/api/dais/owner/profile"

if [ -n "$OWNER_TOKEN" ]; then
  check_status \
    "owner token auth" \
    "200" \
    "https://$ACTIVITYPUB_DOMAIN/api/dais/owner/profile" \
    -H "Authorization: Bearer $OWNER_TOKEN"
else
  echo "SKIP owner token auth: no owner token supplied"
fi
