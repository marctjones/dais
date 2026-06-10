#!/usr/bin/env bash

set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:8790}"
ACTOR="${ACTOR:-social}"
ACTOR_URL="${ACTOR_URL:-https://social.dais.social/users/social}"
CLI=(cargo run --quiet --manifest-path client/Cargo.toml --)

tmpdir="$(mktemp -d /tmp/dais-private-mode.XXXXXX)"
trap 'rm -rf "$tmpdir"' EXIT

echo "Checking inbox rejects unsigned requests..."
status="$(
  curl -s -o "$tmpdir/inbox.body" -w "%{http_code}" \
    -H "Content-Type: application/activity+json" \
    -d '{}' \
    "$BASE_URL/users/$ACTOR/inbox" || true
)"

if [[ "$status" != "401" && "$status" != "400" ]]; then
  echo "Expected unsigned inbox POST to be rejected, got HTTP $status"
  cat "$tmpdir/inbox.body"
  exit 1
fi

echo "Reading ActivityPub home timeline from local D1..."
"${CLI[@]}" timeline home --protocol activitypub --remote --limit 5 >/dev/null

echo "Reading derived friends view from local D1..."
"${CLI[@]}" friends list --remote --actor "$ACTOR_URL" --limit 5 >/dev/null

echo "Round-tripping E2EE encrypt/decrypt..."
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$tmpdir/private.pem" >/dev/null 2>&1
openssl rsa -pubout -in "$tmpdir/private.pem" -out "$tmpdir/public.pem" >/dev/null 2>&1

"${CLI[@]}" e2ee encrypt \
  "private mode smoke test" \
  --recipient "https://example.com/users/alice#main-key=$tmpdir/public.pem" \
  > "$tmpdir/encrypted.json"

"${CLI[@]}" e2ee decrypt \
  "$tmpdir/encrypted.json" \
  --private-key "$tmpdir/private.pem" \
  --key-id "https://example.com/users/alice#main-key" \
  > "$tmpdir/decrypted.txt"

if ! grep -q "private mode smoke test" "$tmpdir/decrypted.txt"; then
  echo "E2EE round-trip failed"
  cat "$tmpdir/decrypted.txt"
  exit 1
fi

echo "Private-mode smoke test passed."
