#!/usr/bin/env bash
#
# Build a repeatable Cloudflare provisioning plan for one managed Dais instance.
#
# The default mode is dry-run: write a manifest and print the commands. Use
# --apply-resources to create Cloudflare D1/R2/Queue resources and set secrets.
# Worker deploys and custom-domain routes still require reviewing the generated
# wrangler environment snippets before deploy.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT/tmp/managed-provision"
SLUG=""
DOMAIN=""
ACTIVITYPUB_DOMAIN=""
PDS_DOMAIN=""
OWNER_TOKEN_FILE=""
DELIVERY_ADMIN_TOKEN_FILE=""
APPLY_RESOURCES="false"
WRANGLER="${WRANGLER:-wrangler}"

usage() {
  cat <<'USAGE'
Usage:
  scripts/provision-managed-instance.sh --slug SLUG --domain DOMAIN [options]

Required:
  --slug SLUG                         Lowercase instance slug, e.g. acme-family
  --domain DOMAIN                     Apex owner domain, e.g. example.com

Options:
  --activitypub-domain DOMAIN         Default: social.DOMAIN
  --pds-domain DOMAIN                 Default: pds.DOMAIN
  --owner-token-file FILE             File used for OWNER_API_TOKEN/DAIS_OWNER_TOKEN secret
  --delivery-admin-token-file FILE    File used for DELIVERY_ADMIN_TOKEN secret
  --out-dir DIR                       Default: tmp/managed-provision
  --apply-resources                   Create D1/R2/Queues and set provided secrets
  -h, --help                          Show this help

The script writes:
  manifest.json                       Resource names, domains, and validation URLs
  router-env.toml                     Router wrangler [env.<slug>] snippet
  pds-env.toml                        PDS wrangler [env.<slug>] snippet
  landing-env.toml                    Landing/WebFinger wrangler [env.<slug>] snippet
  delivery-queue-env.toml             Delivery worker [env.<slug>] snippet
  commands.txt                        Ordered provisioning/deploy/validation commands
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --slug) SLUG="${2:-}"; shift 2 ;;
    --domain) DOMAIN="${2:-}"; shift 2 ;;
    --activitypub-domain) ACTIVITYPUB_DOMAIN="${2:-}"; shift 2 ;;
    --pds-domain) PDS_DOMAIN="${2:-}"; shift 2 ;;
    --owner-token-file) OWNER_TOKEN_FILE="${2:-}"; shift 2 ;;
    --delivery-admin-token-file) DELIVERY_ADMIN_TOKEN_FILE="${2:-}"; shift 2 ;;
    --out-dir) OUT_DIR="${2:-}"; shift 2 ;;
    --apply-resources) APPLY_RESOURCES="true"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ -z "$SLUG" ] || [ -z "$DOMAIN" ]; then
  usage >&2
  exit 2
fi
if ! printf '%s' "$SLUG" | grep -Eq '^[a-z0-9][a-z0-9-]{1,61}[a-z0-9]$'; then
  echo "Invalid --slug '$SLUG': use lowercase letters, numbers, and hyphens" >&2
  exit 2
fi
if printf '%s' "$DOMAIN" | grep -Eq 'https?://|/'; then
  echo "Invalid --domain '$DOMAIN': provide a host name, not a URL" >&2
  exit 2
fi

ACTIVITYPUB_DOMAIN="${ACTIVITYPUB_DOMAIN:-social.$DOMAIN}"
PDS_DOMAIN="${PDS_DOMAIN:-pds.$DOMAIN}"
INSTANCE_OUT="$OUT_DIR/$SLUG"
mkdir -p "$INSTANCE_OUT"

DB_NAME="dais-$SLUG"
R2_BUCKET="dais-media-$SLUG"
QUEUE_NAME="delivery-queue-$SLUG"
DLQ_NAME="delivery-dlq-$SLUG"
ROUTER_WORKER="router-$SLUG"
PDS_WORKER="pds-$SLUG"
LANDING_WORKER="landing-$SLUG"
DELIVERY_WORKER="delivery-queue-$SLUG"

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

