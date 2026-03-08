#!/usr/bin/env bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Testing Phase 1: Basic Federation${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo ""

FAILED=0
PASSED=0

# Test 1: WebFinger Discovery
echo -e "${BLUE}Test 1: WebFinger Discovery${NC}"
echo -e "${BLUE}───────────────────────────────────────────────${NC}"
echo "curl 'http://localhost:8787/.well-known/webfinger?resource=acct:marc@localhost'"
echo ""

RESPONSE=$(curl -s "http://localhost:8787/.well-known/webfinger?resource=acct:marc@localhost")

if echo "$RESPONSE" | jq -e '.subject == "acct:marc@localhost"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ WebFinger returned correct subject${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ WebFinger subject incorrect${NC}"
    echo "Response: $RESPONSE"
    ((FAILED++))
fi

if echo "$RESPONSE" | jq -e '.links[0].type == "application/activity+json"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ WebFinger has ActivityPub link${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ WebFinger missing ActivityPub link${NC}"
    ((FAILED++))
fi

echo ""

# Test 2: Actor Profile
echo -e "${BLUE}Test 2: Actor Profile${NC}"
echo -e "${BLUE}───────────────────────────────────────────────${NC}"
echo "curl -H 'Accept: application/activity+json' 'http://localhost:8788/users/marc'"
echo ""

RESPONSE=$(curl -s -H "Accept: application/activity+json" "http://localhost:8788/users/marc")

if echo "$RESPONSE" | jq -e '.type == "Person"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Actor has correct type (Person)${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Actor type incorrect${NC}"
    echo "Response: $RESPONSE"
    ((FAILED++))
fi

if echo "$RESPONSE" | jq -e '.id == "https://social.dais.social/users/marc"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Actor has correct ID${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Actor ID incorrect${NC}"
    ((FAILED++))
fi

if echo "$RESPONSE" | jq -e '.publicKey.id' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Actor has public key${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Actor missing public key${NC}"
    ((FAILED++))
fi

if echo "$RESPONSE" | jq -e '.inbox == "https://social.dais.social/users/marc/inbox"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Actor has inbox URL${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Actor inbox URL incorrect${NC}"
    ((FAILED++))
fi

if echo "$RESPONSE" | jq -e '.outbox == "https://social.dais.social/users/marc/outbox"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Actor has outbox URL${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Actor outbox URL incorrect${NC}"
    ((FAILED++))
fi

echo ""

# Test 3: Inbox Endpoint (OPTIONS)
echo -e "${BLUE}Test 3: Inbox Endpoint (OPTIONS)${NC}"
echo -e "${BLUE}───────────────────────────────────────────────${NC}"
echo "curl -X OPTIONS 'http://localhost:8789/users/marc/inbox'"
echo ""

STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X OPTIONS "http://localhost:8789/users/marc/inbox")

if [ "$STATUS" = "200" ]; then
    echo -e "${GREEN}✓ Inbox OPTIONS returns 200${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Inbox OPTIONS failed (status: $STATUS)${NC}"
    ((FAILED++))
fi

echo ""

# Test 4: CLI Stats Command
echo -e "${BLUE}Test 4: CLI Stats Command${NC}"
echo -e "${BLUE}───────────────────────────────────────────────${NC}"
echo "dais stats"
echo ""

if dais stats > /dev/null 2>&1; then
    echo -e "${GREEN}✓ dais stats command works${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ dais stats command failed${NC}"
    ((FAILED++))
fi

echo ""

# Test 5: CLI Followers List
echo -e "${BLUE}Test 5: CLI Followers List${NC}"
echo -e "${BLUE}───────────────────────────────────────────────${NC}"
echo "dais followers list"
echo ""

if dais followers list > /dev/null 2>&1; then
    echo -e "${GREEN}✓ dais followers list command works${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ dais followers list command failed${NC}"
    ((FAILED++))
fi

echo ""

# Summary
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Test Summary${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo ""
echo -e "${GREEN}Passed: $PASSED${NC}"
echo -e "${RED}Failed: $FAILED${NC}"
echo ""

if [ "$FAILED" -eq 0 ]; then
    echo -e "${GREEN}✓ All Phase 1 tests passed!${NC}"
    exit 0
else
    echo -e "${RED}✗ Some tests failed${NC}"
    exit 1
fi
