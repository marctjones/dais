#!/usr/bin/env bash
#
# Managed-instance operational health check.
#
# This script intentionally reports unsupported metrics as UNKNOWN instead of
# inventing zero values. In particular, Cloudflare Queues depth is not currently
# exposed through the Dais owner API or Wrangler in this workflow.
#
set -euo pipefail

DOMAIN=""
ACTIVITYPUB_DOMAIN=""
PDS_DOMAIN=""
OWNER_TOKEN_FILE=""
OWNER_TOKEN="${OWNER_TOKEN:-}"
BACKUP_DIR="${HOME}/.dais/backups"
BACKUP_MAX_AGE_HOURS="48"
R2_BUCKET=""
FAILED_DELIVERY_WARN="0"
QUEUED_DELIVERY_WARN="25"
WRANGLER="${WRANGLER:-wrangler}"

usage() {
  cat <<'USAGE'
Usage:
  scripts/managed-health-check.sh --domain DOMAIN --activitypub-domain DOMAIN --pds-domain DOMAIN [options]

Options:
  --owner-token-file FILE       File containing owner API bearer token
  --owner-token TOKEN           Owner API bearer token (not printed)
  --backup-dir DIR              Default: ~/.dais/backups
  --backup-max-age-hours HOURS  Default: 48
  --r2-bucket BUCKET            Optional R2 media bucket inventory check
  --failed-delivery-warn N      Default: 0
  --queued-delivery-warn N      Default: 25
  -h, --help                    Show this help
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --domain) DOMAIN="${2:-}"; shift 2 ;;
    --activitypub-domain) ACTIVITYPUB_DOMAIN="${2:-}"; shift 2 ;;
    --pds-domain) PDS_DOMAIN="${2:-}"; shift 2 ;;
    --owner-token-file) OWNER_TOKEN_FILE="${2:-}"; shift 2 ;;
    --owner-token) OWNER_TOKEN="${2:-}"; shift 2 ;;
    --backup-dir) BACKUP_DIR="${2:-}"; shift 2 ;;
    --backup-max-age-hours) BACKUP_MAX_AGE_HOURS="${2:-}"; shift 2 ;;
    --r2-bucket) R2_BUCKET="${2:-}"; shift 2 ;;
    --failed-delivery-warn) FAILED_DELIVERY_WARN="${2:-}"; shift 2 ;;
    --queued-delivery-warn) QUEUED_DELIVERY_WARN="${2:-}"; shift 2 ;;
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

status="ok"

ok() {
  printf 'OK      %s\n' "$1"
}

warn() {
  status="warn"
  printf 'WARN    %s\n' "$1"
}

unknown() {
  [ "$status" = "ok" ] && status="warn"
  printf 'UNKNOWN %s\n' "$1"
}

fail() {
  status="fail"
  printf 'FAIL    %s\n' "$1"
}

json_get() {
  jq -r "$1 // empty"
}

echo "=== Dais managed health: $ACTIVITYPUB_DOMAIN ==="

if scripts/smoke-managed-instance.sh --domain "$DOMAIN" --activitypub-domain "$ACTIVITYPUB_DOMAIN" --pds-domain "$PDS_DOMAIN" ${OWNER_TOKEN_FILE:+--owner-token-file "$OWNER_TOKEN_FILE"} >/tmp/dais-managed-smoke.out 2>/tmp/dais-managed-smoke.err; then
  ok "public endpoints and owner auth smoke"
else
  fail "public endpoints or owner auth smoke failed"
  cat /tmp/dais-managed-smoke.err >&2 || true
fi

