# Deploying dais to Cloudflare Workers

This guide walks you through deploying your dais instance to Cloudflare Workers, giving you a production-ready single-user ActivityPub server.

## Prerequisites

Before you begin, ensure you have:

1. **A Cloudflare account** (free tier works fine)
   - Sign up at https://cloudflare.com if you don't have one
   - Note your account ID (found in Workers & Pages dashboard)

2. **A domain name**
   - You'll need a domain to host your instance
   - Can be registered through Cloudflare or any registrar
   - Example: `example.com`

3. **Wrangler CLI installed**
   ```bash
   npm install -g wrangler
   ```

4. **Python 3.8+** (for the dais CLI)
   ```bash
   python3 --version
   ```

5. **dais CLI installed**
   ```bash
   pip install -e cli/
   ```

## Quick Start (5 Commands)

If you're already familiar with Cloudflare Workers, here's the quick version:

```bash
# 1. Initialize configuration
dais setup init

# 2. Authenticate with Cloudflare
wrangler login

# 3. Deploy everything (infrastructure, secrets, database, workers)
dais deploy all

# 4. Verify deployment
dais deploy verify

# 5. Check system health
dais doctor
```

## Step-by-Step Setup

### 1. Initialize Configuration

Run the setup wizard to configure your instance:

```bash
dais setup init
```

You'll be prompted for:
- **Username**: Your ActivityPub username (e.g., `alice`)
- **Domain**: Your main domain (e.g., `example.com`)
- **ActivityPub Domain**: Subdomain for ActivityPub (e.g., `social.example.com`)
- **PDS Domain**: Subdomain for AT Protocol (e.g., `pds.example.com`)
- **Cloudflare Account ID**: From your Cloudflare dashboard
- **Cloudflare Account Name**: Your Cloudflare account name (e.g., `alice-smith`)

This will:
- Generate cryptographic keys for signing ActivityPub activities
- Create a configuration file at `.dais/config.toml`
- Set up directory structure

### 2. Authenticate with Cloudflare

Authenticate wrangler with your Cloudflare account:

```bash
wrangler login
```

This opens a browser window for authentication. After logging in, wrangler can deploy to your account.

### 3. Create Infrastructure

Create the required Cloudflare resources (D1 database and R2 bucket):

```bash
dais deploy infrastructure
```

This command:
- Creates a D1 database named `dais-db` (or your configured name)
- Creates an R2 bucket named `dais-media` (or your configured name)
- Saves the resource IDs to your configuration

**Note**: If resources already exist, the command will detect them and continue.

### 4. Upload Secrets

Upload your private key to Cloudflare Workers:

```bash
dais deploy secrets
```

This uploads the private key (generated during `dais setup init`) as a secret to all workers that need it:
- actor
- inbox
- outbox
- delivery-queue

The private key is used to sign ActivityPub activities (HTTP signatures).

### 5. Apply Database Migrations

Create the database schema by applying migrations:

```bash
dais deploy database
```

This runs all SQL migrations in `cli/migrations/` against your D1 database, creating tables for:
- Posts and activities
- Followers and following
- Media attachments
- Direct messages
- AT Protocol records

### 6. Deploy Workers

Deploy all 8 Cloudflare Workers:

```bash
dais deploy workers
```

This command:
1. Generates `wrangler.toml` files from templates using your configuration
2. Deploys workers in the correct order:
   - webfinger (handles `.well-known/webfinger` lookups)
   - actor (serves ActivityPub actor profile)
   - inbox (receives incoming activities)
   - outbox (serves your posts feed)
   - pds (AT Protocol Personal Data Server)
   - delivery-queue (processes outgoing deliveries)
   - router (routes traffic to appropriate workers)
   - landing (serves your homepage)

### 7. Configure DNS

**Important**: You must configure DNS for your domains to point to Cloudflare Workers.

In your Cloudflare dashboard (DNS settings):

#### For your main domain (`example.com`):
- **Type**: CNAME
- **Name**: `@` (or `example.com`)
- **Target**: `<your-account>.workers.dev`
- **Proxy**: Orange cloud (proxied)

#### For ActivityPub domain (`social.example.com`):
- **Type**: CNAME
- **Name**: `social`
- **Target**: `<your-account>.workers.dev`
- **Proxy**: Orange cloud (proxied)

#### For PDS domain (`pds.example.com`):
- **Type**: CNAME
- **Name**: `pds`
- **Target**: `<your-account>.workers.dev`
- **Proxy**: Orange cloud (proxied)

#### Alternative: Use Cloudflare Dashboard

In Workers & Pages → router-production → Settings → Triggers → Custom Domains:
- Add: `social.example.com`
- Add: `pds.example.com`

In Workers & Pages → landing-production → Settings → Triggers → Custom Domains:
- Add: `example.com`
- Add: `www.example.com`

### 8. Verify Deployment

Check that everything is working:

```bash
dais deploy verify
```

This tests:
- WebFinger endpoint (`https://example.com/.well-known/webfinger`)
- Actor endpoint (`https://social.example.com/users/alice`)
- Worker deployment status

