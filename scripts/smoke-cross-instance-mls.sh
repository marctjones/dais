#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DAIS_URL="${DAIS_URL:-https://social.dais.social}"
SKPT_URL="${SKPT_URL:-https://social.skpt.cl}"
DAIS_ACTOR="${DAIS_ACTOR:-https://social.dais.social/users/social}"
SKPT_ACTOR="${SKPT_ACTOR:-https://social.skpt.cl/users/social}"
DAIS_DEVICE_ID="${DAIS_DEVICE_ID:-dais-cli-mls-device}"
SKPT_DEVICE_ID="${SKPT_DEVICE_ID:-skpt-cli-mls-device}"
SKPT_SECOND_DEVICE_ID="${SKPT_SECOND_DEVICE_ID:-skpt-cli-mls-device-secondary}"
DAIS_OWNER_TOKEN_FILE="${DAIS_OWNER_TOKEN_FILE:-/private/tmp/dais-owner-token-20260614.txt}"
SKPT_OWNER_TOKEN_FILE="${SKPT_OWNER_TOKEN_FILE:-/private/tmp/dais-skpt-owner-token.txt}"
DAIS_DELIVERY_WORKER_URL="${DAIS_DELIVERY_WORKER_URL:-https://delivery-queue-production.marc-t-jones.workers.dev}"
SKPT_DELIVERY_WORKER_URL="${SKPT_DELIVERY_WORKER_URL:-https://delivery-queue-skpt.marc-t-jones.workers.dev}"
DAIS_DELIVERY_ADMIN_TOKEN_FILE="${DAIS_DELIVERY_ADMIN_TOKEN_FILE:-/private/tmp/dais-delivery-admin-token.txt}"
SKPT_DELIVERY_ADMIN_TOKEN_FILE="${SKPT_DELIVERY_ADMIN_TOKEN_FILE:-/private/tmp/dais-skpt-delivery-admin-token.txt}"
DAIS_AUDIENCE_LIST_ID="${DAIS_AUDIENCE_LIST_ID:-mls-smoke-skpt}"
SKPT_AUDIENCE_LIST_ID="${SKPT_AUDIENCE_LIST_ID:-mls-smoke-dais}"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
MESSAGE_TEXT="${MESSAGE_TEXT:-cross-instance mls smoke $RUN_ID}"
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

actor_has_mls_device() {
  local actor_url="$1"
  local device_id="$2"
  local body
  body="$(curl -fsS --max-time 20 -H 'Accept: application/activity+json' "$actor_url")"
  grep -Fq "\"deviceId\":\"$device_id\"" <<< "$body" \
    && grep -Fq '"protocol":"mls-rfc9420"' <<< "$body"
}

owner_has_mls_device() {
  local base_url="$1"
  local token="$2"
  local device_id="$3"
  local output
  output="$(owner "$base_url" "$token" e2ee-devices)"
  grep -F "$device_id" <<< "$output" | grep -Fq "mls-rfc9420"
}

ensure_mls_device() {
  local label="$1"
  local base_url="$2"
  local token="$3"
  local actor_url="$4"
  local device_id="$5"

  if owner_has_mls_device "$base_url" "$token" "$device_id"; then
    ok "$label MLS owner device $device_id exists"
  elif [ "$INIT_DEVICES" = "1" ]; then
    owner "$base_url" "$token" e2ee-mls-device-init \
      --device-id "$device_id" \
      --display-name "$label MLS CLI device" \
      --force
    ok "$label MLS owner device $device_id initialized"
  else
    fail "$label MLS owner device $device_id missing and INIT_DEVICES=0"
  fi

  if actor_has_mls_device "$actor_url" "$device_id"; then
    ok "$label actor publishes MLS device $device_id"
  else
    fail "$label actor does not publish MLS device $device_id"
  fi
}

discover_and_trust_mls() {
  local label="$1"
  local base_url="$2"
  local token="$3"
  local actor_url="$4"
  local device_id="$5"

  owner "$base_url" "$token" e2ee-peer-discover "$actor_url" >/tmp/dais-mls-discover.out
  if ! grep -F "$device_id" /tmp/dais-mls-discover.out | grep -Fq "mls-rfc9420"; then
    cat /tmp/dais-mls-discover.out >&2
    fail "$label did not discover MLS device $device_id"
  fi
  ok "$label discovered MLS device $device_id"

  owner "$base_url" "$token" e2ee-peer-trust \
    --actor-id "$actor_url" \
    --device-id "$device_id" >/tmp/dais-mls-trust.out
  if ! grep -F "$device_id" /tmp/dais-mls-trust.out | grep -Fq "[trusted]"; then
    cat /tmp/dais-mls-trust.out >&2
    fail "$label did not trust MLS device $device_id"
  fi
  ok "$label trusted MLS device $device_id"
}

