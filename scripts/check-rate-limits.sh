#!/bin/bash
# Check if Cloudflare rate limits have cleared

echo "Checking rate limit status..."
echo ""

# Check ActivityPub endpoint
echo "1. Testing ActivityPub (social.dais.social):"
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" -H "Accept: application/activity+json" "https://social.dais.social/users/social")
if [ "$HTTP_CODE" = "200" ]; then
    echo "   ✓ ActivityPub OK (HTTP $HTTP_CODE)"
else
    echo "   ✗ Still rate limited (HTTP $HTTP_CODE)"
fi
echo ""

# Check PDS endpoint
echo "2. Testing PDS (pds.dais.social):"
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "https://pds.dais.social/xrpc/com.atproto.server.describeServer")
if [ "$HTTP_CODE" = "200" ]; then
    echo "   ✓ PDS OK (HTTP $HTTP_CODE)"
else
    echo "   ✗ Still rate limited (HTTP $HTTP_CODE)"
fi
echo ""

# Check D1 database access
echo "3. Testing D1 database:"
cd /home/marc/Projects/dais/workers/pds
OUTPUT=$(wrangler d1 execute dais-social --command "SELECT COUNT(*) as count FROM posts" 2>&1)
if echo "$OUTPUT" | grep -q "count"; then
    echo "   ✓ D1 OK"
else
    echo "   ✗ Still rate limited"
    echo "   Error: $OUTPUT"
fi
echo ""

echo "Run this script periodically to check when limits clear."
