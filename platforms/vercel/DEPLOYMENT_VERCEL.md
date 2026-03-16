# Deploying dais to Vercel Edge Functions

**Version**: v1.2.0
**Platform**: Vercel Edge Functions
**Database**: Neon PostgreSQL
**Storage**: Vercel Blob
**Queue**: Upstash Redis

## Overview

This guide walks you through deploying dais v1.2 to Vercel Edge Functions with Neon PostgreSQL database.

**Deployment time**: 45-60 minutes
**Cost**: Vercel Hobby tier ($0/month for most single-user instances)

## Prerequisites

### Required

- [x] **Vercel account** - Hobby tier is sufficient (https://vercel.com/signup)
- [x] **Neon account** - Free tier available (https://neon.tech)
- [x] **Upstash account** - Free tier available (https://upstash.com)
- [x] **Domain name** - You own a domain
- [x] **Git repository** - GitHub, GitLab, or Bitbucket
- [x] **Vercel CLI** - Install with `npm install -g vercel`

### Optional

- [ ] **Custom domain** on Vercel
- [ ] **Bluesky account** for AT Protocol integration

## Step 1: Setup Neon PostgreSQL

### Create Database

1. Go to https://console.neon.tech
2. Click "Create a project"
3. Choose a name: `dais-db`
4. Select region (closest to your users)
5. Click "Create project"

### Get Connection String

1. In Neon dashboard, go to your project
2. Click "Connection Details"
3. Copy the connection string (it looks like):
   ```
   postgresql://username:password@ep-xxx-xxx.us-east-2.aws.neon.tech/daisdb?sslmode=require
   ```
4. Save this for later

### Apply Database Schema

```bash
# Install psql if not already installed
# Ubuntu/Debian: sudo apt install postgresql-client
# macOS: brew install postgresql

# Apply migrations
psql "$DATABASE_URL" -f cli/migrations/001_initial_schema.sql
psql "$DATABASE_URL" -f cli/migrations/002_indexes.sql
psql "$DATABASE_URL" -f cli/migrations/003_at_protocol.sql
```

### Create Initial User

```bash
psql "$DATABASE_URL" -c "
INSERT INTO users (username, email, display_name, summary, created_at)
VALUES ('yourusername', 'you@example.com', 'Your Name', 'Your bio', NOW())
"
```

## Step 2: Setup Upstash Redis

### Create Redis Database

1. Go to https://console.upstash.com
2. Click "Create Database"
3. Name: `dais-queue`
4. Region: Same as your Vercel deployment
5. Click "Create"

### Get Redis Credentials

1. In Upstash dashboard, click on your database
2. Scroll to "REST API" section
3. Copy:
   - `UPSTASH_REDIS_REST_URL`
   - `UPSTASH_REDIS_REST_TOKEN`
4. Save these for later

## Step 3: Setup Vercel Blob Storage

Vercel Blob is automatically available in your Vercel project.

1. Go to https://vercel.com/dashboard
2. Select your project (or create new one)
3. Go to "Storage" tab
4. Click "Create Database"
5. Select "Blob"
6. Click "Continue"
7. Name: `dais-media`
8. Click "Create"

The `BLOB_READ_WRITE_TOKEN` will be automatically added to your environment variables.

## Step 4: Clone and Configure

### Clone Repository

```bash
git clone https://github.com/daisocial/dais.git
cd dais
git checkout v1.2.0
```

### Configure Environment Variables

Create `.env` file:

```bash
# Database (Neon PostgreSQL)
DATABASE_URL=postgresql://username:password@ep-xxx.us-east-2.aws.neon.tech/daisdb?sslmode=require

# Storage (Vercel Blob - auto-configured by Vercel)
BLOB_READ_WRITE_TOKEN=vercel_blob_rw_xxxxx

# Queue (Upstash Redis)
UPSTASH_REDIS_REST_URL=https://xxx.upstash.io
UPSTASH_REDIS_REST_TOKEN=xxxxx

# Configuration
DOMAIN=dais.example.com
ACTIVITYPUB_DOMAIN=social.dais.example.com
PDS_DOMAIN=pds.dais.example.com
USERNAME=yourusername
```

## Step 5: Deploy to Vercel

### Install Vercel CLI

```bash
npm install -g vercel
```

### Login to Vercel

```bash
vercel login
```

### Deploy

```bash
cd platforms/vercel
vercel --prod
```

Follow the prompts:
- Set up and deploy: Yes
- Which scope: Your account
- Link to existing project: No (first time)
- Project name: dais
- Directory: `platforms/vercel`
- Override settings: No

### Add Environment Variables

```bash
# Add Database URL
vercel env add DATABASE_URL production

# Add Redis credentials
vercel env add UPSTASH_REDIS_REST_URL production
vercel env add UPSTASH_REDIS_REST_TOKEN production

# Add configuration
vercel env add DOMAIN production
vercel env add ACTIVITYPUB_DOMAIN production
vercel env add USERNAME production
```

Paste the values when prompted.

### Upload Private Key

```bash
# Generate keys if not already done
mkdir -p ~/.dais/keys
openssl genrsa -out ~/.dais/keys/private.pem 2048
openssl rsa -in ~/.dais/keys/private.pem -pubout -out ~/.dais/keys/public.pem

# Add as environment variable
vercel env add PRIVATE_KEY production < ~/.dais/keys/private.pem
```

### Redeploy with Environment Variables

```bash
vercel --prod
```

## Step 6: Configure Custom Domain

### Add Domain to Vercel

1. In Vercel dashboard, go to your project
2. Go to "Settings" → "Domains"
3. Add your domains:
   - `social.dais.example.com` (ActivityPub)
   - `pds.dais.example.com` (AT Protocol, optional)
   - `dais.example.com` (Landing page, optional)

### Configure DNS

In your DNS provider (e.g., Cloudflare, Namecheap):

Add CNAME records:
```
social.dais.example.com  CNAME  cname.vercel-dns.com.
pds.dais.example.com     CNAME  cname.vercel-dns.com.
dais.example.com         CNAME  cname.vercel-dns.com.
```

Wait for DNS propagation (~5-30 minutes).

## Step 7: Verify Deployment

### Test WebFinger

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

### Test Actor Profile

```bash
curl -H "Accept: application/activity+json" https://social.dais.example.com/users/yourusername
```

### Test from Mastodon

1. Open your Mastodon instance
2. Search for `@yourusername@social.dais.example.com`
3. Click "Follow"
4. Check Vercel function logs for incoming Follow activity

## Step 8: Monitor Deployment

### View Logs

```bash
# Real-time logs
vercel logs --follow

# Function-specific logs
vercel logs --follow --function webfinger
```

### View Metrics

1. In Vercel dashboard, go to your project
2. Click "Analytics"
3. View:
   - Request count
   - Response times
   - Error rates
   - Bandwidth usage

## Troubleshooting

### Function Timeout

**Error**: Function exceeded maximum duration

**Solution**: Increase function timeout in `vercel.json`:
```json
{
  "functions": {
    "functions/webfinger/src/lib.rs": {
      "maxDuration": 30
    }
  }
}
```

### Database Connection Error

**Error**: Failed to connect to Neon

**Solution**:
1. Verify DATABASE_URL is correct
2. Check Neon database is running
3. Verify SSL mode is `require` in connection string

### Missing Environment Variables

**Error**: Environment variable not found

**Solution**:
```bash
# List all env vars
vercel env ls

# Add missing var
vercel env add VARIABLE_NAME production

# Redeploy
vercel --prod
```

### CORS Errors

**Error**: Access-Control-Allow-Origin missing

**Solution**: Ensure functions return CORS headers:
```rust
Response::builder()
    .header("Access-Control-Allow-Origin", "*")
    .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
    .header("Access-Control-Allow-Headers", "Content-Type, Authorization")
```

## Cost Breakdown

### Vercel Hobby Tier (Free)

| Resource | Free Tier | Typical Usage | Cost |
|----------|-----------|---------------|------|
| Function invocations | 100 GB-hours/month | ~10 GB-hours | $0 |
| Bandwidth | 100 GB/month | ~5 GB | $0 |
| Serverless function duration | 100 GB-hours | ~10 GB-hours | $0 |
| Blob storage | 500 MB | ~100 MB | $0 |

### Neon Free Tier

| Resource | Free Tier | Typical Usage | Cost |
|----------|-----------|---------------|------|
| Storage | 3 GB | ~100 MB | $0 |
| Compute hours | 191 hours/month | ~100 hours | $0 |
| Data transfer | 5 GB/month | ~1 GB | $0 |

### Upstash Free Tier

| Resource | Free Tier | Typical Usage | Cost |
|----------|-----------|---------------|------|
| Commands | 10,000/day | ~1,000/day | $0 |
| Storage | 256 MB | ~10 MB | $0 |
| Bandwidth | 1 GB/month | ~100 MB | $0 |

**Total monthly cost for typical single-user instance**: **$0**

### Paid Tier (if exceeding free tier)

- Vercel Pro: $20/month (unlimited bandwidth, 1000 GB-hours)
- Neon Scale: $19/month (starts at 3 GB storage)
- Upstash: ~$10/month (100K commands/day)

## Performance

### Expected Metrics

- **Function cold start**: ~200-300ms
- **Function warm start**: ~50-100ms
- **Database query**: ~10-30ms (Neon)
- **Redis operation**: ~5-10ms (Upstash)
- **Global latency**: ~50-200ms (via Edge Network)

### Optimization Tips

1. **Minimize cold starts**: Keep functions warm with periodic health checks
2. **Connection pooling**: Use PgBouncer or Neon's connection pooling
3. **Cache frequently accessed data**: Use Redis for caching
4. **Optimize bundle size**: Remove unused dependencies
5. **Use Edge Config**: For static configuration data

## Next Steps

### Federation Testing

1. Follow your account from Mastodon
2. Create posts via API or admin interface
3. Verify federation with other ActivityPub servers

### Add More Functions

Deploy additional functions:
- `actor` - Actor profiles
- `inbox` - Receive activities
- `outbox` - Serve posts
- `pds` - AT Protocol support

### Custom Domain

1. Add custom domain in Vercel settings
2. Configure DNS records
3. Wait for SSL certificate provisioning

### Monitoring

1. Enable Vercel Analytics
2. Set up uptime monitoring (e.g., UptimeRobot)
3. Configure error alerts

## Resources

- **Vercel Documentation**: https://vercel.com/docs
- **Neon Documentation**: https://neon.tech/docs
- **Upstash Documentation**: https://upstash.com/docs
- **dais Architecture Guide**: `../../ARCHITECTURE_v1.1.md`
- **dais GitHub**: https://github.com/daisocial/dais

## Support

- **Vercel Support**: https://vercel.com/support
- **Neon Discord**: https://discord.gg/neon
- **Upstash Discord**: https://upstash.com/discord
- **dais Issues**: https://github.com/daisocial/dais/issues

Congratulations! Your dais instance is now running on Vercel Edge Functions! 🎉