latest_mls_message_id() {
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
    "$worker_url/deliveries/process" >/tmp/dais-mls-delivery-process.out
  ok "$label delivery $delivery_id processed"
}

ensure_audience_list() {
  local label="$1"
  local base_url="$2"
  local token="$3"
  local list_id="$4"
  local member_actor="$5"

  curl -fsS --max-time 20 \
    -H "Authorization: Bearer $token" \
    -H "Content-Type: application/json" \
    -d "{\"id\":\"$list_id\",\"name\":\"MLS smoke $label\",\"description\":\"Temporary MLS smoke audience\",\"member_actor_ids\":[\"$member_actor\"]}" \
    "$base_url/api/dais/owner/audience-lists" >/tmp/dais-mls-audience.out
  ok "$label audience list $list_id includes $member_actor"
}

send_and_decrypt_mls() {
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
  local group_id="${12}"

  local before after send_output delivery_ids
  before="$(latest_mls_message_id "$recipient_url" "$recipient_token" || true)"
  send_output="$(owner "$sender_url" "$sender_token" e2ee-mls-send \
    --recipient-actor-id "$recipient_actor" \
    --recipient-device-id "$recipient_device" \
    --sender-device-id "$sender_device" \
    --group-id "$group_id" \
    "$MESSAGE_TEXT")"
  printf '%s\n' "$send_output"
  grep -Fq "mls_epoch=" <<< "$send_output" || fail "$sender_label MLS send did not print epoch"
  ok "$sender_label sent MLS message to $recipient_label"

  delivery_ids="$(awk -F= '/^delivery_ids=/ {print $2; exit}' <<< "$send_output")"
  if [ -n "$delivery_ids" ]; then
    IFS=',' read -r -a delivery_id_list <<< "$delivery_ids"
    for delivery_id in "${delivery_id_list[@]}"; do
      process_delivery_if_possible "$sender_label" "$sender_delivery_worker_url" "$sender_delivery_admin_token" "$delivery_id"
    done
  fi

  sleep 2
  after="$(latest_mls_message_id "$recipient_url" "$recipient_token" || true)"
  if [ -z "$after" ] || [ "$after" = "$before" ]; then
    fail "$recipient_label did not receive a new MLS E2EE message"
  fi

  local plaintext
  plaintext="$(owner "$recipient_url" "$recipient_token" e2ee-mls-decrypt "$after" --device-id "$recipient_device")"
  if [ "$plaintext" != "$MESSAGE_TEXT" ]; then
    printf 'expected: %s\nactual: %s\n' "$MESSAGE_TEXT" "$plaintext" >&2
    fail "$recipient_label decrypted MLS plaintext mismatch"
  fi
  ok "$recipient_label decrypted received MLS message"
}

send_and_decrypt_mls_group() {
  local sender_label="$1"
  local sender_url="$2"
  local sender_token="$3"
  local sender_device="$4"
  local sender_delivery_worker_url="$5"
  local sender_delivery_admin_token="$6"
  local recipient_label="$7"
  local recipient_url="$8"
  local recipient_token="$9"
  local recipient_device="${10}"
  local audience_list_id="${11}"
  local group_id="${12}"

  local before after send_output delivery_ids
  before="$(latest_mls_message_id "$recipient_url" "$recipient_token" || true)"
  send_output="$(owner "$sender_url" "$sender_token" e2ee-mls-group-send \
    --audience-list-id "$audience_list_id" \
    --sender-device-id "$sender_device" \
    --group-id "$group_id" \
    "$MESSAGE_TEXT")"
  printf '%s\n' "$send_output"
  grep -Fq "wire_material=daisEncryptedMessage-v2-mls-rfc9420" <<< "$send_output" \
    || fail "$sender_label MLS group send did not use MLS v2 wire material"
  ok "$sender_label sent MLS group message to $recipient_label"

  delivery_ids="$(awk -F= '/^delivery_ids=/ {print $2; exit}' <<< "$send_output")"
  if [ -n "$delivery_ids" ]; then
    IFS=',' read -r -a delivery_id_list <<< "$delivery_ids"
    for delivery_id in "${delivery_id_list[@]}"; do
      process_delivery_if_possible "$sender_label group" "$sender_delivery_worker_url" "$sender_delivery_admin_token" "$delivery_id"
    done
  fi

  sleep 2
  after="$(latest_mls_message_id "$recipient_url" "$recipient_token" || true)"
  if [ -z "$after" ] || [ "$after" = "$before" ]; then
    fail "$recipient_label did not receive a new MLS group E2EE message"
  fi

  local plaintext
  plaintext="$(owner "$recipient_url" "$recipient_token" e2ee-mls-decrypt "$after" --device-id "$recipient_device")"
  if [ "$plaintext" != "$MESSAGE_TEXT" ]; then
    printf 'expected: %s\nactual: %s\n' "$MESSAGE_TEXT" "$plaintext" >&2
    fail "$recipient_label decrypted MLS group plaintext mismatch"
  fi
  ok "$recipient_label decrypted received MLS group message"
}

