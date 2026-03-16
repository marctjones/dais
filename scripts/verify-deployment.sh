#!/bin/bash
# Deployment verification script
# Tests endpoints to ensure all workers are responding correctly

set -e

# Configuration
DOMAIN=${DOMAIN:-"dais.social"}
ACTIVITYPUB_DOMAIN=${ACTIVITYPUB_DOMAIN:-"social.dais.social"}
USERNAME=${USERNAME:-"social"}

echo "========================================="
echo "Deployment Verification"
echo "========================================="
echo ""
echo "Domain: $DOMAIN"
echo "ActivityPub Domain: $ACTIVITYPUB_DOMAIN"
echo "Username: $USERNAME"
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

PASSED=0
FAILED=0

test_endpoint() {
    local name=$1
    local url=$2
    local expected_status=${3:-200}

    echo -n "Testing $name... "

    status=$(curl -s -o /dev/null -w "%{http_code}" "$url" 2>/dev/null || echo "000")

    if [ "$status" = "$expected_status" ]; then
        echo -e "${GREEN}✓ $status${NC}"
        ((PASSED++))
    else
        echo -e "${RED}✗ $status (expected $expected_status)${NC}"
        ((FAILED++))
    fi
}

echo "Testing WebFinger endpoint..."
test_endpoint "WebFinger" "https://$ACTIVITYPUB_DOMAIN/.well-known/webfinger?resource=acct:$USERNAME@$ACTIVITYPUB_DOMAIN"
echo ""

echo "Testing Actor endpoint..."
test_endpoint "Actor Profile" "https://$ACTIVITYPUB_DOMAIN/users/$USERNAME"
echo ""

echo "Testing Landing page..."
test_endpoint "Landing Page" "https://$DOMAIN/"
test_endpoint "Health Check" "https://$DOMAIN/health"
echo ""

echo "========================================="
echo "Verification Summary"
echo "========================================="
echo -e "Passed: ${GREEN}$PASSED${NC}"
echo -e "Failed: ${RED}$FAILED${NC}"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All endpoints verified successfully!${NC}"
    exit 0
else
    echo -e "${RED}Some endpoints failed verification${NC}"
    exit 1
fi