if [ -n "$OWNER_TOKEN" ]; then
  stats="$(curl -fsS --max-time 20 -H "Authorization: Bearer $OWNER_TOKEN" "https://$ACTIVITYPUB_DOMAIN/api/dais/owner/stats")" || {
    fail "owner stats unavailable"
    stats="{}"
  }
  diagnostics="$(curl -fsS --max-time 20 -H "Authorization: Bearer $OWNER_TOKEN" "https://$ACTIVITYPUB_DOMAIN/api/dais/owner/diagnostics")" || {
    fail "owner diagnostics unavailable"
    diagnostics='{"items":[]}'
  }
  deliveries="$(curl -fsS --max-time 20 -H "Authorization: Bearer $OWNER_TOKEN" "https://$ACTIVITYPUB_DOMAIN/api/dais/owner/deliveries")" || {
    fail "owner deliveries unavailable"
    deliveries='{"items":[]}'
  }

  failed="$(printf '%s' "$stats" | json_get '.deliveries_failed')"
  queued="$(printf '%s' "$stats" | json_get '.deliveries_queued')"
  posts="$(printf '%s' "$stats" | json_get '.posts_total')"
  media_posts="$(printf '%s' "$stats" | json_get '.media_posts')"
  encrypted="$(printf '%s' "$stats" | json_get '.encrypted_posts')"
  printf 'INFO    posts=%s media_posts=%s encrypted_posts=%s deliveries_failed=%s deliveries_queued=%s\n' \
    "${posts:-unknown}" "${media_posts:-unknown}" "${encrypted:-unknown}" "${failed:-unknown}" "${queued:-unknown}"

  if [ -n "$failed" ] && [ "$failed" -gt "$FAILED_DELIVERY_WARN" ]; then
    warn "delivery failures exceed threshold: $failed > $FAILED_DELIVERY_WARN"
  else
    ok "delivery failure count within threshold"
  fi
  if [ -n "$queued" ] && [ "$queued" -gt "$QUEUED_DELIVERY_WARN" ]; then
    warn "queued deliveries exceed threshold: $queued > $QUEUED_DELIVERY_WARN"
  else
    ok "queued delivery count within threshold"
  fi

  bad_diagnostics="$(printf '%s' "$diagnostics" | jq -r '.items[]? | select(.ok == false) | "\(.key): \(.detail)"')"
  if [ -n "$bad_diagnostics" ]; then
    warn "diagnostics report failing checks"
    printf '%s\n' "$bad_diagnostics" | sed 's/^/        /'
  else
    ok "owner diagnostics"
  fi

  failed_rows="$(printf '%s' "$deliveries" | jq -r '.items[]? | select(.status == "failed" or .status == "retry") | "\(.id) \(.status) \(.target_url) \(.error_message // "")"' | head -5)"
  if [ -n "$failed_rows" ]; then
    warn "recent failed/retry deliveries present"
    printf '%s\n' "$failed_rows" | sed 's/^/        /'
  else
    ok "recent delivery rows"
  fi
else
  unknown "owner-token-backed stats, diagnostics, delivery failures, and auth checks"
fi

unknown "queue depth exact count: not exposed by current Dais owner API/Wrangler workflow"

if [ -n "$R2_BUCKET" ]; then
  if command -v "$WRANGLER" >/dev/null 2>&1 || [ -x "$WRANGLER" ]; then
    if inventory="$("$WRANGLER" r2 bucket info "$R2_BUCKET" 2>/tmp/dais-r2.err)"; then
      count="$(printf '%s\n' "$inventory" | awk -F: '/object_count:/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')"
      size="$(printf '%s\n' "$inventory" | awk -F: '/bucket_size:/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')"
      ok "R2 bucket info bucket=$R2_BUCKET object_count=${count:-unknown} bucket_size=${size:-unknown}"
    else
      warn "R2 bucket info failed for bucket=$R2_BUCKET"
      sed 's/^/        /' /tmp/dais-r2.err >&2 || true
    fi
  else
    unknown "R2 storage usage: wrangler unavailable"
  fi
else
  unknown "R2 storage usage: no --r2-bucket supplied"
fi

if [ -d "$BACKUP_DIR" ]; then
  newest="$(find "$BACKUP_DIR" -name 'dais_*_backup_*.tar.gz*' -type f -print0 2>/dev/null | xargs -0 ls -t 2>/dev/null | head -1 || true)"
  if [ -n "$newest" ]; then
    max_age_minutes=$((BACKUP_MAX_AGE_HOURS * 60))
    if find "$BACKUP_DIR" -name 'dais_*_backup_*.tar.gz*' -type f -mmin "-$max_age_minutes" | grep -q .; then
      ok "backup freshness: $(basename "$newest")"
    else
      warn "no backup newer than ${BACKUP_MAX_AGE_HOURS}h; newest is $(basename "$newest")"
    fi
  else
    warn "no backup archives found in $BACKUP_DIR"
  fi
else
  warn "backup directory not found: $BACKUP_DIR"
fi

echo "=== result: $status ==="
[ "$status" != "fail" ]