latest_mls_message_id_after() {
  local label="$1"
  local base_url="$2"
  local token="$3"
  local before="$4"

  local after
  after="$(latest_mls_message_id "$base_url" "$token" || true)"
  if [ -z "$after" ] || [ "$after" = "$before" ]; then
    fail "$label did not receive a new MLS group E2EE message"
  fi
  printf '%s' "$after"
}

decrypt_mls_expect_plaintext() {
  local label="$1"
  local base_url="$2"
  local token="$3"
  local message_id="$4"
  local device_id="$5"
  local expected="$6"

  local plaintext
  plaintext="$(owner "$base_url" "$token" e2ee-mls-decrypt "$message_id" --device-id "$device_id")"
  if [ "$plaintext" != "$expected" ]; then
    printf 'expected: %s\nactual: %s\n' "$expected" "$plaintext" >&2
    fail "$label decrypted MLS plaintext mismatch on $device_id"
  fi
  ok "$label decrypted MLS group message with $device_id"
}

decrypt_mls_expect_failure() {
  local label="$1"
  local base_url="$2"
  local token="$3"
  local message_id="$4"
  local device_id="$5"

  set +e
  local output
  output="$(owner "$base_url" "$token" e2ee-mls-decrypt "$message_id" --device-id "$device_id" 2>&1)"
  local status=$?
  set -e
  if [ "$status" -eq 0 ]; then
    printf '%s\n' "$output" >&2
    fail "$label unexpectedly decrypted MLS group message with removed device $device_id"
  fi
  ok "$label rejected MLS decrypt with removed device $device_id"
}

send_group_and_decrypt_on_two_devices() {
  local sender_label="$1"
  local sender_url="$2"
  local sender_token="$3"
  local sender_device="$4"
  local sender_delivery_worker_url="$5"
  local sender_delivery_admin_token="$6"
  local recipient_label="$7"
  local recipient_url="$8"
  local recipient_token="$9"
  local primary_device="${10}"
  local secondary_device="${11}"
  local audience_list_id="${12}"
  local group_id="${13}"
  local plaintext="${14}"

  local before after send_output delivery_ids
  before="$(latest_mls_message_id "$recipient_url" "$recipient_token" || true)"
  send_output="$(owner "$sender_url" "$sender_token" e2ee-mls-group-send \
    --audience-list-id "$audience_list_id" \
    --sender-device-id "$sender_device" \
    --group-id "$group_id" \
    "$plaintext")"
  printf '%s\n' "$send_output"
  # A group send must fan out to every trusted device of the recipient, so the
  # count is at least the primary plus the secondary. It is not pinned to an
  # exact number: these instances are shared with the other smoke gates, which
  # register devices of their own, and encrypting to those as well is correct
  # MLS behaviour rather than a defect. The property that matters — that the
  # secondary device is really reached — is proved by decrypting there below.
  local device_count
  device_count="$(awk -F= '/^recipient_device_count=/ {print $2; exit}' <<< "$send_output")"
  case "$device_count" in
    ''|*[!0-9]*) fail "$sender_label MLS group send reported no recipient_device_count" ;;
  esac
  [ "$device_count" -ge 2 ] \
    || fail "$sender_label MLS group send reached $device_count recipient device(s); expected the primary and secondary"
  ok "$sender_label sent MLS group message to $device_count $recipient_label devices"

  delivery_ids="$(awk -F= '/^delivery_ids=/ {print $2; exit}' <<< "$send_output")"
  if [ -n "$delivery_ids" ]; then
    IFS=',' read -r -a delivery_id_list <<< "$delivery_ids"
    for delivery_id in "${delivery_id_list[@]}"; do
      process_delivery_if_possible "$sender_label group multi-device" "$sender_delivery_worker_url" "$sender_delivery_admin_token" "$delivery_id"
    done
  fi

  sleep 2
  after="$(latest_mls_message_id_after "$recipient_label" "$recipient_url" "$recipient_token" "$before")"
  decrypt_mls_expect_plaintext "$recipient_label primary" "$recipient_url" "$recipient_token" "$after" "$primary_device" "$plaintext"
  decrypt_mls_expect_plaintext "$recipient_label secondary" "$recipient_url" "$recipient_token" "$after" "$secondary_device" "$plaintext"
}

