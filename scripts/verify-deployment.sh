#!/bin/bash

echo "🔍 Verifying dais.social deployment..."
echo ""

# Color codes
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

ERRORS=0

# Test WebFinger
echo -e "${BLUE}Testing WebFinger endpoint...${NC}"
WEBFINGER_URL="https://dais.social/.well-known/webfinger?resource=acct:marc@dais.social"
WEBFINGER_RESPONSE=$(curl -s -w "\n%{http_code}" "$WEBFINGER_URL" 2>&1)
WEBFINGER_HTTP_CODE=$(echo "$WEBFINGER_RESPONSE" | tail -n1)
WEBFINGER_BODY=$(echo "$WEBFINGER_RESPONSE" | sed '$d')

if [ "$WEBFINGER_HTTP_CODE" = "200" ]; then
    if echo "$WEBFINGER_BODY" | jq -e '.subject' &> /dev/null; then
        echo -e "${GREEN}✅ WebFinger: OK (HTTP $WEBFINGER_HTTP_CODE)${NC}"
        echo "   Subject: $(echo "$WEBFINGER_BODY" | jq -r '.subject')"
    else
        echo -e "${RED}❌ WebFinger: Invalid JSON response${NC}"
        ERRORS=$((ERRORS + 1))
    fi
else
    echo -e "${RED}❌ WebFinger: Failed (HTTP $WEBFINGER_HTTP_CODE)${NC}"
    ERRORS=$((ERRORS + 1))
fi
echo ""

# Test Actor
echo -e "${BLUE}Testing Actor endpoint...${NC}"
ACTOR_URL="https://social.dais.social/users/marc"
ACTOR_RESPONSE=$(curl -s -w "\n%{http_code}" -H "Accept: application/activity+json" "$ACTOR_URL" 2>&1)
ACTOR_HTTP_CODE=$(echo "$ACTOR_RESPONSE" | tail -n1)
ACTOR_BODY=$(echo "$ACTOR_RESPONSE" | sed '$d')

if [ "$ACTOR_HTTP_CODE" = "200" ]; then
    if echo "$ACTOR_BODY" | jq -e '.type' &> /dev/null; then
        ACTOR_TYPE=$(echo "$ACTOR_BODY" | jq -r '.type')
        echo -e "${GREEN}✅ Actor: OK (HTTP $ACTOR_HTTP_CODE)${NC}"
        echo "   Type: $ACTOR_TYPE"
        echo "   ID: $(echo "$ACTOR_BODY" | jq -r '.id')"
    else
        echo -e "${RED}❌ Actor: Invalid JSON response${NC}"
        ERRORS=$((ERRORS + 1))
    fi
else
    echo -e "${RED}❌ Actor: Failed (HTTP $ACTOR_HTTP_CODE)${NC}"
    ERRORS=$((ERRORS + 1))
fi
echo ""

# Test Inbox (should reject GET requests)
echo -e "${BLUE}Testing Inbox endpoint...${NC}"
INBOX_URL="https://social.dais.social/users/marc/inbox"
INBOX_RESPONSE=$(curl -s -w "\n%{http_code}" "$INBOX_URL" 2>&1)
INBOX_HTTP_CODE=$(echo "$INBOX_RESPONSE" | tail -n1)

if [ "$INBOX_HTTP_CODE" = "405" ]; then
    echo -e "${GREEN}✅ Inbox: OK (correctly rejects GET with HTTP $INBOX_HTTP_CODE)${NC}"
elif [ "$INBOX_HTTP_CODE" = "401" ] || [ "$INBOX_HTTP_CODE" = "403" ]; then
    echo -e "${GREEN}✅ Inbox: OK (requires authentication, HTTP $INBOX_HTTP_CODE)${NC}"
else
    echo -e "${YELLOW}⚠️  Inbox: Unexpected response (HTTP $INBOX_HTTP_CODE)${NC}"
    echo "   Note: Inbox should return 405 for GET or 401/403 for missing signature"
fi
echo ""

# Check DNS resolution
echo -e "${BLUE}Checking DNS configuration...${NC}"
WEBFINGER_DNS=$(dig +short dais.social | head -n1)
ACTOR_DNS=$(dig +short social.dais.social | head -n1)

if [ -n "$WEBFINGER_DNS" ]; then
    echo -e "${GREEN}✅ dais.social resolves to: $WEBFINGER_DNS${NC}"
else
    echo -e "${RED}❌ dais.social does not resolve${NC}"
    ERRORS=$((ERRORS + 1))
fi

if [ -n "$ACTOR_DNS" ]; then
    echo -e "${GREEN}✅ social.dais.social resolves to: $ACTOR_DNS${NC}"
else
    echo -e "${RED}❌ social.dais.social does not resolve${NC}"
    ERRORS=$((ERRORS + 1))
fi
echo ""

# Summary
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if [ $ERRORS -eq 0 ]; then
    echo -e "${GREEN}🎉 All checks passed! Phase 1 is LIVE!${NC}"
    echo ""
    echo "Next steps:"
    echo "  1. Test from Mastodon: search for @marc@dais.social"
    echo "  2. Check followers: dais followers list --status pending --remote"
    echo "  3. Approve followers: dais followers approve <actor-url> --remote"
else
    echo -e "${RED}❌ Found $ERRORS error(s)${NC}"
    echo ""
    echo "Troubleshooting:"
    echo "  - Run: wrangler tail <worker-name> --env production"
    echo "  - Check: ./scripts/deploy.sh output for errors"
    echo "  - Review: DNS_SETUP.md for configuration steps"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
