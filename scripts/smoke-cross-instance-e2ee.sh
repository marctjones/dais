#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DAIS_URL="${DAIS_URL:-https://social.dais.social}"
SKPT_URL="${SKPT_URL:-https://social.skpt.cl}"
DAIS_ACTOR="${DAIS_ACTOR:-https://social.dais.social/users/social}"
SKPT_ACTOR="${SKPT_ACTOR:-https://social.skpt.cl/users/social}"
DAIS_DEVICE_ID="${DAIS_DEVICE_ID:-dais-cli-device}"
SKPT_DEVICE_ID="${SKPT_DEVICE_ID:-skpt-cli-device}"
DAIS_OWNER_TOKEN_FILE="${DAIS_OWNER_TOKEN_FILE:-/private/tmp/dais-owner-token-20260614.txt}"
SKPT_OWNER_TOKEN_FILE="${SKPT_OWNER_TOKEN_FILE:-/private/tmp/dais-skpt-owner-token.txt}"
DAIS_DELIVERY_WORKER_URL="${DAIS_DELIVERY_WORKER_URL:-https://delivery-queue-production.marc-t-jones.workers.dev}"
SKPT_DELIVERY_WORKER_URL="${SKPT_DELIVERY_WORKER_URL:-https://delivery-queue-skpt.marc-t-jones.workers.dev}"
DAIS_DELIVERY_ADMIN_TOKEN_FILE="${DAIS_DELIVERY_ADMIN_TOKEN_FILE:-/private/tmp/dais-delivery-admin-token.txt}"
SKPT_DELIVERY_ADMIN_TOKEN_FILE="${SKPT_DELIVERY_ADMIN_TOKEN_FILE:-/private/tmp/dais-skpt-delivery-admin-token.txt}"
MESSAGE_TEXT="${MESSAGE_TEXT:-cross-instance e2ee smoke $(date -u +%Y%m%dT%H%M%SZ)}"
INIT_DEVICES="${INIT_DEVICES:-1}"
REQUIRE_FULL="${REQUIRE_FULL:-0}"

if [ -n "${DAIS_CLI:-}" ]; then
  read -r -a CLI <<< "$DAIS_CLI"
else
  CLI=(cargo run --quiet --manifest-path "$ROOT_DIR/client/Cargo.toml" --)
fi

ok() {
  printf 'OK   %s\n' "$1"
}

fail() {
  printf 'FAIL %s\n' "$1" >&2
  exit 1
}

skip() {
  printf 'SKIP %s\n' "$1"
}

token_from_env_or_file() {
  local env_name="$1"
  local file_path="$2"
  local value="${!env_name:-}"
  if [ -n "$value" ]; then
    printf '%s' "$value"
    return 0
  fi
  if [ -f "$file_path" ]; then
    tr -d '\n' < "$file_path"
    return 0
  fi
  return 1
}

owner() {
  local base_url="$1"
  local token="$2"
  shift 2
  "${CLI[@]}" owner "$@" --instance-url "$base_url" --owner-token "$token"
}

actor_has_device() {
  local actor_url="$1"
  local device_id="$2"
  local body
  body="$(curl -fsS --max-time 20 -H 'Accept: application/activity+json' "$actor_url")"
  grep -Fq "\"deviceId\":\"$device_id\"" <<< "$body"
}

owner_has_device() {
  local base_url="$1"
  local token="$2"
  local device_id="$3"
  local output
  output="$(owner "$base_url" "$token" e2ee-devices)"
  grep -Fq "$device_id" <<< "$output"
}

ensure_device() {
  local label="$1"
  local base_url="$2"
  local token="$3"
  local actor_url="$4"
  local device_id="$5"
  if [ "$INIT_DEVICES" = "1" ]; then
    owner "$base_url" "$token" e2ee-mls-device-init \
      --device-id "$device_id" \
      --display-name "$label CLI device" \
      --force
    ok "$label MLS owner device $device_id initialized/refreshed"
  elif owner_has_device "$base_url" "$token" "$device_id"; then
    ok "$label owner device $device_id exists"
  else
    fail "$label owner device $device_id missing and INIT_DEVICES=0"
  fi

  if actor_has_device "$actor_url" "$device_id"; then
    ok "$label actor publishes $device_id"
  else
    fail "$label actor does not publish $device_id"
  fi
}

