# Development vs Production Environments

Guide to managing development and production environments in dais.

## Overview

dais supports two environments:
- **Production** - Your live @social@dais.social instance
- **Development** - Local testing with `localhost` and seed data

## Default Behavior

By default, **all commands use the production environment** for safety. This prevents accidentally posting to development when you meant production.

## Configuration

### Set Default Environment

```bash
# Set production as default (recommended)
dais config set environment.default production

# Set development as default (for active development)
dais config set environment.default development

# Check current setting
dais config list | grep environment
```

### Check Current Environment

When you run commands, dais will show which environment is being used:

```bash
dais post create "Hello!"
# Output shows: Environment: [production]
```

## Per-Command Override

You can override the default environment for any command:

### Use Production (explicit)

```bash
dais post create "Hello!" --env production
```

### Use Development (explicit)

```bash
dais post create "Testing..." --env development
```

### Backward Compatibility

The `--remote` flag still works (deprecated):

```bash
# Old way (still works)
dais post create "Hello!" --remote

# New way (preferred)
dais post create "Hello!" --env production
```

## Environment Differences

### Production Environment
- **Domain**: Uses `server.activitypub_domain` from config (e.g., `social.dais.social`)
- **Username**: Uses `server.username` from config (e.g., `social`)
- **Database**: Cloudflare D1 production database
- **Delivery**: Posts federate to real followers
- **Bluesky**: Posts to production AT Protocol PDS

### Development Environment
- **Domain**: Uses `localhost` (local wrangler dev)
- **Username**: Uses `marc` (matches seed-local-db.sh)
- **Database**: Local SQLite database
- **Delivery**: Attempts delivery to seed followers (will fail unless local)
- **Bluesky**: Posts to local PDS (localhost:8791)

## Common Workflows

### Daily Production Use

With `environment.default = production` (default):

```bash
# Posts directly to production
dais post create "My post"

# All these use production by default
dais followers list
dais notifications list
dais stats
```

### Local Development & Testing

```bash
# Start local development environment
./scripts/dev-start.sh
./scripts/seed-local-db.sh

# Temporarily set dev as default
dais config set environment.default development

# Test locally
dais post create "Test post"
dais followers list

# Switch back to production
dais config set environment.default production
```

### Mixed Use (Override Per-Command)

```bash
# Default is production, but test one thing locally
dais post create "Testing new feature" --env development

# Then post announcement to production
dais post create "Feature released!" --env production
```

## Safety Tips

### ✅ Recommended: Production Default

Set production as default to avoid mistakes:

```bash
dais config set environment.default production
```

This means:
- Plain `dais post create "..."` goes to production
- Intentionally use `--env development` for testing
- Less chance of posting test messages publicly by mistake

### ⚠️ Alternative: Development Default

If you're actively developing, you might prefer:

```bash
dais config set environment.default development
```

But remember:
- Plain commands now post to localhost
- Must explicitly use `--env production` for real posts
- Higher risk of forgetting and not posting publicly

## Environment-Specific Commands

Some commands behave differently per environment:

### `dais post create`
- **Production**: Posts to real followers, federates across Fediverse
- **Development**: Saves to local DB, attempts local delivery

### `dais followers list`
- **Production**: Shows real followers from D1 database
- **Development**: Shows seed followers (Alice, Bob)

### `dais deploy`
- Always uses production (Cloudflare Workers)
- Not affected by environment.default

### `dais test`
- Has its own `--local` flag for testing local endpoints
- Independent of environment.default

## Troubleshooting

### "My posts aren't appearing on Fediverse"

Check your environment:

```bash
dais config list | grep environment
```

If it says `development`, switch to production:

```bash
dais config set environment.default production
```

### "I accidentally posted to production!"

You can delete the post:

```bash
dais post list
dais post delete <post-id>
```

### "Where did my post go?"

Check which environment you used:

```bash
# Production posts
dais post list --env production

# Development posts (local DB)
dais post list --env development
```

## Migration from Old Behavior

### Before (v1.0.0 and earlier)

```bash
# Default was localhost
dais post create "Hello"  # → went to localhost

# Had to use --remote for production
dais post create "Hello" --remote  # → went to production
```

### After (v1.0.1+)

```bash
# Default is now production
dais post create "Hello"  # → goes to production

# Use --env for development
dais post create "Hello" --env development  # → goes to localhost

# --remote still works (deprecated)
dais post create "Hello" --remote  # → goes to production
```

## Best Practices

1. **Set production as default** unless actively developing
2. **Check environment** before posting important updates
3. **Use explicit --env flag** when switching contexts
4. **Test locally first** with --env development
5. **Don't mix environments** in the same workflow

## Summary

| Command | Environment | Destination |
|---------|-------------|-------------|
| `dais post create "..."` | Uses default (production) | social.dais.social |
| `dais post create "..." --env production` | Production (explicit) | social.dais.social |
| `dais post create "..." --env development` | Development (explicit) | localhost |
| `dais post create "..." --remote` | Production (deprecated) | social.dais.social |

**Default**: Set with `dais config set environment.default production`
**Override**: Use `--env production` or `--env development` on any command
