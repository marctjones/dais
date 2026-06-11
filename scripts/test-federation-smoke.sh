#!/usr/bin/env bash

set -euo pipefail

DAIS_BASE_URL="${DAIS_BASE_URL:-http://localhost:8790}"
DAIS_ACTOR="${DAIS_ACTOR:-social}"
DAIS_ACTOR_URL="${DAIS_ACTOR_URL:-https://social.dais.social/users/social}"
DAIS_CLI=(cargo run --quiet --manifest-path client/Cargo.toml --)
TOOT_BIN="${TOOT_BIN:-toot}"
TOOT_ACCOUNT="${TOOT_ACCOUNT:-}"
FOLLOWER_STATUS_URL="${FOLLOWER_STATUS_URL:-}"
PUBLIC_STATUS_URL="${PUBLIC_STATUS_URL:-}"
REMOTE_HANDLE="${REMOTE_HANDLE:-}"
DELIVERY_ADMIN_TOKEN="${DELIVERY_ADMIN_TOKEN:-}"
REMOTE_TIMELINE_ASSERT="${REMOTE_TIMELINE_ASSERT:-0}"
DIRECT_RECIPIENT="${DIRECT_RECIPIENT:-}"
SMOKE_POST_TEXT="${SMOKE_POST_TEXT:-dais federation smoke $(date +%s)}"
SMOKE_REPLY_TEXT="${SMOKE_REPLY_TEXT:-dais federation reply smoke $(date +%s)}"
SMOKE_DIRECT_TEXT="${SMOKE_DIRECT_TEXT:-dais federation direct smoke $(date +%s)}"
RUN_LIVE_DELIVERY="${RUN_LIVE_DELIVERY:-}"

tmpdir="$(mktemp -d /tmp/dais-federation-smoke.XXXXXX)"
trap 'rm -rf "$tmpdir"' EXIT

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1"
    exit 1
  }
}

require_cmd curl
require_cmd jq
require_cmd cargo

if ! command -v "$TOOT_BIN" >/dev/null 2>&1; then
  echo "toot not found at $TOOT_BIN"
  exit 1
fi

dispatch_delivery() {
  local delivery_id="$1"
  if [[ -n "$DELIVERY_ADMIN_TOKEN" ]]; then
    "${DAIS_CLI[@]}" deliveries process "$delivery_id" \
      --base-url "$DAIS_BASE_URL" >/dev/null
  else
    "${DAIS_CLI[@]}" deliveries enqueue "$delivery_id" \
      --base-url "$DAIS_BASE_URL" >/dev/null
  fi
}

echo "Checking dais actor endpoint..."
curl -fsS -H "Accept: application/activity+json" \
  "$DAIS_BASE_URL/users/$DAIS_ACTOR" \
  | jq -e '.type == "Person"' >/dev/null

echo "Checking unsigned inbox rejection..."
status="$(
  curl -s -o "$tmpdir/inbox.body" -w "%{http_code}" \
    -H "Content-Type: application/activity+json" \
    -d '{}' \
    "$DAIS_BASE_URL/users/$DAIS_ACTOR/inbox" || true
)"

if [[ "$status" != "401" && "$status" != "400" ]]; then
  echo "Expected unsigned inbox POST to be rejected, got HTTP $status"
  cat "$tmpdir/inbox.body"
  exit 1
fi

echo "Checking local home timeline and friends view..."
"${DAIS_CLI[@]}" timeline home --protocol activitypub --remote --limit 5 >/dev/null
"${DAIS_CLI[@]}" friends list --remote --actor "$DAIS_ACTOR_URL" --limit 5 >/dev/null
echo "Checking approved follower roster..."
"${DAIS_CLI[@]}" followers list --remote --limit 10 | grep -F 'mastodon.social' >/dev/null

echo "Checking E2EE round trip..."
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$tmpdir/private.pem" >/dev/null 2>&1
openssl rsa -pubout -in "$tmpdir/private.pem" -out "$tmpdir/public.pem" >/dev/null 2>&1

"${DAIS_CLI[@]}" e2ee encrypt \
  "federation smoke test" \
  --recipient "https://example.com/users/alice#main-key=$tmpdir/public.pem" \
  > "$tmpdir/encrypted.json"

"${DAIS_CLI[@]}" e2ee decrypt \
  "$tmpdir/encrypted.json" \
  --private-key "$tmpdir/private.pem" \
  --key-id "https://example.com/users/alice#main-key" \
  > "$tmpdir/decrypted.txt"

grep -q "federation smoke test" "$tmpdir/decrypted.txt"

