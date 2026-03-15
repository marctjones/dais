# Installation Guide

Quick installation guide for dais v1.0.0 on Cloudflare.

## Prerequisites

Before installing dais, ensure you have:

### Required
- **Cloudflare Account** - [Sign up for free](https://dash.cloudflare.com/sign-up)
- **Domain Name** - Must be added to Cloudflare DNS
- **Python 3.10+** - [Download Python](https://www.python.org/downloads/)
- **Rust** - [Install Rust](https://rustup.rs/)
- **wrangler CLI** - Install with `npm install -g wrangler`

### Optional
- **Git** - For cloning repository
- **tmux** - For local development environment

## Quick Install (5 Steps)

### Step 1: Clone Repository

```bash
git clone https://github.com/yourusername/dais.git
cd dais
```

### Step 2: Install CLI

```bash
cd cli
pip install -e .
```

Verify installation:
```bash
dais --version
# Should output: dais, version 1.0.0
```

### Step 3: Initialize Configuration

```bash
dais setup init
```

You'll be prompted for:
- **Server domain** (e.g., `yourdomain.com`)
- **ActivityPub domain** (e.g., `social.yourdomain.com`)
- **PDS domain** (e.g., `pds.yourdomain.com`)
- **Username** (e.g., `social`)
- **Cloudflare Account ID** - Find at: Cloudflare Dashboard → Workers → Account ID
- **Cloudflare API Token** - Create at: Cloudflare Dashboard → API Tokens → Create Token
  - Use "Edit Cloudflare Workers" template
  - Permissions needed: Workers Scripts (Edit), D1 (Edit), R2 (Edit)

This creates:
- `~/.dais/config.toml` - Configuration file
- `~/.dais/keys/private.pem` - RSA-4096 private key
- `~/.dais/keys/public.pem` - RSA-4096 public key

### Step 4: Deploy to Cloudflare

```bash
dais deploy all
```

This command:
1. ✓ Creates D1 database (`dais-db`)
2. ✓ Creates R2 bucket (`dais-media`)
3. ✓ Uploads private key to Workers
4. ✓ Applies database migrations
5. ✓ Deploys all 9 Workers
6. ✓ Verifies deployment health

**Time**: ~5 minutes

### Step 5: Configure DNS

Add DNS records in Cloudflare Dashboard:

#### WebFinger (Required)
- **Type**: CNAME
- **Name**: `@` (root domain)
- **Target**: `yourdomain.com` (proxied through Cloudflare)

#### ActivityPub Endpoint (Required)
- **Type**: CNAME
- **Name**: `social`
- **Target**: `yourdomain.com` (proxied through Cloudflare)

#### AT Protocol PDS (Optional, for Bluesky)
- **Type**: CNAME
- **Name**: `pds`
- **Target**: `yourdomain.com` (proxied through Cloudflare)

See [DNS_SETUP.md](DNS_SETUP.md) for detailed DNS configuration.

## Verification

### Test WebFinger

```bash
dais test webfinger
```

Expected output:
```
✓ WebFinger endpoint responding
✓ Resource found: acct:social@yourdomain.com
✓ ActivityPub link present
```

### Test Actor Endpoint

```bash
dais test actor
```

Expected output:
```
✓ Actor endpoint responding
✓ Valid ActivityPub actor
✓ Public key present
```

### Run Full Diagnostics

```bash
dais doctor
```

Expected output:
```
✓ Config file exists
✓ Keys generated
✓ Wrangler installed
✓ Cloudflare authenticated
✓ D1 database exists
✓ R2 bucket exists
✓ Workers deployed
✓ WebFinger responding
✓ Actor responding
```

## Authentication Setup (Optional)

Set up Cloudflare Access for API authentication:

```bash
dais auth setup
```

Follow the interactive wizard to:
1. Create Cloudflare Zero Trust account
2. Set up Access application
3. Configure identity provider (Google, GitHub, etc.)
4. Create access policy
5. Upload secrets

See [AUTH_API.md](AUTH_API.md) for details.

## First Post

Create your first post:

```bash
dais post create "Hello, Fediverse! 👋"
```

Or use the TUI:

```bash
dais tui
# Press 2 for Posts view
# Press 'n' to create new post
```

## Join the Fediverse

Search for your account from Mastodon, Pleroma, or any ActivityPub client:

```
@social@yourdomain.com
```

Click "Follow" - you'll receive a follow request notification in dais.

Approve the follower:

```bash
dais followers list
dais followers approve @friend@mastodon.social
```

## What's Next?

### Learn the CLI

```bash
dais --help              # List all commands
dais post --help         # Post commands
dais followers --help    # Follower management
```

### Explore the TUI

```bash
dais tui                 # Launch Terminal UI
```

**Keyboard shortcuts:**
- `1-6` - Switch views
- `n` - New post
- `?` - Help

See [TUI_SHORTCUTS.md](TUI_SHORTCUTS.md) for all shortcuts.

### Read Documentation

- [USER_GUIDE.md](USER_GUIDE.md) - End-user guide
- [OPERATIONAL_RUNBOOK.md](OPERATIONAL_RUNBOOK.md) - Daily operations
- [FEDERATION_GUIDE.md](FEDERATION_GUIDE.md) - Federation details
- [FEATURES.md](FEATURES.md) - Complete feature list

## Troubleshooting

### "wrangler not found"

Install wrangler:
```bash
npm install -g wrangler
```

### "Cloudflare authentication failed"

Re-authenticate:
```bash
wrangler login
```

### "D1 database not found"

Recreate infrastructure:
```bash
dais deploy infrastructure
```

### "Workers not deploying"

Check account ID and API token:
```bash
dais config show
```

Update if needed:
```bash
dais config set cloudflare.account_id YOUR_ACCOUNT_ID
dais config set cloudflare.api_token YOUR_API_TOKEN
```

### Get Help

Run diagnostics:
```bash
dais doctor
```

Check logs:
```bash
wrangler tail webfinger
wrangler tail actor
```

## Uninstall

### Remove Workers

```bash
cd workers/webfinger && wrangler delete --env production
cd workers/actor && wrangler delete --env production
cd workers/inbox && wrangler delete --env production
cd workers/outbox && wrangler delete --env production
cd workers/auth && wrangler delete --env production
cd workers/pds && wrangler delete --env production
cd workers/delivery-queue && wrangler delete --env production
cd workers/router && wrangler delete --env production
cd workers/landing && wrangler delete --env production
```

### Delete Database & Storage

```bash
wrangler d1 delete dais-db
wrangler r2 bucket delete dais-media
```

### Remove CLI

```bash
pip uninstall dais-cli
rm -rf ~/.dais
```

## Support

- **Documentation**: [README.md](README.md)
- **Issues**: [GitHub Issues](https://github.com/yourusername/dais/issues)
- **Fediverse**: `@social@dais.social`

## Next Steps

After installation, see:
- [USER_GUIDE.md](USER_GUIDE.md) - How to use dais
- [DEPLOYMENT.md](DEPLOYMENT.md) - Production deployment best practices
- [BACKUP_RESTORE.md](BACKUP_RESTORE.md) - Backup procedures
