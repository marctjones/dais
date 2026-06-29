#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKERS_DIR="$ROOT_DIR/platforms/cloudflare/workers"
INSTANCE_FILE="$ROOT_DIR/instances/skpt-cl.toml"
TMP_DIR="${TMPDIR:-/tmp}/dais-skpt-audit.$$"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT
mkdir -p "$TMP_DIR"

ok() {
  printf 'OK   %s\n' "$1"
}

fail() {
  printf 'FAIL %s\n' "$1" >&2
  exit 1
}

extract_skpt_env() {
  local file="$1"
  awk '
    /^\[env\.skpt\]$/ { in_skpt = 1; print; next }
    /^\[/ && $0 !~ /^\[+env\.skpt(\.|\])/ {
      if (in_skpt) {
        exit
      }
    }
    in_skpt { print }
  ' "$file"
}

require_file_contains() {
  local label="$1"
  local file="$2"
  local needle="$3"
  grep -Fq "$needle" "$file" || fail "$label missing $needle"
}

require_file_not_contains() {
  local label="$1"
  local file="$2"
  local needle="$3"
  if grep -Fq "$needle" "$file"; then
    fail "$label unexpectedly contains $needle"
  fi
}

require_skpt_block_contains() {
  local worker="$1"
  local needle="$2"
  local block="$TMP_DIR/$worker.skpt.toml"
  require_file_contains "$worker skpt env" "$block" "$needle"
}

require_skpt_block_not_contains() {
  local worker="$1"
  local needle="$2"
  local block="$TMP_DIR/$worker.skpt.toml"
  require_file_not_contains "$worker skpt env" "$block" "$needle"
}

[ -f "$INSTANCE_FILE" ] || fail "missing $INSTANCE_FILE"
require_file_contains "instance manifest" "$INSTANCE_FILE" 'environment = "skpt"'
require_file_contains "instance manifest" "$INSTANCE_FILE" 'activitypub_domain = "social.skpt.cl"'
require_file_contains "instance manifest" "$INSTANCE_FILE" 'pds_hostname = "pds.skpt.cl"'
require_file_contains "instance manifest" "$INSTANCE_FILE" 'd1_database_name = "dais-skpt"'
require_file_contains "instance manifest" "$INSTANCE_FILE" 'r2_bucket = "dais-media-skpt"'
require_file_contains "instance manifest" "$INSTANCE_FILE" 'delivery_queue = "delivery-queue-skpt"'
require_file_contains "instance manifest" "$INSTANCE_FILE" 'delivery_dead_letter_queue = "delivery-dlq-skpt"'
ok "instance manifest declares skpt-only resources"

workers=(actor auth delivery-queue inbox landing outbox pds router webfinger)
for worker in "${workers[@]}"; do
  file="$WORKERS_DIR/$worker/wrangler.toml"
  block="$TMP_DIR/$worker.skpt.toml"
  [ -f "$file" ] || fail "missing worker config $file"
  extract_skpt_env "$file" > "$block"
  [ -s "$block" ] || fail "$worker has no [env.skpt] block"
  require_skpt_block_contains "$worker" "name = \"$worker-skpt\""
  require_skpt_block_not_contains "$worker" "social.dais.social"
  require_skpt_block_not_contains "$worker" "pds.dais.social"
  require_skpt_block_not_contains "$worker" "dais-social"
  require_skpt_block_not_contains "$worker" 'bucket_name = "dais-media"'
  require_skpt_block_not_contains "$worker" 'queue = "delivery-queue"'
  require_skpt_block_not_contains "$worker" 'dead_letter_queue = "delivery-dlq"'
done
ok "all worker skpt env blocks use skpt worker names and avoid production resources"

for worker in actor auth inbox landing outbox router webfinger; do
  require_skpt_block_contains "$worker" 'DOMAIN = "skpt.cl"'
  require_skpt_block_contains "$worker" 'ACTIVITYPUB_DOMAIN = "social.skpt.cl"'
done
ok "skpt worker domains are isolated"

for worker in actor auth inbox outbox pds router webfinger; do
  require_skpt_block_contains "$worker" 'database_name = "dais-skpt"'
  require_skpt_block_contains "$worker" 'database_id = "39490363-8871-4c24-b28e-4873e6a25a0a"'
done
ok "skpt workers use the skpt D1 database"

for worker in delivery-queue outbox pds router; do
  require_skpt_block_contains "$worker" 'bucket_name = "dais-media-skpt"'
done
ok "skpt media-capable workers use the skpt R2 bucket"

require_skpt_block_contains actor 'queue = "delivery-queue-skpt"'
require_skpt_block_contains delivery-queue 'queue = "delivery-queue-skpt"'
require_skpt_block_contains delivery-queue 'dead_letter_queue = "delivery-dlq-skpt"'
ok "skpt delivery queue and DLQ are isolated"

require_skpt_block_contains router '{ pattern = "social.skpt.cl", custom_domain = true }'
require_skpt_block_contains pds '{ pattern = "pds.skpt.cl", custom_domain = true }'
require_skpt_block_contains landing '{ pattern = "skpt.cl/.well-known/webfinger*", zone_name = "skpt.cl" }'
require_skpt_block_contains pds 'PDS_HOSTNAME = "pds.skpt.cl"'
require_skpt_block_contains pds 'DOMAIN = "social.skpt.cl"'
ok "skpt custom domains and PDS hostname are isolated"