write_manifest() {
  cat > "$INSTANCE_OUT/manifest.json" <<EOF
{
  "format": "dais-managed-instance-v1",
  "slug": "$(json_escape "$SLUG")",
  "domain": "$(json_escape "$DOMAIN")",
  "activitypub_domain": "$(json_escape "$ACTIVITYPUB_DOMAIN")",
  "pds_domain": "$(json_escape "$PDS_DOMAIN")",
  "resources": {
    "d1_database": "$(json_escape "$DB_NAME")",
    "r2_bucket": "$(json_escape "$R2_BUCKET")",
    "queue": "$(json_escape "$QUEUE_NAME")",
    "dead_letter_queue": "$(json_escape "$DLQ_NAME")",
    "router_worker": "$(json_escape "$ROUTER_WORKER")",
    "pds_worker": "$(json_escape "$PDS_WORKER")",
    "landing_worker": "$(json_escape "$LANDING_WORKER")",
    "delivery_worker": "$(json_escape "$DELIVERY_WORKER")"
  },
  "validation": {
    "webfinger": "https://$(json_escape "$DOMAIN")/.well-known/webfinger?resource=acct:social@$(json_escape "$DOMAIN")",
    "activitypub_actor": "https://$(json_escape "$ACTIVITYPUB_DOMAIN")/users/social",
    "owner_profile": "https://$(json_escape "$ACTIVITYPUB_DOMAIN")/api/dais/owner/profile",
    "pds_describe_server": "https://$(json_escape "$PDS_DOMAIN")/xrpc/com.atproto.server.describeServer"
  },
  "secrets": {
    "owner_token_file_provided": $([ -n "$OWNER_TOKEN_FILE" ] && printf true || printf false),
    "delivery_admin_token_file_provided": $([ -n "$DELIVERY_ADMIN_TOKEN_FILE" ] && printf true || printf false)
  }
}
EOF
}

write_snippets() {
  cat > "$INSTANCE_OUT/router-env.toml" <<EOF
[env.$SLUG]
name = "$ROUTER_WORKER"
routes = [
  { pattern = "$ACTIVITYPUB_DOMAIN", custom_domain = true }
]

[[env.$SLUG.r2_buckets]]
binding = "MEDIA_BUCKET"
bucket_name = "$R2_BUCKET"

[[env.$SLUG.d1_databases]]
binding = "DB"
database_name = "$DB_NAME"
database_id = "REPLACE_WITH_D1_DATABASE_ID"

[env.$SLUG.ai]
binding = "AI"

[env.$SLUG.triggers]
crons = ["*/30 * * * *"]

[env.$SLUG.vars]
ENVIRONMENT = "$SLUG"
DOMAIN = "$DOMAIN"
ACTIVITYPUB_DOMAIN = "$ACTIVITYPUB_DOMAIN"
USERNAME = "social"
WEBFINGER_URL = "https://webfinger-$SLUG.marc-t-jones.workers.dev"
ACTOR_URL = "https://actor-$SLUG.marc-t-jones.workers.dev"
INBOX_URL = "https://inbox-$SLUG.marc-t-jones.workers.dev"
OUTBOX_URL = "https://outbox-$SLUG.marc-t-jones.workers.dev"
PDS_URL = "https://$PDS_DOMAIN"
DELIVERY_QUEUE_URL = "https://$DELIVERY_WORKER.marc-t-jones.workers.dev"
EOF

  cat > "$INSTANCE_OUT/pds-env.toml" <<EOF
[env.$SLUG]
name = "$PDS_WORKER"
routes = [
  { pattern = "$PDS_DOMAIN", custom_domain = true }
]

[env.$SLUG.vars]
DOMAIN = "$ACTIVITYPUB_DOMAIN"
PDS_HOSTNAME = "$PDS_DOMAIN"

[[env.$SLUG.d1_databases]]
binding = "DB"
database_name = "$DB_NAME"
database_id = "REPLACE_WITH_D1_DATABASE_ID"

[[env.$SLUG.r2_buckets]]
binding = "MEDIA_BUCKET"
bucket_name = "$R2_BUCKET"
EOF

  cat > "$INSTANCE_OUT/landing-env.toml" <<EOF
[env.$SLUG]
name = "$LANDING_WORKER"
routes = [
  { pattern = "$DOMAIN/.well-known/webfinger*", zone_name = "$DOMAIN" }
]

[env.$SLUG.vars]
DOMAIN = "$DOMAIN"
ACTIVITYPUB_DOMAIN = "$ACTIVITYPUB_DOMAIN"
WEBFINGER_URL = "https://webfinger-$SLUG.marc-t-jones.workers.dev"
EOF

  cat > "$INSTANCE_OUT/delivery-queue-env.toml" <<EOF
[env.$SLUG]
name = "$DELIVERY_WORKER"

[[env.$SLUG.queues.producers]]
binding = "DELIVERY_QUEUE"
queue = "$QUEUE_NAME"

[[env.$SLUG.queues.consumers]]
queue = "$QUEUE_NAME"
max_batch_size = 10
max_batch_timeout = 30
dead_letter_queue = "$DLQ_NAME"

[[env.$SLUG.d1_databases]]
binding = "DB"
database_name = "$DB_NAME"
database_id = "REPLACE_WITH_D1_DATABASE_ID"

[[env.$SLUG.r2_buckets]]
binding = "MEDIA_BUCKET"
bucket_name = "$R2_BUCKET"

[env.$SLUG.vars]
DOMAIN = "$DOMAIN"
ACTIVITYPUB_DOMAIN = "$ACTIVITYPUB_DOMAIN"
EOF
}

