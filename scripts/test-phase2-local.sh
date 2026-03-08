#!/usr/bin/env bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Testing Phase 2: Content Publishing${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo ""

FAILED=0
PASSED=0

# Test 1: Outbox Collection
echo -e "${BLUE}Test 1: Outbox Collection${NC}"
echo -e "${BLUE}───────────────────────────────────────────────${NC}"
echo "curl -H 'Accept: application/activity+json' 'http://localhost:8790/users/marc/outbox'"
echo ""

RESPONSE=$(curl -s -H "Accept: application/activity+json" "http://localhost:8790/users/marc/outbox")

if echo "$RESPONSE" | jq -e '.type == "OrderedCollection"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Outbox has correct type (OrderedCollection)${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Outbox type incorrect${NC}"
    echo "Response: $RESPONSE"
    ((FAILED++))
fi

if echo "$RESPONSE" | jq -e '.id == "https://social.dais.social/users/marc/outbox"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Outbox has correct ID${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Outbox ID incorrect${NC}"
    ((FAILED++))
fi

if echo "$RESPONSE" | jq -e '.totalItems >= 0' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Outbox has totalItems field${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Outbox missing totalItems${NC}"
    ((FAILED++))
fi

echo ""

# Test 2: Individual Post
echo -e "${BLUE}Test 2: Individual Post (from seed data)${NC}"
echo -e "${BLUE}───────────────────────────────────────────────${NC}"
echo "curl -H 'Accept: application/activity+json' 'http://localhost:8790/users/marc/posts/001'"
echo ""

RESPONSE=$(curl -s -H "Accept: application/activity+json" "http://localhost:8790/users/marc/posts/001")

if echo "$RESPONSE" | jq -e '.type == "Note"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Post has correct type (Note)${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Post type incorrect${NC}"
    echo "Response: $RESPONSE"
    ((FAILED++))
fi

if echo "$RESPONSE" | jq -e '.attributedTo' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Post has attributedTo field${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Post missing attributedTo${NC}"
    ((FAILED++))
fi

if echo "$RESPONSE" | jq -e '.content' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Post has content${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Post missing content${NC}"
    ((FAILED++))
fi

echo ""

# Test 3: CLI Post Create
echo -e "${BLUE}Test 3: CLI Post Create${NC}"
echo -e "${BLUE}───────────────────────────────────────────────${NC}"
echo "dais post create \"Test post from integration test\""
echo ""

if dais post create "Test post from integration test" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ dais post create command works${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ dais post create command failed${NC}"
    ((FAILED++))
fi

echo ""

# Test 4: CLI Post List
echo -e "${BLUE}Test 4: CLI Post List${NC}"
echo -e "${BLUE}───────────────────────────────────────────────${NC}"
echo "dais post list"
echo ""

if dais post list > /dev/null 2>&1; then
    echo -e "${GREEN}✓ dais post list command works${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ dais post list command failed${NC}"
    ((FAILED++))
fi

# Verify the outbox now has at least 2 items (seed post + test post)
RESPONSE=$(curl -s -H "Accept: application/activity+json" "http://localhost:8790/users/marc/outbox")

TOTAL_ITEMS=$(echo "$RESPONSE" | jq -r '.totalItems')
if [ "$TOTAL_ITEMS" -ge 2 ]; then
    echo -e "${GREEN}✓ Outbox now contains $TOTAL_ITEMS posts (includes test post)${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Outbox has only $TOTAL_ITEMS posts (expected >= 2)${NC}"
    ((FAILED++))
fi

echo ""

# Test 5: Outbox Options (CORS)
echo -e "${BLUE}Test 5: Outbox CORS (OPTIONS)${NC}"
echo -e "${BLUE}───────────────────────────────────────────────${NC}"
echo "curl -X OPTIONS 'http://localhost:8790/users/marc/outbox'"
echo ""

STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X OPTIONS "http://localhost:8790/users/marc/outbox")

if [ "$STATUS" = "200" ]; then
    echo -e "${GREEN}✓ Outbox OPTIONS returns 200${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ Outbox OPTIONS failed (status: $STATUS)${NC}"
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
    echo -e "${GREEN}✓ All Phase 2 tests passed!${NC}"
    echo ""
    echo -e "${YELLOW}Note: Delete test not included (requires post ID from create)${NC}"
    echo -e "${YELLOW}To test delete: dais post list, then dais post delete <post-id>${NC}"
    exit 0
else
    echo -e "${RED}✗ Some tests failed${NC}"
    exit 1
fi
