# dais v1.1 Deployment Guide

**Date**: March 15, 2026
**Version**: v1.1.0

This guide walks you through deploying dais v1.1 from scratch on Cloudflare Workers.

## Overview

**Deployment time**: 30-45 minutes
**Cost**: Cloudflare Workers Free tier ($0/month for most single-user instances)

**What you'll deploy**:
- 9 Cloudflare Workers (WebFinger, Actor, Inbox, Outbox, etc.)
- 1 D1 Database (SQLite)
- 1 R2 Bucket (media storage)
- 1 Queue (delivery retries)

## Prerequisites

### Required

- [x] **Domain name** - You own a domain (e.g., `example.com`)
- [x] **Cloudflare account** - Free tier is sufficient
- [x] **Domain on Cloudflare** - Add your domain to Cloudflare DNS
- [x] **Git** - For cloning the repository
- [x] **Rust** (1.75+) - Install from https://rustup.rs
- [x] **Node.js** (18+) - For wrangler CLI
- [x] **wrangler CLI** (3.0+) - Cloudflare's deployment tool

### Optional

- [ ] **Bluesky account** - For AT Protocol / PDS integration
- [ ] **Admin interface** - For web-based post creation
- [ ] **Custom theme** - For personalized appearance

## Step 1: Install Prerequisites

### Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustup target add wasm32-unknown-unknown
```

Verify:
```bash
rustc --version  # Should be 1.75+
```

### Install Node.js

**Ubuntu/Debian**:
```bash
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs
```

**macOS**:
```bash
brew install node
```

**Windows**:
Download from https://nodejs.org

Verify:
```bash
node --version  # Should be 18+
```

### Install wrangler CLI

```bash
npm install -g wrangler
wrangler --version  # Should be 3.0+
```

### Install worker-build

```bash
cargo install worker-build
```

## Step 2: Setup Cloudflare

### Create Cloudflare Account

1. Go to https://dash.cloudflare.com/sign-up
2. Create free account
3. Verify email

### Add Domain to Cloudflare

1. In Cloudflare Dashboard, click "Add a site"
2. Enter your domain (e.g., `example.com`)
3. Select "Free" plan
4. Follow instructions to change nameservers at your domain registrar
5. Wait for DNS to propagate (~10 minutes to 24 hours)

### Authenticate wrangler

```bash
wrangler login
```

This opens a browser to authorize wrangler with your Cloudflare account.

Verify:
```bash
wrangler whoami
```

## Step 3: Clone and Configure

### Clone Repository

```bash
git clone https://github.com/daisocial/dais.git
cd dais
git checkout release/v1.1
```

### Choose Your Domains

You need 3 subdomains:

1. **Main domain** - Landing page (e.g., `dais.example.com`)
2. **ActivityPub domain** - Federation endpoint (e.g., `social.dais.example.com`)
3. **PDS domain** - Bluesky/AT Protocol (e.g., `pds.dais.example.com`)

**Example**:
- Main: `dais.social`
- ActivityPub: `social.dais.social`
- PDS: `pds.dais.social`

### Generate Cryptographic Keys

```bash
# Create keys directory
mkdir -p ~/.dais/keys

# Generate RSA key pair for ActivityPub HTTP signatures
openssl genrsa -out ~/.dais/keys/private.pem 2048
openssl rsa -in ~/.dais/keys/private.pem -pubout -out ~/.dais/keys/public.pem
```

## Step 4: Create Cloudflare Resources

### Create D1 Database

```bash
wrangler d1 create dais-db
```

Save the database ID from the output:
```
✅ Successfully created DB 'dais-db'
   database_id = "f90f9da8-136c-40c6-b96a-eba38d7efa65"
