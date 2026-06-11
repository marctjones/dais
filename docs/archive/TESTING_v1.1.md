# Testing Guide for v1.1 Refactored Architecture

This guide covers testing the refactored multi-platform architecture.

## Quick Verification

### 1. Compile All Workers

```bash
./scripts/test-workers.sh
```

This verifies that all 9 workers and the core library compile successfully.

### 2. Verify Deployment (if deployed)

```bash
./scripts/verify-deployment.sh
```

This tests live endpoints to ensure workers are responding correctly.

## Unit Tests

### Core Library Tests

```bash
cd core
cargo test
```

Tests for:
- SQL abstraction layer (SQLite, Postgres, MySQL)
- Schema builder
- Migration runner
- Database query portability

### Platform Bindings Tests

```bash
cd platforms/cloudflare/bindings
cargo test
```

Tests for:
- D1 provider
- HTTP provider
- Queue provider

## Integration Tests

### Local Development Testing

1. Start local development environment:
```bash
./scripts/dev-start.sh
```

2. Seed local database:
```bash
./scripts/seed-local-db.sh
```

3. Test WebFinger:
```bash
curl http://localhost:8787/.well-known/webfinger?resource=acct:social@localhost
```

4. Test Actor endpoint:
```bash
curl -H "Accept: application/activity+json" http://localhost:8788/users/social
```

### Federation Testing

Test with a live Mastodon instance:

1. **Follow Test**:
   - From Mastodon, follow `@social@social.dais.social`
   - Check inbox worker logs: `wrangler tail inbox`
   - Verify follower appears in database

2. **Post Delivery Test**:
   - Create a post via API
   - Check delivery queue: `wrangler tail delivery-queue`
   - Verify post appears on Mastodon timeline

3. **Reply Test**:
   - Reply to a dais post from Mastodon
   - Check inbox worker logs
   - Verify reply is stored correctly

## Performance Testing

### Database Query Performance

Test query performance across dialects:

```rust
use dais_core::sql::convert_placeholders;
use dais_core::traits::DatabaseDialect;

// SQLite query
let sqlite_query = "SELECT * FROM posts WHERE id = ?1";
println!("{}", convert_placeholders(sqlite_query, DatabaseDialect::SQLite));

// Postgres query
println!("{}", convert_placeholders(sqlite_query, DatabaseDialect::PostgreSQL));
// Output: SELECT * FROM posts WHERE id = $1
```

### Migration Performance

Test migration speed:

```bash
time wrangler d1 execute DB --file=cli/migrations/001_initial_schema.sql
```

## Platform Portability Tests

### SQLite (Cloudflare D1)

```bash
cd platforms/cloudflare/workers/inbox
cargo build --target wasm32-unknown-unknown
```

### PostgreSQL (Future - Vercel)

When Vercel bindings are complete:

```bash
cd platforms/vercel/workers/inbox
cargo build --target wasm32-unknown-unknown
```

## Debugging

### View Worker Logs

```bash
# Specific worker
wrangler tail inbox

# All workers (in separate terminals)
./scripts/tail-logs.sh
```

### Inspect Database

```bash
# D1 database
wrangler d1 execute DB --command "SELECT * FROM posts LIMIT 10"

# Check migrations
wrangler d1 execute DB --command "SELECT * FROM schema_migrations"
```

### Test SQL Portability

```rust
use dais_core::sql::SchemaBuilder;
use dais_core::sql::schema::{ColumnDef, ColumnType};
use dais_core::traits::DatabaseDialect;

// SQLite
let builder = SchemaBuilder::new(DatabaseDialect::SQLite);
let columns = vec![
    ColumnDef::new("id", ColumnType::Integer).auto_increment(),
    ColumnDef::new("name", ColumnType::Text).not_null(),
];
println!("{}", builder.create_table("users", &columns));

// PostgreSQL
let builder = SchemaBuilder::new(DatabaseDialect::PostgreSQL);
println!("{}", builder.create_table("users", &columns));
```

## Automated Testing Checklist

- [ ] All workers compile without errors
- [ ] Core library tests pass
- [ ] Platform bindings tests pass
- [ ] WebFinger responds correctly
- [ ] Actor endpoint returns valid JSON
- [ ] Inbox accepts signed activities
- [ ] Outbox returns posts collection
- [ ] Delivery queue processes jobs
- [ ] Auth handles login/logout
- [ ] PDS responds to AT Protocol queries
- [ ] Router forwards requests correctly
- [ ] Landing page loads

## Federation Testing Checklist

- [ ] Can follow from Mastodon
- [ ] Posts appear on Mastodon
- [ ] Replies work bidirectionally
- [ ] Likes federate correctly
- [ ] Boosts federate correctly
- [ ] HTTP signatures verify
- [ ] Deliveries succeed
- [ ] Retry logic works

## Next Steps

After all tests pass:
1. Deploy to production
2. Monitor for errors
3. Test with real users
4. Performance profiling
5. Add Vercel platform support (v1.2)
