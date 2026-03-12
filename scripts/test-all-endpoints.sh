#!/bin/bash
# Comprehensive endpoint testing for dais dual-protocol server

set -e

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

PASS=0
FAIL=0
WARN=0

function test_endpoint() {
    local name="$1"
    local url="$2"
    local expected_status="${3:-200}"
    local headers="${4:-}"

    echo -n "Testing $name... "

    if [ -n "$headers" ]; then
        response=$(curl -s -w "\n%{http_code}" -H "$headers" "$url")
    else
        response=$(curl -s -w "\n%{http_code}" "$url")
    fi

    status=$(echo "$response" | tail -n1)
    body=$(echo "$response" | head -n-1)

    if [ "$status" = "$expected_status" ]; then
        echo -e "${GREEN}✓ PASS${NC} (HTTP $status)"
        ((PASS++))
        return 0
    else
        echo -e "${RED}✗ FAIL${NC} (Expected $expected_status, got $status)"
        echo "Response: $body" | head -5
        ((FAIL++))
        return 1
    fi
}

function test_json_field() {
    local name="$1"
    local url="$2"
    local field="$3"
    local headers="${4:-}"

    echo -n "Testing $name... "

    if [ -n "$headers" ]; then
        response=$(curl -s -H "$headers" "$url")
    else
        response=$(curl -s "$url")
    fi

    value=$(echo "$response" | jq -r "$field" 2>/dev/null)

    if [ "$value" != "null" ] && [ -n "$value" ]; then
        echo -e "${GREEN}✓ PASS${NC} ($field = $value)"
        ((PASS++))
        return 0
    else
        echo -e "${RED}✗ FAIL${NC} (Field $field not found or null)"
        echo "Response: $response" | head -5
        ((FAIL++))
        return 1
    fi
}

echo "========================================"
echo "Dual-Protocol Server Endpoint Tests"
echo "========================================"
echo ""

echo "--- ActivityPub Tests ---"
test_json_field "Actor endpoint" \
    "https://social.dais.social/users/social" \
    ".type" \
    "Accept: application/activity+json"

test_json_field "Actor has publicKey" \
    "https://social.dais.social/users/social" \
    ".publicKey.publicKeyPem" \
    "Accept: application/activity+json"

test_json_field "Outbox endpoint" \
    "https://social.dais.social/users/social/outbox" \
    ".type" \
    "Accept: application/activity+json"

test_json_field "Outbox has totalItems" \
    "https://social.dais.social/users/social/outbox" \
    ".totalItems" \
    "Accept: application/activity+json"

test_json_field "Followers collection" \
    "https://social.dais.social/users/social/followers" \
    ".type" \
    "Accept: application/activity+json"

test_json_field "Followers pagination" \
    "https://social.dais.social/users/social/followers?page=1" \
    ".type" \
    "Accept: application/activity+json"

test_json_field "Following collection" \
    "https://social.dais.social/users/social/following" \
    ".type" \
    "Accept: application/activity+json"

echo ""
echo "--- AT Protocol Tests ---"
test_json_field "PDS describeServer" \
    "https://pds.dais.social/xrpc/com.atproto.server.describeServer" \
    ".did"

test_json_field "PDS DID document" \
    "https://pds.dais.social/.well-known/did.json" \
    ".id"

test_json_field "Sync listRepos" \
    "https://pds.dais.social/xrpc/com.atproto.sync.listRepos" \
    ".repos"

test_json_field "Sync getRepoStatus" \
    "https://pds.dais.social/xrpc/com.atproto.sync.getRepoStatus?did=did:web:social.dais.social" \
    ".did"

test_endpoint "Sync subscribeRepos (non-WebSocket)" \
    "https://pds.dais.social/xrpc/com.atproto.sync.subscribeRepos" \
    200

echo ""
echo "--- Webfinger Tests ---"
test_json_field "Webfinger lookup" \
    "https://social.dais.social/.well-known/webfinger?resource=acct:social@dais.social" \
    ".subject"

echo ""
echo "--- Cross-Protocol Tests ---"
# Get latest post and check for atproto_uri
echo -n "Testing cross-protocol metadata... "
latest_post=$(curl -s -H "Accept: application/activity+json" "https://social.dais.social/users/social/outbox" | jq -r '.orderedItems[0]')
if echo "$latest_post" | jq -e '.atproto_uri' > /dev/null 2>&1; then
    atproto_uri=$(echo "$latest_post" | jq -r '.atproto_uri')
    echo -e "${GREEN}✓ PASS${NC} (atproto_uri: $atproto_uri)"
    ((PASS++))
else
    echo -e "${YELLOW}⚠ WARN${NC} (No posts with atproto_uri found - may be expected if no dual-protocol posts exist)"
    ((WARN++))
fi

echo ""
echo "========================================"
echo "Test Summary"
echo "========================================"
echo -e "${GREEN}Passed: $PASS${NC}"
echo -e "${RED}Failed: $FAIL${NC}"
echo -e "${YELLOW}Warnings: $WARN${NC}"
echo ""

if [ $FAIL -eq 0 ]; then
    echo -e "${GREEN}✓ All critical tests passed!${NC}"
    exit 0
else
    echo -e "${RED}✗ Some tests failed${NC}"
    exit 1
fi
