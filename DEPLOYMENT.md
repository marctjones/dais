# Production Deployment Guide

Complete guide for deploying dais to Cloudflare Workers in production.

## Prerequisites

### Cloudflare Account
- Free tier Cloudflare account
- Domain added to Cloudflare (e.g., `dais.social`)
- DNS managed by Cloudflare

### Local Tools
- [Wrangler CLI](https://developers.cloudflare.com/workers/wrangler/install-and-update/) installed
- Git repository cloned
- Rust toolchain installed (for building workers)
- Python 3.11+ (for CLI tools)

### Authentication
```bash
# Login to Cloudflare
wrangler login

# Verify authentication
wrangler whoami
```

## Architecture Overview

The dais deployment uses a **router pattern** with 5 workers:

```
┌─────────────────────────────────────────┐
│  social.dais.social (custom domain)     │
└──────────────┬──────────────────────────┘
               │
        ┌──────▼──────┐
        │   Router    │  ← Owns custom domain
        │   Worker    │     Routes by path
        └─────┬───────┘
              │
    ┌─────────┼─────────┬─────────┐
    │         │         │         │
┌───▼───┐ ┌──▼───┐ ┌───▼──┐ ┌────▼────┐
│WebFing│ │Actor │ │Inbox │ │ Outbox  │
│  er   │ │      │ │      │ │         │
└───────┘ └──────┘ └──────┘ └─────────┘
   *.workers.dev URLs (backend)
```

**Why router pattern?**
- Cloudflare custom domains only allow ONE worker per domain
- Router proxies requests to appropriate backend workers
- Backend workers run on `*.workers.dev` URLs

## Step 1: Create D1 Database

```bash
# Create production database
wrangler d1 create dais-social

# Save the database_id from output
# Example: f90f9da8-136c-40c6-b96a-eba38d7efa65
```

Update database_id in all `wrangler.toml` files:
- `workers/actor/wrangler.toml`
- `workers/inbox/wrangler.toml`
- `workers/outbox/wrangler.toml`

## Step 2: Run Database Migrations

```bash
# Navigate to any worker with D1 binding
cd workers/actor

# Run initial schema migration
wrangler d1 execute DB --remote --file=../../cli/migrations/001_initial_schema.sql

# Verify tables created
wrangler d1 execute DB --remote --command="SELECT name FROM sqlite_master WHERE type='table';"
```

Expected tables:
- `actors`
- `posts`
- `followers`
- `activities`

## Step 3: Generate Actor Keys

```bash
# Generate RSA keypair for HTTP signatures
cd cli/test_keys

# Generate new keys (if not already done)
python -c "
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.backends import default_backend

private_key = rsa.generate_private_key(
    public_exponent=65537,
    key_size=2048,
    backend=default_backend()
)

# Save private key
with open('private_key.pem', 'wb') as f:
    f.write(private_key.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption()
    ))

# Save public key
public_key = private_key.public_key()
with open('public_key.pem', 'wb') as f:
    f.write(public_key.public_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PublicFormat.SubjectPublicKeyInfo
    ))
"

# Read public key for database
cat public_key.pem
```

## Step 4: Seed Actor Data

```bash
# Insert your actor into database
cd workers/actor

wrangler d1 execute DB --remote --command="
INSERT INTO actors (
  id,
  username,
  display_name,
  summary,
  public_key_pem,
  created_at
) VALUES (
  'https://social.dais.social/users/social',
  'social',
  'dais',
  'Official account for the dais project - a self-hosted, single-user ActivityPub server running on Cloudflare Workers.',
  '$(cat ../../cli/test_keys/public_key.pem | tr -d '\n')',
  datetime('now')
);
"

# Verify actor created
wrangler d1 execute DB --remote --command="SELECT username, display_name FROM actors;"
```

## Step 5: Deploy Backend Workers

Deploy workers in this order:

```bash
# 1. WebFinger (discovery)
cd workers/webfinger
wrangler deploy --env production

# 2. Actor (profile)
cd ../actor
wrangler deploy --env production

# 3. Inbox (receiving activities)
cd ../inbox
wrangler deploy --env production

# 4. Outbox (serving posts)
cd ../outbox
wrangler deploy --env production

# 5. Landing page
cd ../landing
wrangler deploy --env production
```

Note the deployed URLs:
- `https://webfinger-production.YOUR_ACCOUNT.workers.dev`
- `https://actor-production.YOUR_ACCOUNT.workers.dev`
- `https://inbox-production.YOUR_ACCOUNT.workers.dev`
- `https://outbox-production.YOUR_ACCOUNT.workers.dev`
- `https://landing-production.YOUR_ACCOUNT.workers.dev`

## Step 6: Configure Router Worker

Update `workers/router/wrangler.toml` with your worker URLs:

```toml
[env.production.vars]
WEBFINGER_URL = "https://webfinger-production.YOUR_ACCOUNT.workers.dev"
ACTOR_URL = "https://actor-production.YOUR_ACCOUNT.workers.dev"
INBOX_URL = "https://inbox-production.YOUR_ACCOUNT.workers.dev"
OUTBOX_URL = "https://outbox-production.YOUR_ACCOUNT.workers.dev"
```

Deploy router:

```bash
cd workers/router
wrangler deploy --env production
```

## Step 7: Configure Custom Domains

### Add Domains to Cloudflare

In Cloudflare Dashboard:

1. **DNS** → **Records**
2. Add CNAME records:

| Type | Name | Target | Proxy |
|------|------|--------|-------|
| CNAME | @ | landing-production.YOUR_ACCOUNT.workers.dev | ✅ Proxied |
| CNAME | www | landing-production.YOUR_ACCOUNT.workers.dev | ✅ Proxied |
| CNAME | social | router-production.YOUR_ACCOUNT.workers.dev | ✅ Proxied |

### Verify Custom Domains

The router worker's `wrangler.toml` already has:

```toml
[env.production]
routes = [
  { pattern = "social.dais.social", custom_domain = true }
]
```

And landing worker has:

```toml
[env.production]
routes = [
  { pattern = "dais.social", custom_domain = true },
  { pattern = "www.dais.social", custom_domain = true }
]
```

## Step 8: Configure CLI

```bash
cd cli

# Create/edit config
cat > ~/.dais/config.json <<EOF
{
  "server": {
    "domain": "dais.social",
    "activitypub_domain": "social.dais.social",
    "username": "social"
  }
}
EOF

# Test CLI connection
dais post list --remote
```

## Step 9: Create First Post

```bash
dais post create "Hello, fediverse! 🎉 This is my first post from dais." --remote

# Verify post appears
dais post list --remote

# Check outbox
curl -H "Accept: application/activity+json" "https://social.dais.social/users/social/outbox"
```

## Step 10: Enable Media Attachments (R2)

**Optional but recommended** - Enables image/video uploads with posts.

### 10.1: Enable R2 Service

R2 must be enabled through the Cloudflare Dashboard (cannot be done via CLI):

1. Go to **Cloudflare Dashboard** → **R2**
2. Click **"Enable R2"**
3. Accept terms of service
4. Confirm (no payment required for free tier)

**Free Tier Limits:**
- 10 GB storage/month
- 10M Class A operations/month
- 10M Class B operations/month
- 10 GB egress/month

More than enough for a single-user server!

### 10.2: Create R2 Bucket

```bash
# Create bucket for media storage
wrangler r2 bucket create dais-media

# Verify bucket created
wrangler r2 bucket list
```

Expected output:
```
┌─────────────┬──────────────────────────────────┐
│ Name        │ Created                          │
├─────────────┼──────────────────────────────────┤
│ dais-media  │ 2026-03-09T...                   │
└─────────────┴──────────────────────────────────┘
```

### 10.3: Configure Public Domain (Optional)

For custom domain like `media.dais.social`:

1. Go to **Cloudflare Dashboard** → **R2** → **dais-media**
2. Click **Settings** → **Public Access**
3. Click **Connect Domain**
4. Enter `media.dais.social`
5. Confirm DNS records

Or use default R2.dev public URL (format: `pub-<hash>.r2.dev`).

### 10.4: Update Worker Configuration

Uncomment R2 bindings in `workers/outbox/wrangler.toml`:

```toml
# Before:
# [[r2_buckets]]
# binding = "MEDIA_BUCKET"
# bucket_name = "dais-media"

# After:
[[r2_buckets]]
binding = "MEDIA_BUCKET"
bucket_name = "dais-media"

[[env.production.r2_buckets]]
binding = "MEDIA_BUCKET"
bucket_name = "dais-media"
```

### 10.5: Redeploy Outbox Worker

```bash
cd workers/outbox

# Deploy to dev
wrangler deploy

# Deploy to production
wrangler deploy --env production
```

### 10.6: Test Media Upload

```bash
# Create test image
convert -size 800x600 xc:lightblue -pointsize 72 -gravity center \
  -annotate +0+0 "Test Image" test.jpg

# Create post with image
dais post create "Testing media attachments! 📷" --attach test.jpg --remote

# View in browser
open https://social.dais.social/users/social/outbox
```

Expected: Image displays inline in post.

### 10.7: Verify Federation

Posts with images should federate to Mastodon:

1. Search for `@social@dais.social` on Mastodon
2. View timeline
3. Images should display inline

**Troubleshooting:**

If upload fails with `403 Forbidden`:
- Verify R2 is enabled in dashboard
- Check bucket exists: `wrangler r2 bucket list`
- Verify wrangler authentication: `wrangler whoami`

If images don't display:
- Check R2 bucket public access enabled
- Verify `media.dais.social` domain configured
- Check browser console for CORS errors

## Step 11: Test Federation

### Test WebFinger

```bash
curl "https://social.dais.social/.well-known/webfinger?resource=acct:social@dais.social"
```

Expected: JSON with `subject` and `links`

### Test Actor Profile

```bash
curl -H "Accept: application/activity+json" "https://social.dais.social/users/social"
```

Expected: ActivityPub Person object

### Test from Mastodon

1. Open Mastodon instance
2. Search for: `@social@dais.social`
3. Click "Follow"
4. Check follower requests:

```bash
dais followers list --status pending --remote
```

5. Approve follower:

```bash
dais followers approve <follower-actor-url> --remote
```

## Monitoring

### View Worker Logs

```bash
# Real-time logs
wrangler tail router-production
wrangler tail actor-production
wrangler tail inbox-production
wrangler tail outbox-production

# Or in dashboard
# Cloudflare Dashboard → Workers & Pages → Select worker → Logs
```

### Check Analytics

Cloudflare Dashboard → Workers & Pages → Select worker → Metrics

- Requests per second
- Errors
- CPU time
- Duration

### Database Queries

```bash
# Count posts
wrangler d1 execute DB --remote --command="SELECT COUNT(*) FROM posts;"

# Count followers
wrangler d1 execute DB --remote --command="SELECT COUNT(*) FROM followers WHERE status='approved';"

# Recent activities
wrangler d1 execute DB --remote --command="SELECT type, created_at FROM activities ORDER BY created_at DESC LIMIT 10;"
```

## Troubleshooting

### 404 on custom domain

**Problem:** `https://social.dais.social/users/social` returns 404

**Solution:**
1. Check DNS is proxied (orange cloud)
2. Verify router has custom domain route
3. Check router environment variables point to `-production` workers
4. Wait 2-3 minutes for DNS propagation

```bash
# Test router directly
curl "https://router-production.YOUR_ACCOUNT.workers.dev/users/social"

# Check router logs
wrangler tail router-production
```

### Posts not federating

**Problem:** Posts created but don't appear on Mastodon

**Solution:**
1. Check if followers are approved
2. Verify HTTP signature generation
3. Check delivery logs

```bash
dais followers list --remote
wrangler tail outbox-production
```

### Database connection errors

**Problem:** Workers can't access D1

**Solution:**
1. Verify `database_id` matches in all `wrangler.toml` files
2. Check D1 binding name is `DB`
3. Re-deploy workers after config changes

```bash
# Verify binding
wrangler deploy --env production --dry-run
```

### CORS errors in browser

**Problem:** Browser shows CORS errors when accessing ActivityPub endpoints

**Solution:**
- OPTIONS endpoints already configured
- Check Access-Control headers are present

```bash
curl -X OPTIONS -I "https://social.dais.social/users/social/outbox"
# Should return Access-Control-Allow-Origin: *
```

## Security Checklist

- [x] HTTPS enforced (automatic with Cloudflare)
- [x] HTTP signatures verified in inbox
- [x] Rate limiting (Cloudflare default: 100k req/day free tier)
- [x] DDoS protection (Cloudflare automatic)
- [x] Private keys NOT in git (use `cli/test_keys/` - gitignored)
- [x] Database credentials via Wrangler bindings only
- [x] CORS configured for ActivityPub federation
- [x] Input validation in all workers

## Maintenance

### Update Workers

```bash
# Pull latest code
git pull origin main

# Rebuild and deploy
cd workers/actor && wrangler deploy --env production
cd workers/inbox && wrangler deploy --env production
cd workers/outbox && wrangler deploy --env production
cd workers/router && wrangler deploy --env production
```

### Database Backups

```bash
# Export D1 database
wrangler d1 export DB --remote --output=backup-$(date +%Y%m%d).sql

# Store backups securely (not in git)
mv backup-*.sql ~/backups/dais/
```

### Monitoring Costs

Cloudflare Free Tier limits:
- 100,000 requests/day per worker
- 10ms CPU time per request
- 5GB D1 storage
- 5M D1 rows read/day

Check usage: Cloudflare Dashboard → Workers & Pages → Plans

## Upgrading

### Database Schema Changes

```bash
# Create migration file
cat > cli/migrations/002_add_media.sql <<EOF
CREATE TABLE IF NOT EXISTS media (
  id TEXT PRIMARY KEY,
  post_id TEXT NOT NULL,
  media_type TEXT NOT NULL,
  url TEXT NOT NULL,
  FOREIGN KEY (post_id) REFERENCES posts(id)
);
EOF

# Run migration
wrangler d1 execute DB --remote --file=cli/migrations/002_add_media.sql
```

### Worker Updates

1. Test locally first (see DEVELOPMENT.md)
2. Deploy to production during low-traffic period
3. Monitor logs for errors
4. Rollback if needed:

```bash
# List deployments
wrangler deployments list --env production

# Rollback to previous version
wrangler rollback --env production <deployment-id>
```

## Cost Estimate

For single-user instance with ~100 posts/month and ~50 followers:

| Service | Usage | Cost |
|---------|-------|------|
| Workers (5x) | ~50k req/month | Free |
| D1 Database | ~1M rows read/month | Free |
| Custom Domains | 2 domains | Free |
| **Total** | | **$0/month** |

Cloudflare Workers free tier is generous for single-user instances!

## Next Steps

Once deployed:

1. **Follow from Mastodon** - Test federation
2. **Create posts** - Use `dais post create --remote`
3. **Add media** - Implement R2 bucket for images (Phase 2.5)
4. **Build web UI** - HTML interface for viewing posts (Phase 3)
5. **Analytics** - Track follower growth and engagement

---

**Need help?** Open an issue on [GitHub](https://github.com/marctjones/dais/issues)
