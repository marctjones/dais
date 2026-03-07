# Phase 1 Deployment Guide

## Status: ✅ CODE COMPLETE - READY TO DEPLOY

All Phase 1 code is complete and workers are built. Follow these steps to go live!

---

## Quick Start Deployment

```bash
# 1. Deploy all workers to Cloudflare
./scripts/deploy.sh

# 2. Configure DNS (see DNS_SETUP.md for details)
# - Add CNAME: dais.social → webfinger worker URL
# - Add CNAME: social.dais.social → actor worker URL

# 3. Verify deployment
./scripts/verify-deployment.sh

# 4. Test from Mastodon
# Search for: @marc@dais.social
```

---

## Phase 1 Complete Feature List

### ✅ Federation Core
- **WebFinger Discovery**: Enables `@marc@dais.social` lookups
- **Actor Profile**: Returns ActivityPub Person object with public key
- **Inbox Endpoint**: Receives and processes Follow activities
- **HTTP Signatures**: Full signature generation and verification
- **D1 Database**: Complete ActivityPub schema (actors, followers, posts, activities)

### ✅ Follower Management
- **Follow Request Storage**: All requests stored in D1
- **Approval CLI**: `dais followers approve <url> --remote`
- **Rejection CLI**: `dais followers reject <url> --remote`
- **Accept/Reject Activities**: Automatically sent to remote instances
- **List Followers**: `dais followers list --status <pending|accepted|rejected>`

### ✅ Security
- **HTTP Signature Verification**: Validates incoming requests
- **Public Key Fetching**: Retrieves remote actor keys with caching
- **Signature Validation**: Verifies request authenticity

### ✅ Infrastructure
- **Rust Workers**: Compiled to WASM, optimized for size
- **Custom Domains**: Configured in wrangler.toml
- **Production Ready**: All workers built and tested

---

## Deployment Scripts

### `./scripts/deploy.sh`
Deploys all three workers to Cloudflare Workers:
1. Checks wrangler authentication
2. Deploys webfinger worker
3. Deploys actor worker
4. Deploys inbox worker
5. Provides next steps for DNS configuration

### `./scripts/verify-deployment.sh`
Verifies that all endpoints are working:
1. Tests WebFinger endpoint
2. Tests Actor endpoint
3. Tests Inbox endpoint (expects 405 for GET)
4. Checks DNS resolution
5. Reports any errors found

---

## Manual Deployment Steps

If you prefer to deploy manually:

### 1. Deploy WebFinger Worker
```bash
cd workers/webfinger
wrangler deploy --env production
```

Expected output:
```
✨ Successfully published your script to
 https://webfinger-production.<account>.workers.dev
```

### 2. Deploy Actor Worker
```bash
cd workers/actor
wrangler deploy --env production
```

Expected output:
```
✨ Successfully published your script to
 https://actor-production.<account>.workers.dev
```

### 3. Deploy Inbox Worker
```bash
cd workers/inbox
wrangler deploy --env production
```

Expected output:
```
✨ Successfully published your script to
 https://inbox-production.<account>.workers.dev
```

---

## DNS Configuration

See **[DNS_SETUP.md](./DNS_SETUP.md)** for complete instructions.

### Quick Summary:
1. Go to Cloudflare Dashboard → `dais.social` → DNS
2. Add CNAME records:
   - `@` → webfinger worker URL (Proxied)
   - `social` → actor worker URL (Proxied)
3. Wait 1-2 minutes for propagation
4. Run `./scripts/verify-deployment.sh`

---

## Testing Federation

### From Mastodon/Pleroma:
1. Search for: `@marc@dais.social`
2. Click "Follow"
3. Wait for follow request to arrive

### On Your Server:
```bash
# List pending follow requests
dais followers list --status pending --remote

# Approve a follower
dais followers approve https://mastodon.social/users/someone --remote

# Check accepted followers
dais followers list --status accepted
```

### Manual Testing:
```bash
# Test WebFinger
curl "https://dais.social/.well-known/webfinger?resource=acct:marc@dais.social" | jq

# Test Actor
curl -H "Accept: application/activity+json" "https://social.dais.social/users/marc" | jq

# Test Inbox (should return 405)
curl -i "https://social.dais.social/users/marc/inbox"
```

---

## Monitoring

### View Worker Logs
```bash
# WebFinger worker
wrangler tail webfinger --env production

# Actor worker
wrangler tail actor --env production

# Inbox worker
wrangler tail inbox --env production
```

### Check Database
```bash
# List all followers
dais followers list

# View database directly
wrangler d1 execute DB --env production --command "SELECT * FROM followers"
```

---

## Troubleshooting

### WebFinger Not Found
- Verify DNS: `dig dais.social`
- Check worker logs: `wrangler tail webfinger --env production`
- Test worker directly: `curl https://webfinger-production.<account>.workers.dev/.well-known/webfinger?resource=acct:marc@dais.social`

### Follow Requests Not Arriving
- Check inbox logs: `wrangler tail inbox --env production`
- Verify HTTP signature verification is working
- Check D1 database: `dais followers list --status pending`

### Accept/Reject Not Working
- Verify CLI is installed: `dais --version`
- Check activity delivery in logs
- Ensure remote actor URL is correct

---

## What's Next? Phase 2

Once Phase 1 is deployed and working:

1. **Outbox Implementation**
   - Serve list of public posts
   - Enable timeline fetching

2. **Post Publishing**
   - `dais post create "content"` CLI command
   - Create and distribute Create activities
   - Store posts in D1

3. **Media Support**
   - Upload images to Cloudflare R2
   - Attach media to posts
   - Serve media with proper headers

See `PHASE2_PLAN.md` (to be created) for detailed roadmap.

---

## Cost Estimate

### Cloudflare Free Tier Limits:
- **Workers**: 100,000 requests/day (✅ plenty for single user)
- **D1 Database**: 5 GB storage (✅ sufficient)
- **R2 Storage**: 10 GB storage (✅ for media)

**Estimated cost: $0/month** for typical single-user usage!

---

## Support

If you encounter issues:
1. Check logs with `wrangler tail`
2. Review [DEPLOYMENT.md](./DEPLOYMENT.md)
3. See [DNS_SETUP.md](./DNS_SETUP.md)
4. Check [CONTRIBUTING.md](./CONTRIBUTING.md) for dev setup

**Phase 1 is ready to go live! 🚀**