discover_and_trust() {
  local label="$1"
  local base_url="$2"
  local token="$3"
  local actor_url="$4"
  local device_id="$5"

  owner "$base_url" "$token" e2ee-peer-discover "$actor_url" >/tmp/dais-e2ee-discover.out
  if ! grep -Fq "$device_id" /tmp/dais-e2ee-discover.out; then
    cat /tmp/dais-e2ee-discover.out >&2
    fail "$label did not discover $device_id"
  fi
  ok "$label discovered $device_id"

  owner "$base_url" "$token" e2ee-peer-trust \
    --actor-id "$actor_url" \
    --device-id "$device_id" >/tmp/dais-e2ee-trust.out
  if ! grep -Fq "[$(printf trusted)]" /tmp/dais-e2ee-trust.out; then
    cat /tmp/dais-e2ee-trust.out >&2
    fail "$label did not trust $device_id"
  fi
  ok "$label trusted $device_id"
}

latest_message_id() {
  local base_url="$1"
  local token="$2"
  local output
  output="$(owner "$base_url" "$token" e2ee-messages)"
  awk 'BEGIN { id="" } /^https:\/\// { candidate=$1 } /^protocol=mls-rfc9420$/ { id=candidate; print id; exit }' <<< "$output"
}

process_delivery_if_possible() {
  local label="$1"
  local worker_url="$2"
  local admin_token="$3"
  local delivery_id="$4"

  if [ -z "$delivery_id" ] || [ "$delivery_id" = "[]" ]; then
    return 0
  fi
  if [ -z "$worker_url" ] || [ -z "$admin_token" ]; then
    skip "$label delivery $delivery_id not processed; delivery admin token unavailable"
    return 0
  fi

  curl -fsS --max-time 30 \
    -H "Content-Type: application/json" \
    -H "X-Dais-Admin-Token: $admin_token" \
    -d "{\"delivery_id\":\"$delivery_id\"}" \
    "$worker_url/deliveries/process" >/tmp/dais-delivery-process.out
  ok "$label delivery $delivery_id processed"
}

send_and_decrypt() {
  local sender_label="$1"
  local sender_url="$2"
  local sender_token="$3"
  local sender_device="$4"
  local sender_delivery_worker_url="$5"
  local sender_delivery_admin_token="$6"
  local recipient_label="$7"
  local recipient_url="$8"
  local recipient_token="$9"
  local recipient_actor="${10}"
  local recipient_device="${11}"

  local before after send_output delivery_ids delivery_id
  before="$(latest_message_id "$recipient_url" "$recipient_token" || true)"
  send_output="$(owner "$sender_url" "$sender_token" e2ee-mls-send \
    --recipient-actor-id "$recipient_actor" \
    --recipient-device-id "$recipient_device" \
    --sender-device-id "$sender_device" \
    "$MESSAGE_TEXT")"
  printf '%s\n' "$send_output"
  ok "$sender_label sent encrypted message to $recipient_label"

  delivery_ids="$(awk -F= '/^delivery_ids=/ {print $2; exit}' <<< "$send_output")"
  if [ -n "$delivery_ids" ]; then
    IFS=',' read -r -a delivery_id_list <<< "$delivery_ids"
    for delivery_id in "${delivery_id_list[@]}"; do
      process_delivery_if_possible "$sender_label" "$sender_delivery_worker_url" "$sender_delivery_admin_token" "$delivery_id"
    done
  fi

  sleep 2
  after="$(latest_message_id "$recipient_url" "$recipient_token" || true)"
  if [ -z "$after" ] || [ "$after" = "$before" ]; then
    fail "$recipient_label did not receive a new E2EE message"
  fi

  local plaintext
  plaintext="$(owner "$recipient_url" "$recipient_token" e2ee-mls-decrypt "$after" --device-id "$recipient_device")"
  if [ "$plaintext" != "$MESSAGE_TEXT" ]; then
    printf 'expected: %s\nactual: %s\n' "$MESSAGE_TEXT" "$plaintext" >&2
    fail "$recipient_label decrypted plaintext mismatch"
  fi
  ok "$recipient_label decrypted received E2EE message"
}

