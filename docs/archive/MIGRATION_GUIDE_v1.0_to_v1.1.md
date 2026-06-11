# Migration Guide: v1.0 to v1.1

**Date**: March 15, 2026

This guide helps you upgrade your dais instance from v1.0 to v1.1.

## Overview

**v1.1 introduces**:
- Multi-platform architecture with 85-90% code reuse
- Database abstraction (SQLite, PostgreSQL, MySQL support)
- Portable migration system
- New directory structure
- Updated worker deployment

**Breaking Changes**:
- Directory structure changed (workers moved to `platforms/cloudflare/workers/`)
- Database queries now use abstraction layer
- Worker build process updated

**Data Compatibility**:
- ✅ Database schema unchanged - no data migration needed
- ✅ Existing D1 database works with v1.1
- ✅ ActivityPub federation remains compatible

## Prerequisites

Before upgrading, ensure you have:

- [x] Cloudflare account with existing v1.0 deployment
- [x] Git repository with v1.0 code
- [x] Backup of D1 database (optional but recommended)
- [x] Rust toolchain (1.75+)
- [x] wrangler CLI (3.0+)

## Step 1: Backup Your Data

### Export D1 Database

```bash
# Export current database
wrangler d1 export DB --output=backup-v1.0.sql

# Verify backup
ls -lh backup-v1.0.sql
```

### Save Configuration

```bash
# Save current wrangler.toml files
mkdir -p backup-v1.0/config
cp workers/*/wrangler.toml backup-v1.0/config/
```

### Save Environment Variables

```bash
# List current secrets
wrangler secret list --env production > backup-v1.0/secrets.txt
```

## Step 2: Update Git Repository

### Create Upgrade Branch

```bash
git checkout -b upgrade-v1.1
```

### Pull v1.1 Changes

```bash
# If upgrading from dais repository
git pull origin release/v1.1

# Or clone fresh
git clone https://github.com/daisocial/dais.git dais-v1.1
cd dais-v1.1
git checkout release/v1.1
```

## Step 3: Update Dependencies

### Update Rust Toolchain

```bash
rustup update stable
rustup target add wasm32-unknown-unknown
```

### Install worker-build

```bash
cargo install worker-build
```

### Update wrangler

```bash
npm install -g wrangler@latest
wrangler --version  # Should be 3.0+
```

## Step 4: Migrate Configuration

### Update wrangler.toml Files

The new structure uses templates. Copy your configuration:

**Old location** (v1.0):
```
workers/actor/wrangler.toml
workers/inbox/wrangler.toml
# etc...
```

**New location** (v1.1):
```
platforms/cloudflare/workers/actor/wrangler.toml
platforms/cloudflare/workers/inbox/wrangler.toml
# etc...
```

**Migration script**:

```bash
# For each worker, update wrangler.toml
for worker in actor inbox outbox webfinger delivery-queue auth pds router landing; do
    echo "Migrating $worker..."

    # Copy D1 database ID
    OLD_DB_ID=$(grep "database_id" workers/$worker/wrangler.toml | cut -d'"' -f2)

    # Update new wrangler.toml
    sed -i "s/database_id = \".*\"/database_id = \"$OLD_DB_ID\"/" \
        platforms/cloudflare/workers/$worker/wrangler.toml

    # Copy environment variables
    OLD_DOMAIN=$(grep "DOMAIN" workers/$worker/wrangler.toml | cut -d'"' -f2)
    sed -i "s/DOMAIN = \".*\"/DOMAIN = \"$OLD_DOMAIN\"/" \
        platforms/cloudflare/workers/$worker/wrangler.toml
done
```

### Environment Variables

Update environment-specific configuration in each `wrangler.toml`:

```toml
[env.production.vars]
DOMAIN = "your-domain.com"
ACTIVITYPUB_DOMAIN = "social.your-domain.com"
PDS_DOMAIN = "pds.your-domain.com"
THEME = "cat-light"
```

## Step 5: Database Migration

### Check Database Compatibility

```bash
# Check current schema version
wrangler d1 execute DB --command "SELECT * FROM schema_migrations"
```

v1.1 uses the same schema, so no migration needed. The new migration system is **forward-compatible**.

### Initialize Migration System (Optional)

If you want to use the new migration system:

```bash
# Create schema_migrations table
wrangler d1 execute DB --file=core/migrations/000_migration_system.sql --remote
```

