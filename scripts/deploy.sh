#!/bin/bash
set -e

echo "🚀 Deploying dais.social ActivityPub Server - Phase 1"
echo ""

# Color codes
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check prerequisites
echo -e "${BLUE}Checking prerequisites...${NC}"
if ! command -v wrangler &> /dev/null; then
    echo "❌ wrangler not found. Install with: npm install -g wrangler"
    exit 1
fi
echo -e "${GREEN}✅ wrangler found: $(wrangler --version)${NC}"

# Check if logged in to Cloudflare
if ! wrangler whoami &> /dev/null; then
    echo -e "${YELLOW}⚠️  Not logged in to Cloudflare. Running: wrangler login${NC}"
    wrangler login
fi
echo -e "${GREEN}✅ Authenticated with Cloudflare${NC}"
echo ""

# Deploy WebFinger worker
echo -e "${BLUE}📦 Deploying WebFinger worker...${NC}"
cd /home/marc/Projects/dais/workers/webfinger
wrangler deploy --env production
echo -e "${GREEN}✅ WebFinger deployed${NC}"
echo ""

# Deploy Actor worker
echo -e "${BLUE}📦 Deploying Actor worker...${NC}"
cd /home/marc/Projects/dais/workers/actor
wrangler deploy --env production
echo -e "${GREEN}✅ Actor deployed${NC}"
echo ""

# Deploy Inbox worker
echo -e "${BLUE}📦 Deploying Inbox worker...${NC}"
cd /home/marc/Projects/dais/workers/inbox
wrangler deploy --env production
echo -e "${GREEN}✅ Inbox deployed${NC}"
echo ""

cd /home/marc/Projects/dais

echo ""
echo -e "${GREEN}🎉 All workers deployed successfully!${NC}"
echo ""
echo "Next steps:"
echo ""
echo "1. Configure DNS records in Cloudflare dashboard:"
echo "   CNAME: dais.social → <webfinger-worker-url>"
echo "   CNAME: social → <actor-worker-url>"
echo ""
echo "2. Test WebFinger:"
echo "   curl 'https://dais.social/.well-known/webfinger?resource=acct:marc@dais.social'"
echo ""
echo "3. Test from Mastodon:"
echo "   Search for: @marc@dais.social"
echo ""
echo "4. Manage followers:"
echo "   dais followers list --status pending --remote"
echo "   dais followers approve <actor-url> --remote"
echo ""