write_commands() {
  cat > "$INSTANCE_OUT/commands.txt" <<EOF
# Review generated *-env.toml snippets and merge them into the matching worker wrangler.toml files.
# Then replace REPLACE_WITH_D1_DATABASE_ID with the id from the D1 create output.

$WRANGLER d1 create $DB_NAME
$WRANGLER r2 bucket create $R2_BUCKET
$WRANGLER queues create $QUEUE_NAME
$WRANGLER queues create $DLQ_NAME
EOF
  if [ -n "$OWNER_TOKEN_FILE" ]; then
    cat >> "$INSTANCE_OUT/commands.txt" <<EOF
$WRANGLER secret put OWNER_API_TOKEN --env $SLUG --config platforms/cloudflare/workers/router/wrangler.toml < $OWNER_TOKEN_FILE
$WRANGLER secret put DAIS_OWNER_TOKEN --env $SLUG --config platforms/cloudflare/workers/router/wrangler.toml < $OWNER_TOKEN_FILE
EOF
  else
    cat >> "$INSTANCE_OUT/commands.txt" <<'EOF'
# Generate an owner token, store it with 0600 permissions, then set OWNER_API_TOKEN and DAIS_OWNER_TOKEN on the router worker.
EOF
  fi
  if [ -n "$DELIVERY_ADMIN_TOKEN_FILE" ]; then
    cat >> "$INSTANCE_OUT/commands.txt" <<EOF
$WRANGLER secret put DELIVERY_ADMIN_TOKEN --env $SLUG --config platforms/cloudflare/workers/delivery-queue/wrangler.toml < $DELIVERY_ADMIN_TOKEN_FILE
EOF
  fi
  cat >> "$INSTANCE_OUT/commands.txt" <<EOF
scripts/deploy.sh deploy --env $SLUG --only landing --yes
scripts/deploy.sh deploy --env $SLUG --only router --yes
scripts/deploy.sh deploy --env $SLUG --only pds --yes
scripts/deploy.sh deploy --env $SLUG --only delivery-queue --yes
scripts/smoke-managed-instance.sh --domain $DOMAIN --activitypub-domain $ACTIVITYPUB_DOMAIN --pds-domain $PDS_DOMAIN --owner-token-file ${OWNER_TOKEN_FILE:-OWNER_TOKEN_FILE}
EOF
}

run_if_needed() {
  if [ "$APPLY_RESOURCES" != "true" ]; then
    return 0
  fi
  if ! command -v "$WRANGLER" >/dev/null 2>&1 && [ ! -x "$WRANGLER" ]; then
    echo "wrangler not found. Install wrangler or set WRANGLER=/path/to/wrangler." >&2
    exit 1
  fi
  "$WRANGLER" d1 create "$DB_NAME"
  "$WRANGLER" r2 bucket create "$R2_BUCKET"
  "$WRANGLER" queues create "$QUEUE_NAME"
  "$WRANGLER" queues create "$DLQ_NAME"
  if [ -n "$OWNER_TOKEN_FILE" ]; then
    [ -f "$OWNER_TOKEN_FILE" ] || { echo "Owner token file not found: $OWNER_TOKEN_FILE" >&2; exit 2; }
    "$WRANGLER" secret put OWNER_API_TOKEN --env "$SLUG" --config "$ROOT/platforms/cloudflare/workers/router/wrangler.toml" < "$OWNER_TOKEN_FILE"
    "$WRANGLER" secret put DAIS_OWNER_TOKEN --env "$SLUG" --config "$ROOT/platforms/cloudflare/workers/router/wrangler.toml" < "$OWNER_TOKEN_FILE"
  fi
  if [ -n "$DELIVERY_ADMIN_TOKEN_FILE" ]; then
    [ -f "$DELIVERY_ADMIN_TOKEN_FILE" ] || { echo "Delivery admin token file not found: $DELIVERY_ADMIN_TOKEN_FILE" >&2; exit 2; }
    "$WRANGLER" secret put DELIVERY_ADMIN_TOKEN --env "$SLUG" --config "$ROOT/platforms/cloudflare/workers/delivery-queue/wrangler.toml" < "$DELIVERY_ADMIN_TOKEN_FILE"
  fi
}

write_manifest
write_snippets
write_commands
run_if_needed

echo "Managed instance provisioning plan: $INSTANCE_OUT"
echo
cat "$INSTANCE_OUT/commands.txt"