## Step 6: Build and Test Workers

### Compile Core Library

```bash
cd core
cargo check
cargo test
cd ..
```

### Build All Workers

```bash
# Use the new test script
chmod +x scripts/test-workers.sh
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

### Local Testing

```bash
# Start local development environment
./scripts/dev-start.sh

# In another terminal, test endpoints
./scripts/verify-deployment.sh
```

## Step 7: Deploy to Production

### Deploy Workers One at a Time

Start with non-critical workers first:

```bash
# 1. Deploy landing page (safe, no database dependencies)
cd platforms/cloudflare/workers/landing
wrangler deploy --env production
cd ../../../..

# 2. Deploy WebFinger (read-only)
cd platforms/cloudflare/workers/webfinger
wrangler deploy --env production
cd ../../../..

# 3. Deploy actor (read-only)
cd platforms/cloudflare/workers/actor
wrangler deploy --env production
cd ../../../..

# 4. Deploy auth (isolated)
cd platforms/cloudflare/workers/auth
wrangler deploy --env production
cd ../../../..

# 5. Deploy PDS (isolated)
cd platforms/cloudflare/workers/pds
wrangler deploy --env production
cd ../../../..

# 6. Deploy outbox (read-only)
cd platforms/cloudflare/workers/outbox
wrangler deploy --env production
cd ../../../..

# 7. Deploy inbox (write operations - test carefully)
cd platforms/cloudflare/workers/inbox
wrangler deploy --env production
cd ../../../..

# 8. Deploy delivery queue (background jobs)
cd platforms/cloudflare/workers/delivery-queue
wrangler deploy --env production
cd ../../../..

# 9. Deploy router (last - routes traffic)
cd platforms/cloudflare/workers/router
wrangler deploy --env production
cd ../../../..
```

### Verify Each Deployment

After each worker deployment:

```bash
# Check worker status
wrangler deployments list --name <worker-name>

# Test endpoint
curl -I https://your-domain.com/<endpoint>

# Check logs
wrangler tail <worker-name> --env production
```

## Step 8: Verification

### Automated Verification

```bash
# Set your domain
export DOMAIN="your-domain.com"
export ACTIVITYPUB_DOMAIN="social.your-domain.com"

# Run verification script
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

1. **WebFinger**:
```bash
curl "https://social.your-domain.com/.well-known/webfinger?resource=acct:username@social.your-domain.com"
```

2. **Actor Profile**:
```bash
curl -H "Accept: application/activity+json" https://social.your-domain.com/users/username
```

3. **Federation**:
- Follow your account from Mastodon
- Check inbox logs: `wrangler tail inbox --env production`
- Verify follower appears in database

4. **Post Creation** (if using admin interface):
- Create a test post
- Check outbox: `curl https://social.your-domain.com/users/username/outbox`
- Verify delivery: `wrangler tail delivery-queue --env production`

## Step 9: Monitor for Issues

### Check Worker Logs

```bash
# Monitor all workers
./scripts/tail-logs.sh

# Or individually
wrangler tail actor --env production
wrangler tail inbox --env production
wrangler tail delivery-queue --env production
```

### Common Issues and Fixes

**Issue 1: Worker fails to start**
```
Error: Module not found
```
Fix:
```bash
cd platforms/cloudflare/workers/<worker-name>
cargo clean
cargo build --target wasm32-unknown-unknown --release
wrangler deploy --env production
```

**Issue 2: Database errors**
```
Error: D1_ERROR: no such table
```
Fix:
```bash
# Check database binding in wrangler.toml
grep -A 3 "d1_databases" platforms/cloudflare/workers/<worker-name>/wrangler.toml

# Verify database ID matches
wrangler d1 list
```

**Issue 3: CORS errors**
```
Access-Control-Allow-Origin missing
```
Fix: Router worker handles CORS. Ensure router is deployed last.

**Issue 4: Old worker still responding**
Fix:
```bash
# Force deploy with new version
wrangler deploy --env production --compatibility-date 2025-01-04
```

## Step 10: Cleanup

### Remove Old Files (Optional)

After confirming v1.1 works:

```bash
# Remove old worker directories
rm -rf workers/

# Keep backup
mkdir -p archive-v1.0
mv backup-v1.0 archive-v1.0/
```

### Update DNS (if needed)

No DNS changes required. v1.1 uses the same endpoints as v1.0.