if [[ -z "$RUN_LIVE_DELIVERY" ]]; then
  if [[ -n "$DELIVERY_ADMIN_TOKEN" || "$DAIS_BASE_URL" == https://* || "$REMOTE_TIMELINE_ASSERT" == "1" ]]; then
    RUN_LIVE_DELIVERY=1
  else
    RUN_LIVE_DELIVERY=0
  fi
fi

if [[ "$RUN_LIVE_DELIVERY" == "1" ]]; then
  echo "Creating a live ActivityPub delivery test post..."
  post_output="$(
    "${DAIS_CLI[@]}" post create "$SMOKE_POST_TEXT" \
      --protocol activitypub \
      --visibility followers \
      --remote
  )"
  echo "$post_output"
  post_url="$(printf '%s\n' "$post_output" | awk '/^Post: / { print $2; exit }')"

  delivery_ids=()
  while IFS= read -r delivery_id; do
    if [[ -n "$delivery_id" ]]; then
      delivery_ids+=("$delivery_id")
    fi
  done < <(
    printf '%s\n' "$post_output" | awk '
      /^Delivery IDs:$/ { capture=1; next }
      capture && NF == 0 { capture=0 }
      capture { gsub(/^  /, ""); print }
    '
  )

  if [[ "${#delivery_ids[@]}" -eq 0 ]]; then
    echo "No delivery IDs were created for the smoke post."
    exit 1
  fi

  for delivery_id in "${delivery_ids[@]}"; do
    if [[ -n "$DELIVERY_ADMIN_TOKEN" ]]; then
      echo "Processing delivery $delivery_id..."
    else
      echo "Enqueueing delivery $delivery_id..."
    fi
    dispatch_delivery "$delivery_id"
  done

  echo "Waiting for Mastodon home timeline to reflect the delivered post..."
  found=0
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    if "$TOOT_BIN" timelines home --limit 20 --no-pager | grep -F "$SMOKE_POST_TEXT" >/dev/null; then
      found=1
      break
    fi
    sleep 5
  done

  if [[ "$found" -ne 1 ]]; then
    if [[ "$REMOTE_TIMELINE_ASSERT" == "1" ]]; then
      echo "Delivered post was not visible in Mastodon home timeline."
      exit 1
    fi

    echo "Delivered post was not visible in Mastodon home timeline yet; continuing because REMOTE_TIMELINE_ASSERT is disabled."
  fi

  if [[ -n "$post_url" ]]; then
    echo "Creating a live ActivityPub reply delivery test post..."
    reply_output="$(
      "${DAIS_CLI[@]}" post create "$SMOKE_REPLY_TEXT" \
        --protocol activitypub \
        --visibility followers \
        --reply-to "$post_url" \
        --remote
    )"
    echo "$reply_output"

    while IFS= read -r delivery_id; do
      if [[ -n "$delivery_id" ]]; then
        if [[ -n "$DELIVERY_ADMIN_TOKEN" ]]; then
          echo "Processing reply delivery $delivery_id..."
        else
          echo "Enqueueing reply delivery $delivery_id..."
        fi
        dispatch_delivery "$delivery_id"
      fi
    done < <(
      printf '%s\n' "$reply_output" | awk '
        /^Delivery IDs:$/ { capture=1; next }
        capture && NF == 0 { capture=0 }
        capture { gsub(/^  /, ""); print }
      '
    )
  fi

  if [[ -n "$DIRECT_RECIPIENT" ]]; then
    echo "Creating a live ActivityPub direct/private delivery test post..."
    direct_output="$(
      "${DAIS_CLI[@]}" post create "$SMOKE_DIRECT_TEXT" \
        --protocol activitypub \
        --visibility direct \
        --to "$DIRECT_RECIPIENT" \
        --remote
    )"
    echo "$direct_output"

    while IFS= read -r delivery_id; do
      if [[ -n "$delivery_id" ]]; then
        if [[ -n "$DELIVERY_ADMIN_TOKEN" ]]; then
          echo "Processing direct delivery $delivery_id..."
        else
          echo "Enqueueing direct delivery $delivery_id..."
        fi
        dispatch_delivery "$delivery_id"
      fi
    done < <(
      printf '%s\n' "$direct_output" | awk '
        /^Delivery IDs:$/ { capture=1; next }
        capture && NF == 0 { capture=0 }
        capture { gsub(/^  /, ""); print }
      '
    )
  else
    echo "DIRECT_RECIPIENT is not set; direct/private live delivery is skipped."
  fi
else
  echo "Live delivery is skipped. Set RUN_LIVE_DELIVERY=1 with DAIS_BASE_URL=https://social.dais.social to enqueue without DELIVERY_ADMIN_TOKEN."
fi

if [[ -n "$TOOT_ACCOUNT" ]]; then
  echo "Using toot account: $TOOT_ACCOUNT"
fi

if [[ -n "$REMOTE_HANDLE" ]]; then
  echo "Remote handle supplied: $REMOTE_HANDLE"
else
  echo "REMOTE_HANDLE is not set; toot federation steps are skipped."
fi

if [[ -n "$FOLLOWER_STATUS_URL" ]]; then
  echo "Checking follower-only status via curl..."
  curl -fsS "$FOLLOWER_STATUS_URL" >/dev/null
fi

if [[ -n "$PUBLIC_STATUS_URL" ]]; then
  echo "Checking public status via curl..."
  curl -fsS "$PUBLIC_STATUS_URL" >/dev/null
fi

echo "Federation smoke harness completed."