```

### Create R2 Bucket

```bash
wrangler r2 bucket create dais-media
```

### Create Queue

```bash
wrangler queues create delivery
```

## Step 5: Configure Workers

### Update wrangler.toml Files

For each worker in `platforms/cloudflare/workers/*/wrangler.toml`, update:

**Example** (`platforms/cloudflare/workers/actor/wrangler.toml`):

```toml
name = "actor"
main = "build/worker/shim.mjs"
compatibility_date = "2025-01-04"

[vars]
THEME = "cat-light"

[build]
command = "cargo install -q worker-build && worker-build --release"

[env.production]
name = "actor-production"

[env.production.vars]
DOMAIN = "dais.example.com"                      # ← YOUR MAIN DOMAIN
ACTIVITYPUB_DOMAIN = "social.dais.example.com"   # ← YOUR ACTIVITYPUB DOMAIN
PDS_DOMAIN = "pds.dais.example.com"              # ← YOUR PDS DOMAIN (optional)
THEME = "cat-light"
USERNAME = "yourusername"                         # ← YOUR USERNAME

[[env.production.d1_databases]]
binding = "DB"
database_name = "dais-db"
database_id = "f90f9da8-136c-40c6-b96a-eba38d7efa65"  # ← YOUR DATABASE ID

[[env.production.r2_buckets]]
binding = "MEDIA"
bucket_name = "dais-media"

[[env.production.queues.producers]]
queue = "delivery"
binding = "DELIVERY_QUEUE"
```

**Repeat for all 9 workers**:
- `actor`
- `inbox`
- `outbox`
- `webfinger`
- `delivery-queue`
- `auth`
- `pds`
- `router`
- `landing`

### Helper Script for Configuration

Create `update-config.sh`:

```bash
#!/bin/bash
# update-config.sh

# Your configuration
DOMAIN="dais.example.com"
ACTIVITYPUB_DOMAIN="social.dais.example.com"
PDS_DOMAIN="pds.dais.example.com"
DATABASE_ID="f90f9da8-136c-40c6-b96a-eba38d7efa65"  # From step 4
USERNAME="yourusername"

# Update all workers
for worker in actor inbox outbox webfinger delivery-queue auth pds router landing; do
    CONFIG="platforms/cloudflare/workers/$worker/wrangler.toml"
    echo "Updating $worker..."

    sed -i "s/DOMAIN = \".*\"/DOMAIN = \"$DOMAIN\"/" "$CONFIG"
    sed -i "s/ACTIVITYPUB_DOMAIN = \".*\"/ACTIVITYPUB_DOMAIN = \"$ACTIVITYPUB_DOMAIN\"/" "$CONFIG"
    sed -i "s/PDS_DOMAIN = \".*\"/PDS_DOMAIN = \"$PDS_DOMAIN\"/" "$CONFIG"
    sed -i "s/database_id = \".*\"/database_id = \"$DATABASE_ID\"/" "$CONFIG"
    sed -i "s/USERNAME = \".*\"/USERNAME = \"$USERNAME\"/" "$CONFIG"
done

echo "Configuration updated!"
```

Make executable and run:
```bash
chmod +x update-config.sh
./update-config.sh
```

## Step 6: Setup Database Schema

### Apply Migrations

```bash
# Create tables
wrangler d1 execute dais-db --file=cli/migrations/001_initial_schema.sql --remote

# Create indexes
wrangler d1 execute dais-db --file=cli/migrations/002_indexes.sql --remote

# Add AT Protocol tables (if using PDS)
wrangler d1 execute dais-db --file=cli/migrations/003_at_protocol.sql --remote
```

### Verify Schema

```bash
wrangler d1 execute dais-db --command "SELECT name FROM sqlite_master WHERE type='table'" --remote
```

Expected tables:
- `users`
- `posts`
- `followers`
- `following`
- `activities`
- `notifications`
- `at_repos` (if PDS enabled)
- `at_records` (if PDS enabled)

### Seed Initial User

```bash
# Create your user account
wrangler d1 execute dais-db --command "
INSERT INTO users (username, email, display_name, summary, created_at)
VALUES ('yourusername', 'you@example.com', 'Your Name', 'Your bio', datetime('now'))
" --remote
```

## Step 7: Upload Secrets

### Upload Private Key

For workers that need HTTP signature signing (inbox, outbox, delivery-queue):

```bash
# Actor worker
cd platforms/cloudflare/workers/actor
wrangler secret put PRIVATE_KEY --env production < ~/.dais/keys/private.pem

# Inbox worker
cd ../inbox
wrangler secret put PRIVATE_KEY --env production < ~/.dais/keys/private.pem

# Outbox worker
cd ../outbox
wrangler secret put PRIVATE_KEY --env production < ~/.dais/keys/private.pem

# Delivery queue worker
cd ../delivery-queue
wrangler secret put PRIVATE_KEY --env production < ~/.dais/keys/private.pem

cd ../../../../
```

### Upload Admin Password (Optional)

If using the auth worker for admin interface:

```bash
cd platforms/cloudflare/workers/auth

# Generate secure password
ADMIN_PASSWORD=$(openssl rand -base64 32)
echo "Admin password: $ADMIN_PASSWORD"  # Save this!

# Upload as secret
echo -n "$ADMIN_PASSWORD" | wrangler secret put ADMIN_PASSWORD --env production

cd ../../../../
```

## Step 8: Build and Deploy Workers

### Test Compilation First

```bash
./scripts/test-workers.sh
```

Expected output:
```
Testing dais-core library... ✓ PASS
Testing dais-cloudflare bindings... ✓ PASS
Testing actor worker... ✓ PASS
Testing inbox worker... ✓ PASS
Testing outbox worker... ✓ PASS
Testing webfinger worker... ✓ PASS
Testing delivery-queue worker... ✓ PASS
Testing auth worker... ✓ PASS
Testing pds worker... ✓ PASS
Testing router worker... ✓ PASS
Testing landing worker... ✓ PASS

Total: 11/11 components compiled successfully
```

### Deploy Workers

```bash
# Deploy all workers
for worker in landing webfinger actor auth pds outbox inbox delivery-queue router; do
    echo "Deploying $worker..."
    cd platforms/cloudflare/workers/$worker
    wrangler deploy --env production
    cd ../../../../
done
```

This takes ~5-10 minutes. Each worker is deployed to Cloudflare's global network.

## Step 9: Configure DNS

### Add Worker Routes

In Cloudflare Dashboard:

1. Go to **Workers & Pages**
2. Click on each worker
3. Go to **Settings** → **Triggers** → **Custom Domains**
4. Add custom domain

**Domains to add**:

| Worker | Custom Domain | Purpose |
|--------|---------------|---------|
| `landing` | `dais.example.com` | Main landing page |
| `router` | `social.dais.example.com` | ActivityPub router |
| `pds` | `pds.dais.example.com` | AT Protocol / Bluesky |

The router automatically routes:
- `/.well-known/webfinger` → WebFinger worker
- `/users/*` → Actor worker
- `/users/*/inbox` → Inbox worker
- `/users/*/outbox` → Outbox worker

## Step 10: Verify Deployment

### Automated Verification

```bash
export DOMAIN="dais.example.com"
export ACTIVITYPUB_DOMAIN="social.dais.example.com"
export USERNAME="yourusername"

./scripts/verify-deployment.sh
```

Expected output:
```
Testing WebFinger endpoint...
Testing WebFinger... ✓ 200

Testing Actor endpoint...
Testing Actor Profile... ✓ 200

Testing Landing page...
Testing Landing Page... ✓ 200
Testing Health Check... ✓ 200

Verification Summary
Passed: 4
Failed: 0

All endpoints verified successfully!
```

### Manual Testing

**1. Test WebFinger**:
```bash
curl "https://social.dais.example.com/.well-known/webfinger?resource=acct:yourusername@social.dais.example.com"
```

Expected response:
```json
{
  "subject": "acct:yourusername@social.dais.example.com",
  "links": [
    {
      "rel": "self",
      "type": "application/activity+json",
      "href": "https://social.dais.example.com/users/yourusername"
    }
  ]
}
```

**2. Test Actor Profile**:
```bash
curl -H "Accept: application/activity+json" https://social.dais.example.com/users/yourusername
```

**3. Test Landing Page**:

Visit `https://dais.example.com` in browser.

## Troubleshooting

### Workers not responding

**Check deployment status**:
```bash
wrangler deployments list --name actor-production
```

**Check logs**:
```bash
wrangler tail actor-production
```

**Redeploy**:
```bash
cd platforms/cloudflare/workers/actor
wrangler deploy --env production
```

### Database errors

**Verify database**:
```bash
wrangler d1 list
wrangler d1 execute dais-db --command "SELECT 1" --remote
```

### WebFinger not found

**Check DNS**:
```bash
dig social.dais.example.com
```

**Check custom domains in Cloudflare Dashboard**:
- Workers & Pages → router-production → Settings → Triggers → Custom Domains

### Federation not working

**Verify private key uploaded**:
```bash
cd platforms/cloudflare/workers/inbox
wrangler secret list --env production
```

**Check inbox logs**:
```bash
wrangler tail inbox-production
```

## Cost Breakdown

### Cloudflare Workers Free Tier

| Resource | Free Tier | Typical Usage | Cost |
|----------|-----------|---------------|------|
| Worker requests | 100,000/day | ~1,000/day | $0 |
| D1 reads | 5M/day | ~5,000/day | $0 |
| D1 writes | 100,000/day | ~100/day | $0 |
| R2 storage | 10 GB | ~1 GB | $0 |
| Queues | 1M operations/month | ~5,000/month | $0 |

**Total monthly cost for typical single-user instance**: **$0**

## Next Steps

### Test Federation

1. Follow your account from Mastodon
2. Create posts via admin interface or API
3. Check delivery logs: `wrangler tail delivery-queue-production`

### Optional Enhancements

1. **Custom Theme**: Edit `platforms/cloudflare/workers/landing/themes/`
2. **Profile Picture**: Upload to R2 and update user record
3. **Admin Interface**: Deploy custom admin panel
4. **Analytics**: Add Cloudflare Web Analytics

### Maintenance

**Backup database**:
```bash
wrangler d1 export dais-db --output=backup-$(date +%Y%m%d).sql
```

**Monitor logs**:
```bash
wrangler tail router-production
wrangler tail inbox-production
```

**Update dais**:
```bash
git pull origin release/v1.1
./scripts/test-workers.sh
# Redeploy workers
```

## Resources

- **Architecture Guide**: `ARCHITECTURE_v1.1.md`
- **Testing Guide**: `TESTING_v1.1.md`
- **Migration Guide**: `MIGRATION_GUIDE_v1.0_to_v1.1.md`
- **GitHub**: https://github.com/daisocial/dais

## Support

- **Issues**: https://github.com/daisocial/dais/issues
- **Discussions**: https://github.com/daisocial/dais/discussions

Congratulations! Your dais instance is now live and federating with the Fediverse! 🎉
