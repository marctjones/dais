# DNS Configuration Guide

## Overview

To complete Phase 1 and enable federation, you need to configure custom domains for your Cloudflare Workers.

## Prerequisites

1. Workers deployed to Cloudflare (run `./scripts/deploy.sh`)
2. Access to Cloudflare dashboard
3. Domain `dais.social` added to your Cloudflare account

## Step 1: Get Worker URLs

After deploying workers, note their `*.workers.dev` URLs:

```bash
# The deployment script will output URLs like:
# https://webfinger-production.<your-account>.workers.dev
# https://actor-production.<your-account>.workers.dev
# https://inbox-production.<your-account>.workers.dev
```

Or check in Cloudflare dashboard: Workers & Pages → Select worker → Settings → Domains & Routes

## Step 2: Configure DNS Records

### Option A: Using Cloudflare Dashboard

1. Go to **Cloudflare Dashboard** → Select `dais.social` domain
2. Navigate to **DNS** → **Records**
3. Add the following CNAME records:

| Type | Name | Target | Proxy Status |
|------|------|--------|--------------|
| CNAME | @ | webfinger-production.`<account>`.workers.dev | Proxied (orange cloud) |
| CNAME | social | actor-production.`<account>`.workers.dev | Proxied (orange cloud) |

**Note:** The Inbox worker shares the same domain as Actor (`social.dais.social`)

### Option B: Using Wrangler CLI

Alternatively, add custom domains directly via wrangler (already configured in `wrangler.toml`):

```bash
cd workers/webfinger
wrangler domains add dais.social --env production

cd ../actor
wrangler domains add social.dais.social --env production

cd ../inbox
# Inbox uses same domain as actor - routes are differentiated by path
```

## Step 3: Verify Custom Domain Routes

The `wrangler.toml` files already include route patterns:

### WebFinger (`dais.social`)
```toml
routes = [
    { pattern = "dais.social/.well-known/webfinger", custom_domain = true }
]
```

### Actor (`social.dais.social`)
```toml
routes = [
    { pattern = "social.dais.social/users/*", custom_domain = true },
    { pattern = "social.dais.social/.well-known/*", custom_domain = true }
]
```

### Inbox (`social.dais.social`)
```toml
routes = [
    { pattern = "social.dais.social/users/*/inbox", custom_domain = true }
]
```

## Step 4: Test DNS Configuration

Wait 1-2 minutes for DNS propagation, then test:

```bash
# Test WebFinger endpoint
curl -i "https://dais.social/.well-known/webfinger?resource=acct:marc@dais.social"

# Test Actor endpoint
curl -i "https://social.dais.social/users/marc"

# Test Inbox endpoint (requires POST with signature)
curl -i "https://social.dais.social/users/marc/inbox"
```

Expected responses:
- **WebFinger**: JSON with `subject`, `links`
- **Actor**: JSON with `@context`, `type: "Person"`
- **Inbox**: `405 Method Not Allowed` (for GET request)

## Step 5: Test Federation

From a Mastodon or Pleroma instance:

1. Search for: `@marc@dais.social`
2. Click "Follow"
3. Check follow request on your server:

```bash
dais followers list --status pending --remote
```

4. Approve the follower:

```bash
dais followers approve <actor-url> --remote
```

## Troubleshooting

### DNS not resolving

```bash
# Check DNS propagation
dig dais.social
dig social.dais.social

# Check CNAME records
dig CNAME dais.social
dig CNAME social.dais.social
```

### 522 or 524 errors

- Ensure "Proxy status" is enabled (orange cloud) in Cloudflare DNS
- Verify workers are deployed and active

### WebFinger not found

- Confirm route pattern matches exactly: `dais.social/.well-known/webfinger`
- Check worker logs: `wrangler tail webfinger --env production`

### Follow requests not received

- Verify HTTP signature verification in inbox worker
- Check inbox worker logs: `wrangler tail inbox --env production`
- Ensure D1 database is bound correctly in `wrangler.toml`

## Next Steps

Once DNS is configured and federation is working:

1. **Phase 2**: Implement Outbox and post publishing
2. **Phase 3**: Add interactions (replies, likes, boosts)
3. **Web UI**: Build simple web interface for viewing profile

---

**Status**: DNS configuration pending - workers ready to deploy!