send_group_after_device_removal() {
  local sender_label="$1"
  local sender_url="$2"
  local sender_token="$3"
  local sender_device="$4"
  local sender_delivery_worker_url="$5"
  local sender_delivery_admin_token="$6"
  local recipient_label="$7"
  local recipient_url="$8"
  local recipient_token="$9"
  local remaining_device="${10}"
  local removed_device="${11}"
  local audience_list_id="${12}"
  local group_id="${13}"
  local plaintext="${14}"

  owner "$sender_url" "$sender_token" e2ee-peer-revoke \
    --actor-id "$SKPT_ACTOR" \
    --device-id "$removed_device" >/tmp/dais-mls-peer-revoke.out
  grep -F "$removed_device" /tmp/dais-mls-peer-revoke.out | grep -Fq "[revoked]" \
    || fail "$sender_label did not revoke MLS peer device $removed_device"
  ok "$sender_label revoked MLS peer device $removed_device"

  local before after send_output delivery_ids
  before="$(latest_mls_message_id "$recipient_url" "$recipient_token" || true)"
  send_output="$(owner "$sender_url" "$sender_token" e2ee-mls-group-send \
    --audience-list-id "$audience_list_id" \
    --sender-device-id "$sender_device" \
    --group-id "$group_id" \
    "$plaintext")"
  printf '%s\n' "$send_output"
  # The security property is exclusion, not a head count: after revoking a peer
  # device, a group send must not encrypt to it, while still reaching the device
  # that is still trusted. Asserting an exact count instead would pass or fail on
  # unrelated devices registered by the other smoke gates against this instance.
  local recipient_devices
  recipient_devices="$(awk -F= '/^recipient_devices=/ {print $2; exit}' <<< "$send_output")"
  [ -n "$recipient_devices" ] \
    || fail "$sender_label MLS group send did not report recipient_devices"
  grep -q "\(^\|,\)${removed_device}\(,\|$\)" <<< "$recipient_devices" \
    && fail "$sender_label MLS group send still encrypted to revoked device $removed_device"
  grep -q "\(^\|,\)${remaining_device}\(,\|$\)" <<< "$recipient_devices" \
    || fail "$sender_label MLS group send did not reach still-trusted device $remaining_device"
  ok "$sender_label MLS group send excluded revoked $removed_device (sent to $recipient_devices)"
  ok "$sender_label sent MLS group message after removing $removed_device"

  delivery_ids="$(awk -F= '/^delivery_ids=/ {print $2; exit}' <<< "$send_output")"
  if [ -n "$delivery_ids" ]; then
    IFS=',' read -r -a delivery_id_list <<< "$delivery_ids"
    for delivery_id in "${delivery_id_list[@]}"; do
      process_delivery_if_possible "$sender_label group removed-device" "$sender_delivery_worker_url" "$sender_delivery_admin_token" "$delivery_id"
    done
  fi

  sleep 2
  after="$(latest_mls_message_id_after "$recipient_label" "$recipient_url" "$recipient_token" "$before")"
  decrypt_mls_expect_plaintext "$recipient_label remaining" "$recipient_url" "$recipient_token" "$after" "$remaining_device" "$plaintext"
  decrypt_mls_expect_failure "$recipient_label removed" "$recipient_url" "$recipient_token" "$after" "$removed_device"
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

ensure_mls_device "dais.social" "$DAIS_URL" "$DAIS_TOKEN" "$DAIS_ACTOR" "$DAIS_DEVICE_ID"
ensure_mls_device "skpt" "$SKPT_URL" "$SKPT_TOKEN" "$SKPT_ACTOR" "$SKPT_DEVICE_ID"
ensure_mls_device "skpt secondary" "$SKPT_URL" "$SKPT_TOKEN" "$SKPT_ACTOR" "$SKPT_SECOND_DEVICE_ID"

discover_and_trust_mls "dais.social -> skpt" "$DAIS_URL" "$DAIS_TOKEN" "$SKPT_ACTOR" "$SKPT_DEVICE_ID"
discover_and_trust_mls "dais.social -> skpt secondary" "$DAIS_URL" "$DAIS_TOKEN" "$SKPT_ACTOR" "$SKPT_SECOND_DEVICE_ID"
discover_and_trust_mls "skpt -> dais.social" "$SKPT_URL" "$SKPT_TOKEN" "$DAIS_ACTOR" "$DAIS_DEVICE_ID"

ensure_audience_list "dais.social" "$DAIS_URL" "$DAIS_TOKEN" "$DAIS_AUDIENCE_LIST_ID" "$SKPT_ACTOR"
ensure_audience_list "skpt" "$SKPT_URL" "$SKPT_TOKEN" "$SKPT_AUDIENCE_LIST_ID" "$DAIS_ACTOR"

send_and_decrypt_mls \
  "dais.social" "$DAIS_URL" "$DAIS_TOKEN" "$DAIS_DEVICE_ID" "$DAIS_DELIVERY_WORKER_URL" "$DAIS_DELIVERY_ADMIN_TOKEN" \
  "skpt" "$SKPT_URL" "$SKPT_TOKEN" "$SKPT_ACTOR" "$SKPT_DEVICE_ID" \
  "dais-mls-live-dais-to-skpt"

send_and_decrypt_mls \
  "skpt" "$SKPT_URL" "$SKPT_TOKEN" "$SKPT_DEVICE_ID" "$SKPT_DELIVERY_WORKER_URL" "$SKPT_DELIVERY_ADMIN_TOKEN" \
  "dais.social" "$DAIS_URL" "$DAIS_TOKEN" "$DAIS_ACTOR" "$DAIS_DEVICE_ID" \
  "dais-mls-live-skpt-to-dais"

send_and_decrypt_mls_group \
  "dais.social" "$DAIS_URL" "$DAIS_TOKEN" "$DAIS_DEVICE_ID" "$DAIS_DELIVERY_WORKER_URL" "$DAIS_DELIVERY_ADMIN_TOKEN" \
  "skpt" "$SKPT_URL" "$SKPT_TOKEN" "$SKPT_DEVICE_ID" "$DAIS_AUDIENCE_LIST_ID" \
  "dais-mls-live-group-dais-to-skpt-$RUN_ID"

send_and_decrypt_mls_group \
  "skpt" "$SKPT_URL" "$SKPT_TOKEN" "$SKPT_DEVICE_ID" "$SKPT_DELIVERY_WORKER_URL" "$SKPT_DELIVERY_ADMIN_TOKEN" \
  "dais.social" "$DAIS_URL" "$DAIS_TOKEN" "$DAIS_DEVICE_ID" "$SKPT_AUDIENCE_LIST_ID" \
  "dais-mls-live-group-skpt-to-dais-$RUN_ID"

send_group_and_decrypt_on_two_devices \
  "dais.social" "$DAIS_URL" "$DAIS_TOKEN" "$DAIS_DEVICE_ID" "$DAIS_DELIVERY_WORKER_URL" "$DAIS_DELIVERY_ADMIN_TOKEN" \
  "skpt" "$SKPT_URL" "$SKPT_TOKEN" "$SKPT_DEVICE_ID" "$SKPT_SECOND_DEVICE_ID" "$DAIS_AUDIENCE_LIST_ID" \
  "dais-mls-live-group-two-skpt-devices-$RUN_ID" \
  "cross-instance mls two-device smoke $RUN_ID"

send_group_after_device_removal \
  "dais.social" "$DAIS_URL" "$DAIS_TOKEN" "$DAIS_DEVICE_ID" "$DAIS_DELIVERY_WORKER_URL" "$DAIS_DELIVERY_ADMIN_TOKEN" \
  "skpt" "$SKPT_URL" "$SKPT_TOKEN" "$SKPT_DEVICE_ID" "$SKPT_SECOND_DEVICE_ID" "$DAIS_AUDIENCE_LIST_ID" \
  "dais-mls-live-group-removed-skpt-device-$RUN_ID" \
  "cross-instance mls removed-device smoke $RUN_ID"

ok "MLS topology coverage: two independently managed actors, three live MLS devices, two trusted recipient devices for one actor, and removed-device decrypt failure"
