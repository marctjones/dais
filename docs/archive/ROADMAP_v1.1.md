# Roadmap: v1.1 - Vercel Platform Support

Target audience: **Technical users who prefer Vercel over Cloudflare**

## Goals

1. Support Vercel as a **second deployment platform** (in addition to Cloudflare)
2. Maintain **feature parity** where possible
3. Provide **one-click Vercel deploy** button
4. Keep **shared CLI** for both platforms

## Timeline

**Target**: 4-6 weeks from v1.0.0 release (mid-April 2026)

## Architecture Changes

### Minimal Refactor (v1.1 approach)

Instead of full Rust+WASM abstraction (save for v2.0), use **platform-specific implementations**:

```
dais/
├── platforms/
│   ├── cloudflare/         # Current implementation
│   │   ├── workers/        # Existing Rust workers
│   │   └── deployment/     # wrangler configs
│   │
│   └── vercel/             # New Vercel implementation
│       ├── api/            # Next.js API routes (TypeScript)
│       ├── lib/            # Shared logic (ported from Rust)
│       └── vercel.json     # Vercel configuration
│
└── cli/
    └── dais_cli/
        ├── platforms/      # Platform adapters
        │   ├── cloudflare.py
        │   └── vercel.py
        └── commands/
            └── deploy.py   # Enhanced with --platform flag
```

**Strategy**: Port Rust worker logic to TypeScript for Vercel. No shared WASM (yet).

## Feature Comparison

| Feature | Cloudflare v1.0 | Vercel v1.1 | Notes |
|---------|----------------|-------------|-------|
| **ActivityPub** | ✅ Full | ✅ Full | Core protocol |
| **AT Protocol** | ✅ Full with WebSockets | ⚠️ Limited | No native WebSocket support |
| **Posts** | ✅ | ✅ | |
| **Media** | ✅ R2 | ✅ Vercel Blob | |
| **Database** | ✅ D1 (SQLite) | ✅ Neon Postgres | Schema changes needed |
| **Auth** | ✅ Cloudflare Access | ✅ Vercel Auth | Different IdP setup |
| **Queue** | ✅ Cloudflare Queues | ✅ QStash or Inngest | External service |
| **Cost (free tier)** | ✅ $0 | ⚠️ Limited | 100GB bandwidth |
| **Cost (heavy use)** | ✅ ~$5/mo | ❌ ~$80/mo | Bandwidth costs |
| **Deployment** | ✅ CLI + wrangler | ✅ CLI + vercel | |
| **One-click deploy** | ❌ | ✅ | Vercel advantage |

**Verdict**: Vercel v1.1 will have **95% feature parity**, with AT Protocol WebSocket as known limitation.

## Implementation Phases

### Phase 1: Database Adapter (Week 1)
- [ ] Port D1 SQLite schema to Postgres
- [ ] Create migration scripts
- [ ] Update SQL queries for Postgres syntax
- [ ] Test with Neon Postgres

**Files**:
- `platforms/vercel/lib/db.ts` - Neon Postgres adapter
- `platforms/vercel/migrations/` - Postgres migrations

### Phase 2: API Routes (Week 2)
- [ ] Port webfinger worker → `api/.well-known/webfinger/route.ts`
- [ ] Port actor worker → `api/users/[username]/route.ts`
- [ ] Port inbox worker → `api/users/[username]/inbox/route.ts`
- [ ] Port outbox worker → `api/users/[username]/outbox/route.ts`
- [ ] Port auth worker → `api/auth/[...route]/route.ts`

**Files**:
- `platforms/vercel/api/` - All API routes

### Phase 3: Media Storage (Week 3)
- [ ] Integrate Vercel Blob storage
- [ ] Update media upload logic
- [ ] Test image/video uploads
- [ ] CDN configuration

**Files**:
- `platforms/vercel/lib/storage.ts` - Vercel Blob adapter

### Phase 4: Background Jobs (Week 3-4)
- [ ] Choose queue provider (QStash recommended)
- [ ] Implement delivery queue
- [ ] Test ActivityPub delivery
- [ ] Retry logic and error handling

**Files**:
- `platforms/vercel/lib/queue.ts` - QStash adapter

### Phase 5: CLI Integration (Week 4)
- [ ] Add `--platform vercel` flag to all commands
- [ ] Update `dais deploy` for Vercel
- [ ] Create `dais init --platform vercel`
- [ ] Update configuration schema

**Files**:
- `cli/dais_cli/platforms/vercel.py`
- `cli/dais_cli/commands/deploy.py`