DAIS_TOKEN="$(token_from_env_or_file DAIS_OWNER_TOKEN "$DAIS_OWNER_TOKEN_FILE" || true)"
SKPT_TOKEN="$(token_from_env_or_file SKPT_OWNER_TOKEN "$SKPT_OWNER_TOKEN_FILE" || true)"
DAIS_DELIVERY_ADMIN_TOKEN="$(token_from_env_or_file DAIS_DELIVERY_ADMIN_TOKEN "$DAIS_DELIVERY_ADMIN_TOKEN_FILE" || true)"
SKPT_DELIVERY_ADMIN_TOKEN="$(token_from_env_or_file SKPT_DELIVERY_ADMIN_TOKEN "$SKPT_DELIVERY_ADMIN_TOKEN_FILE" || true)"

curl -fsS --max-time 20 -H 'Accept: application/activity+json' "$DAIS_ACTOR" >/dev/null
ok "dais.social actor fetch"
curl -fsS --max-time 20 -H 'Accept: application/activity+json' "$SKPT_ACTOR" >/dev/null
ok "skpt actor fetch"

if [ -z "$DAIS_TOKEN" ]; then
  message="dais.social owner token unavailable; set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE=$DAIS_OWNER_TOKEN_FILE"
  if [ "$REQUIRE_FULL" = "1" ]; then
    fail "$message"
  fi
  skip "$message"
  if actor_has_device "$SKPT_ACTOR" "$SKPT_DEVICE_ID"; then
    ok "skpt actor publishes $SKPT_DEVICE_ID"
  fi
  if actor_has_device "$DAIS_ACTOR" "$DAIS_DEVICE_ID"; then
    ok "dais.social actor publishes $DAIS_DEVICE_ID"
  else
    skip "dais.social actor does not publish $DAIS_DEVICE_ID yet"
  fi
  exit 0
fi

if [ -z "$SKPT_TOKEN" ]; then
  message="skpt owner token unavailable; set SKPT_OWNER_TOKEN or SKPT_OWNER_TOKEN_FILE=$SKPT_OWNER_TOKEN_FILE"
  if [ "$REQUIRE_FULL" = "1" ]; then
    fail "$message"
  fi
  skip "$message"
  exit 0
fi

if [ "$REQUIRE_FULL" = "1" ]; then
  [ -n "$DAIS_DELIVERY_ADMIN_TOKEN" ] \
    || fail "dais.social delivery admin token unavailable; set DAIS_DELIVERY_ADMIN_TOKEN or DAIS_DELIVERY_ADMIN_TOKEN_FILE=$DAIS_DELIVERY_ADMIN_TOKEN_FILE"
  [ -n "$SKPT_DELIVERY_ADMIN_TOKEN" ] \
    || fail "skpt delivery admin token unavailable; set SKPT_DELIVERY_ADMIN_TOKEN or SKPT_DELIVERY_ADMIN_TOKEN_FILE=$SKPT_DELIVERY_ADMIN_TOKEN_FILE"
fi

ensure_device "dais.social" "$DAIS_URL" "$DAIS_TOKEN" "$DAIS_ACTOR" "$DAIS_DEVICE_ID"
ensure_device "skpt" "$SKPT_URL" "$SKPT_TOKEN" "$SKPT_ACTOR" "$SKPT_DEVICE_ID"

discover_and_trust "dais.social -> skpt" "$DAIS_URL" "$DAIS_TOKEN" "$SKPT_ACTOR" "$SKPT_DEVICE_ID"
discover_and_trust "skpt -> dais.social" "$SKPT_URL" "$SKPT_TOKEN" "$DAIS_ACTOR" "$DAIS_DEVICE_ID"

send_and_decrypt \
  "dais.social" "$DAIS_URL" "$DAIS_TOKEN" "$DAIS_DEVICE_ID" "$DAIS_DELIVERY_WORKER_URL" "$DAIS_DELIVERY_ADMIN_TOKEN" \
  "skpt" "$SKPT_URL" "$SKPT_TOKEN" "$SKPT_ACTOR" "$SKPT_DEVICE_ID"

send_and_decrypt \
  "skpt" "$SKPT_URL" "$SKPT_TOKEN" "$SKPT_DEVICE_ID" "$SKPT_DELIVERY_WORKER_URL" "$SKPT_DELIVERY_ADMIN_TOKEN" \
  "dais.social" "$DAIS_URL" "$DAIS_TOKEN" "$DAIS_ACTOR" "$DAIS_DEVICE_ID"