Expected output:
```
1. Testing WebFinger endpoint
✓ WebFinger endpoint working

2. Testing Actor endpoint
✓ Actor endpoint working

3. Checking worker status
✓ Workers are deployed

Summary
✓ All checks passed (3/3)

Your dais instance is deployed and working!

Your actor URL: https://social.example.com/users/alice
You can now follow @alice@example.com from other instances
```

### 9. Test Federation

Test that other ActivityPub servers can discover you:

```bash
# Test your WebFinger
dais test webfinger

# Test your Actor profile
dais test actor

# Test federation with another instance (e.g., Mastodon)
dais test federation @user@mastodon.social
```

## Troubleshooting

### Run Diagnostics

If something isn't working, run the doctor command:

```bash
dais doctor
```

This checks:
- ✓ Config file exists
- ✓ Keys generated
- ✓ Wrangler installed
- ✓ Cloudflare authenticated
- ✓ D1 database exists
- ✓ R2 bucket exists
- ✓ Workers deployed
- ✓ WebFinger responding
- ✓ Actor responding

And provides specific suggestions for fixing any issues.

### Common Issues

#### "Config not found"
Run `dais setup init` to create configuration.

#### "Keys missing"
Run `dais setup init` to generate cryptographic keys.

#### "Not logged in to Cloudflare"
Run `wrangler login` to authenticate.

#### "D1 database not found"
Run `dais deploy infrastructure` to create the database.

#### "WebFinger/Actor endpoint unreachable"
- Check that DNS is configured correctly
- Ensure workers are deployed (`dais deploy workers`)
- Wait a few minutes for DNS propagation
- Verify custom domains are added to workers in Cloudflare dashboard

#### "Worker deployment failed"
- Check that all required resources exist (D1, R2)
- Ensure secrets are uploaded (`dais deploy secrets`)
- Check `wrangler deploy` output for specific errors
- Verify your Cloudflare account has Workers enabled

## Updating to New Versions

When you pull new code from the repository:

```bash
# 1. Pull latest code
git pull

# 2. Apply any new database migrations
dais deploy database

# 3. Redeploy workers with new code
dais deploy workers

# 4. Verify everything still works
dais deploy verify
```

## Security Considerations

### Private Key Security

- Your private key (`.dais/keys/private.pem`) is **critical** for security
- Never commit it to git (it's in `.gitignore`)
- Back it up securely (encrypted backup recommended)
- If compromised, you'll need to generate a new keypair and redeploy

### Secrets Management

- Secrets are uploaded to Cloudflare Workers using `dais deploy secrets`
- They're stored securely in Cloudflare's infrastructure
- Never log or expose the PRIVATE_KEY environment variable
- Rotate keys periodically for best security

### Access Control

- Your Cloudflare API token should have minimal permissions:
  - Workers Scripts: Edit
  - D1: Edit
  - R2: Edit
- Never share your API token
- Revoke and regenerate if compromised

## Cost Estimates

Cloudflare Workers **Free Tier** includes:
- 100,000 requests/day across all workers
- 10ms CPU time per request
- Unlimited D1 database reads (5 million writes/month)
- 10 GB R2 storage (1 million reads/month, 1 million writes/month)

For a typical single-user instance:
- **Cost**: $0/month (free tier is sufficient)
- If you exceed free tier: ~$5/month for Workers ($0.50 per million requests)

R2 storage costs:
- $0.015/GB/month for storage beyond 10 GB
- No egress fees (unlike S3)

## Next Steps

Once deployed:

1. **Create your first post**:
   ```bash
   dais post create "Hello, fediverse! 👋"
   ```

2. **Follow someone**:
   ```bash
   dais follow add @user@mastodon.social
   ```

3. **Check your timeline**:
   ```bash
   dais timeline home
   ```

4. **Share your profile**:
   - Your profile: `https://social.example.com/users/alice`
   - Tell people to follow: `@alice@example.com`

## Advanced Configuration

### Custom Themes

Edit `workers/actor/wrangler.toml` and `workers/outbox/wrangler.toml`:

```toml
[env.production.vars]
THEME = "cat-dark"  # or "cat-light"
```

Then redeploy:
```bash
dais deploy workers
```

### Multiple Environments

You can create separate dev/staging/production environments by:
1. Creating new wrangler environments in `wrangler.toml`
2. Using different D1 databases and R2 buckets
3. Deploying with `wrangler deploy --env staging`

### Monitoring

View worker logs:
```bash
wrangler tail router-production
wrangler tail inbox-production
```

View D1 database stats:
```bash
wrangler d1 execute <database-id> --command "SELECT COUNT(*) FROM posts"
```

View R2 bucket usage:
```bash
wrangler r2 bucket list
```

## Getting Help

- **GitHub Issues**: https://github.com/marctjones/dais/issues
- **Cloudflare Docs**: https://developers.cloudflare.com/workers/
- **ActivityPub Spec**: https://www.w3.org/TR/activitypub/

## Contributing

Found a bug in deployment? Want to improve this guide?

- Open an issue: https://github.com/marctjones/dais/issues
- Submit a PR: https://github.com/marctjones/dais/pulls

## License

See LICENSE file for details.