### Phase 6: Deployment Template (Week 5)
- [ ] Create vercel.json configuration
- [ ] Add environment variable template
- [ ] Create deploy button (`[![Deploy with Vercel]...`)
- [ ] Test one-click deploy flow

**Files**:
- `platforms/vercel/vercel.json`
- `platforms/vercel/.env.template`
- `README.md` - Add deploy button

### Phase 7: Documentation (Week 6)
- [ ] Write VERCEL_DEPLOYMENT.md
- [ ] Update README with Vercel option
- [ ] Create comparison table (Cloudflare vs Vercel)
- [ ] Migration guide (Cloudflare → Vercel)

**Files**:
- `VERCEL_DEPLOYMENT.md`
- `PLATFORM_COMPARISON.md`

## CLI Enhancements

### New Commands

```bash
# Initialize for specific platform
dais init --platform cloudflare  # Existing
dais init --platform vercel      # New

# Deploy to specific platform
dais deploy --platform cloudflare
dais deploy --platform vercel

# Platform-specific config
dais config set platform vercel
dais config get platform

# Migration helper
dais migrate --from cloudflare --to vercel
```

### Configuration Schema

```toml
[platform]
default = "cloudflare"  # or "vercel"

[cloudflare]
# Existing config...

[vercel]
project_id = ""
team_id = ""
api_token = ""
neon_database_url = ""
blob_storage_token = ""
qstash_token = ""
```

## Testing Strategy

### Unit Tests
- Test each API route independently
- Test database adapters
- Test storage adapter
- Test queue adapter

### Integration Tests
- Test full post creation flow
- Test ActivityPub federation
- Test media uploads
- Test delivery queue

### Platform Parity Tests
- Compare Cloudflare vs Vercel behavior
- Ensure ActivityPub compliance on both
- Test cross-platform migration

## Known Limitations (Vercel v1.1)

### 1. AT Protocol WebSockets
**Problem**: Vercel doesn't support WebSockets natively

**Solutions**:
- **Option A**: Skip AT Protocol on Vercel (v1.1)
- **Option B**: Use external WebSocket service (Pusher/Ably)
- **Option C**: Use Rivet for Vercel Functions (experimental)

**Decision for v1.1**: Skip AT Protocol, document as Cloudflare-only feature.

### 2. Cost at Scale
**Problem**: Vercel bandwidth costs are high

**Solution**: Document cost expectations clearly. Recommend Cloudflare for high-traffic accounts.

### 3. Rust Workers
**Problem**: Can't use existing Rust code on Vercel

**Solution**: Port to TypeScript. Save unified WASM approach for v2.0.

## Success Metrics

- [ ] One-click Vercel deploy takes <10 minutes
- [ ] All core features work (posts, followers, media)
- [ ] Passes ActivityPub compliance tests
- [ ] Documentation is clear for non-Cloudflare users
- [ ] At least 3 beta testers successfully deploy

## Migration Path to v2.0

v1.1 is **intentionally simple** to ship fast. For v2.0, we'll refactor:

1. **Rust+WASM core** - Write once, run anywhere
2. **Platform abstraction** - Traits for DB, storage, queue
3. **More platforms** - Netlify, Deno Deploy, Railway
4. **Unified codebase** - No duplicated logic

But v1.1 gets us:
- ✅ Proof of concept for multi-platform
- ✅ Feedback from Vercel users
- ✅ Revenue (managed hosting insights)
- ✅ Faster time to market

## Open Questions

1. **Database migration**: How easy is Cloudflare → Vercel migration?
2. **Cost transparency**: Should we warn users about Vercel costs?
3. **Feature gaps**: Is skipping AT Protocol acceptable for v1.1?
4. **Support burden**: Can we support two platforms well?

## Resources Needed

- **Development time**: 4-6 weeks part-time
- **Testing**: 1 week with beta users
- **Services**: Neon Postgres (free tier), QStash (free tier)
- **Documentation**: 1 week writing/updating docs

## Release Plan

1. **Week 1-4**: Development (phases 1-4)
2. **Week 5**: Integration + CLI (phases 5-6)
3. **Week 6**: Beta testing with 3-5 users
4. **Week 7**: Documentation + polish
5. **Week 8**: Release v1.1.0

## Alternative: Skip v1.1, Go to v2.0

**Instead of** quick Vercel port, go directly to proper multi-platform architecture:

**Pros**:
- Better long-term architecture
- No duplicated code
- Supports unlimited platforms

**Cons**:
- Takes 2-3 months instead of 1-2 months
- More complex
- No quick wins

**Recommendation**: Do v1.1 for fast feedback, then v2.0 for proper architecture.
