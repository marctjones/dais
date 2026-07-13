#!/usr/bin/env bash
set -euo pipefail

OWNER_TOKEN_FILE="${OWNER_TOKEN_FILE:-/private/tmp/dais-skpt-owner-token.txt}"
REQUIRE_FULL="${REQUIRE_FULL:-0}"

check_contains() {
  local label="$1" url="$2" needle="$3"
  local body
  body="$(curl -fsS --max-time 20 "$url")"
  if ! printf "%s" "$body" | grep -Fq "$needle"; then
    echo "FAIL $label: missing '$needle'" >&2
    return 1
  fi
  echo "OK   $label"
}

check_status() {
  local label="$1" expected="$2" url="$3"
  local status
  status="$(curl -sS -o /dev/null -w "%{http_code}" --max-time 20 "$url")"
  if [ "$status" != "$expected" ]; then
    echo "FAIL $label: expected HTTP $expected, got $status" >&2
    return 1
  fi
  echo "OK   $label"
}

check_contains \
  "skpt ActivityPub actor" \
  "https://social.skpt.cl/users/social" \
  "Skeptical Engineering"

check_contains \
  "skpt E2EE device discovery" \
  "https://social.skpt.cl/users/social?format=json" \
  "daisE2ee"

check_contains \
  "skpt apex WebFinger" \
  "https://skpt.cl/.well-known/webfinger?resource=acct:social@skpt.cl" \
  "https://social.skpt.cl/users/social"

check_contains \
  "skpt PDS describeServer" \
  "https://pds.skpt.cl/xrpc/com.atproto.server.describeServer" \
  "did:web:social.skpt.cl"

check_status \
  "skpt owner API requires bearer" \
  "401" \
  "https://social.skpt.cl/api/dais/owner/profile"

inbox_status="$(
  curl -sS -o /dev/null -w "%{http_code}" --max-time 20 \
    -H "Content-Type: application/activity+json" \
    -d '{"@context":"https://www.w3.org/ns/activitystreams","id":"https://example.invalid/activities/test","type":"Create","actor":"https://example.invalid/users/alice","to":["https://social.skpt.cl/users/social"],"object":{"id":"https://example.invalid/users/alice/messages/test","type":"Note","to":["https://social.skpt.cl/users/social"],"content":"unsigned encrypted fallback","daisEncryptedMessage":{"v":2,"protocol":"mls-rfc9420","groupId":"dGVzdC1ncm91cA==","epoch":1,"senderDeviceId":"test-device","ciphertext":"Y2lwaGVydGV4dA=="}}}' \
    "https://social.skpt.cl/users/social/inbox"
)"
if [ "$inbox_status" != "401" ]; then
  echo "FAIL skpt unsigned encrypted inbox rejection: expected HTTP 401, got $inbox_status" >&2
  exit 1
fi
echo "OK   skpt unsigned encrypted inbox rejection"

if [ -f "$OWNER_TOKEN_FILE" ]; then
  owner_status="$(
    curl -sS -o /dev/null -w "%{http_code}" --max-time 20 \
      -H "Authorization: Bearer $(cat "$OWNER_TOKEN_FILE")" \
      "https://social.skpt.cl/api/dais/owner/profile"
  )"
  if [ "$owner_status" != "200" ]; then
    echo "FAIL skpt owner token auth: expected HTTP 200, got $owner_status" >&2
    exit 1
  fi
  echo "OK   skpt owner token auth"
else
  if [ "$REQUIRE_FULL" = "1" ]; then
    echo "FAIL skpt owner token auth: $OWNER_TOKEN_FILE not found" >&2
    exit 1
  fi
  echo "SKIP skpt owner token auth: $OWNER_TOKEN_FILE not found"
fi

check_contains \
  "dais.social homepage mentions skpt testbed" \
  "https://dais.social/" \
  "Independent skpt.cl dais instance"