## Rollback Procedure

If you need to rollback to v1.0:

### Quick Rollback

```bash
# 1. Checkout v1.0 branch
git checkout main  # or your v1.0 branch

# 2. Redeploy v1.0 workers
cd workers/actor && wrangler deploy --env production
cd ../inbox && wrangler deploy --env production
# etc...

# 3. Restore database (if needed)
wrangler d1 execute DB --file=backup-v1.0.sql --remote
```

### Detailed Rollback

1. **Stop new deployments**:
```bash
# Cancel any in-progress deployments
wrangler deployments list
```

2. **Restore previous deployment**:
```bash
# Get previous deployment ID
wrangler deployments list --name actor

# Rollback to specific deployment
wrangler rollback <deployment-id> --env production
```

3. **Verify rollback**:
```bash
./scripts/verify-deployment.sh
```

## Performance Comparison

### v1.0 vs v1.1 Metrics

| Metric | v1.0 | v1.1 | Change |
|--------|------|------|--------|
| Worker startup time | ~50ms | ~45ms | 10% faster |
| Database query time | ~15ms | ~15ms | Same |
| Memory usage (avg) | 12 MB | 10 MB | 17% reduction |
| Code size (total) | ~15,000 LOC | ~6,000 LOC | 60% reduction |
| Build time (all workers) | ~3 min | ~1.5 min | 50% faster |

### Code Reusability

| Aspect | v1.0 | v1.1 |
|--------|------|------|
| Platform support | Cloudflare only | Multi-platform ready |
| Code reuse | 0% | 85-90% |
| Time to add platform | 6-8 weeks | 2-3 weeks |
| Maintainability | Update 9 workers | Update core once |

## Frequently Asked Questions

### Q: Do I need to migrate my database?

**A**: No. v1.1 uses the same database schema as v1.0. Your existing D1 database works without changes.

### Q: Will federation break during upgrade?

**A**: No. ActivityPub protocol is unchanged. Other instances will continue federating normally.

### Q: Can I upgrade one worker at a time?

**A**: Yes! Deploy workers gradually. v1.1 workers are compatible with v1.0 database.

### Q: What if I use custom themes?

**A**: Custom themes in `workers/*/themes/` need to be moved to `platforms/cloudflare/workers/*/themes/`. HTML structure is unchanged.

### Q: Do I need to update my admin interface?

**A**: If you use a custom admin interface, API endpoints remain the same. No changes needed.

### Q: How long does the upgrade take?

**A**:
- Preparation: 30 minutes
- Compilation: 5-10 minutes
- Deployment: 15-20 minutes
- Verification: 10 minutes
- **Total**: ~1 hour for standard setup

### Q: Is there downtime?

**A**: Minimal. Each worker deployment takes ~5 seconds. Deploy during low-traffic period for zero user impact.

### Q: Can I test before deploying to production?

**A**: Yes! Use the local development environment:
```bash
./scripts/dev-start.sh
./scripts/seed-local-db.sh
# Test locally before deploying
```

## Getting Help

### Resources

- **Documentation**: `ARCHITECTURE_v1.1.md` - Architecture details
- **Testing**: `TESTING_v1.1.md` - Testing procedures
- **Deployment**: `DEPLOYMENT.md` - Fresh deployment guide

### Troubleshooting

1. **Check compilation**:
```bash
./scripts/test-workers.sh
```

2. **Check deployment status**:
```bash
wrangler deployments list --name <worker>
```

3. **Check logs**:
```bash
wrangler tail <worker> --env production
```

4. **Verify endpoints**:
```bash
./scripts/verify-deployment.sh
```

### Community Support

- **GitHub Issues**: https://github.com/daisocial/dais/issues
- **Discussions**: https://github.com/daisocial/dais/discussions
- **Matrix**: #dais:matrix.org

## Conclusion

The v1.0 to v1.1 upgrade:

✅ **Is safe** - Same database schema, backward compatible
✅ **Is fast** - ~1 hour with minimal downtime
✅ **Is reversible** - Easy rollback if needed
✅ **Adds value** - Multi-platform support, better maintainability

**Benefits after upgrade**:
- 60% less code to maintain
- Multi-platform architecture ready
- Faster build times
- Better separation of concerns
- Foundation for future platforms (Vercel, Netlify, etc.)

Welcome to dais v1.1!
