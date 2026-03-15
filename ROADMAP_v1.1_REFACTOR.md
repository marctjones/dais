# Roadmap: v1.1 - Multi-Platform Architecture Refactor

**Target Release**: v1.1.0 (Cloudflare only, refactored architecture)
**Follow-up**: v1.2.0 (Add Vercel platform support)
**Timeline**: 6-8 weeks

## Strategy

Instead of quick-and-dirty platform ports, **build the proper multi-platform architecture first**, validate it with Cloudflare, then add platforms incrementally.

### Why This Approach?

- ✅ **Proper abstraction layer** - Build it once, use it forever
- ✅ **Test on known platform** - Cloudflare already works, validate refactor doesn't break anything
- ✅ **Easier platform additions** - v1.2 Vercel, v1.3 Netlify, etc.
- ✅ **Rust+WASM foundation** - Core logic stays in Rust, compiles to WASM
- ✅ **No code duplication** - Shared WASM module across all platforms

### Release Plan

```
v1.0.0 (current)  - Cloudflare native implementation
       ↓
v1.1.0 (refactor) - Multi-platform architecture (Cloudflare only)
       ↓
v1.2.0 (expand)   - Add Vercel platform support
       ↓
v1.3.0 (expand)   - Add Netlify/Railway/others
```

## Architecture Goals

### Current Architecture (v1.0.0)

```
workers/
├── actor/         - Rust worker, tightly coupled to Cloudflare APIs
├── inbox/         - Rust worker, D1-specific SQL
├── outbox/        - Rust worker, R2-specific storage
├── webfinger/     - Rust worker, Workers KV
└── delivery-queue/ - Rust worker, Cloudflare Queues
```

**Problem**: All workers directly call `cloudflare_*` APIs, can't run on other platforms.

### Target Architecture (v1.1.0)

```
dais/
├── core/                    # NEW - Rust core library
│   ├── lib.rs               # WASM exports
│   ├── activitypub/         # Protocol logic (platform-agnostic)
│   ├── atproto/             # AT Protocol logic
│   └── traits/              # Platform abstraction traits
│       ├── database.rs      # DatabaseProvider trait
│       ├── storage.rs       # StorageProvider trait
│       ├── queue.rs         # QueueProvider trait
│       └── http.rs          # HttpProvider trait
│
├── platforms/
│   └── cloudflare/          # Cloudflare platform implementation
│       ├── bindings/        # Rust bindings to Cloudflare APIs
│       │   ├── d1.rs        # impl DatabaseProvider for D1
│       │   ├── r2.rs        # impl StorageProvider for R2
│       │   └── queues.rs    # impl QueueProvider for Queues
│       │
│       └── workers/         # Thin worker shims
│           ├── actor/       # Calls core WASM + platform bindings
│           ├── inbox/
│           └── outbox/
│
└── cli/                     # Python CLI (unchanged)
```

### Core Abstraction Traits

```rust
// core/traits/database.rs
#[async_trait(?Send)]
pub trait DatabaseProvider {
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>>;
    async fn batch(&self, statements: Vec<Statement>) -> Result<()>;
    async fn transaction<F>(&self, f: F) -> Result<()>;
}

// core/traits/storage.rs
#[async_trait(?Send)]
pub trait StorageProvider {
    async fn put(&self, key: &str, data: Vec<u8>, content_type: &str) -> Result<String>;
    async fn get(&self, key: &str) -> Result<Vec<u8>>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
}

// core/traits/queue.rs
#[async_trait(?Send)]
pub trait QueueProvider {
    async fn send(&self, message: &str) -> Result<()>;
    async fn batch(&self, messages: Vec<String>) -> Result<()>;
}

// core/traits/http.rs
#[async_trait(?Send)]
pub trait HttpProvider {
    async fn fetch(&self, url: &str, options: RequestOptions) -> Result<Response>;
}
```

### Platform Implementation (Cloudflare)

```rust
// platforms/cloudflare/bindings/d1.rs
use dais_core::traits::DatabaseProvider;
use worker::*;

pub struct D1Provider {
    db: D1Database,
}

#[async_trait(?Send)]
impl DatabaseProvider for D1Provider {
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>> {
        self.db
            .prepare(sql)
            .bind(&params)?
            .all()
            .await?
            .results()
    }
    // ... other methods
}
```

### WASM Core Module

```rust
// core/lib.rs
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct ActivityPubCore {
    db: Box<dyn DatabaseProvider>,
    storage: Box<dyn StorageProvider>,
    queue: Box<dyn QueueProvider>,
}

#[wasm_bindgen]
impl ActivityPubCore {
    // Exported to WASM, callable from any platform
    pub async fn handle_inbox(&self, actor: &str, activity: &str) -> Result<JsValue, JsValue> {
        // Platform-agnostic logic
        let activity = parse_activity(activity)?;

        // Uses trait methods (works on any platform)
        self.db.execute("INSERT INTO inbox ...", params).await?;
        self.queue.send(&delivery_message).await?;

        Ok(JsValue::from_str("OK"))
    }

    pub async fn create_post(&self, content: &str) -> Result<JsValue, JsValue> {
        // Platform-agnostic logic
    }
}
```

### Worker Shim (Cloudflare)

```rust
// platforms/cloudflare/workers/inbox/src/lib.rs
use worker::*;
use dais_core::ActivityPubCore;
use dais_cloudflare::bindings::*;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // Create platform-specific providers
    let db = D1Provider::new(env.d1("DB")?);
    let storage = R2Provider::new(env.r2("MEDIA")?);
    let queue = QueueProvider::new(env.queue("delivery")?);

    // Initialize WASM core with providers
    let core = ActivityPubCore::new(
        Box::new(db),
        Box::new(storage),
        Box::new(queue),
    );

    // Call core logic (platform-agnostic)
    let result = core.handle_inbox(&actor, &body).await?;

    Response::ok(result)
}
```

## Implementation Phases

### Phase 1: Core Abstraction Layer (Week 1-2)

**Goal**: Define platform traits and create core library

- [ ] Create `dais-core/` workspace
- [ ] Define `DatabaseProvider` trait
- [ ] Define `StorageProvider` trait
- [ ] Define `QueueProvider` trait
- [ ] Define `HttpProvider` trait
- [ ] Move ActivityPub logic to core (platform-agnostic)
- [ ] Move AT Protocol logic to core (platform-agnostic)
- [ ] Export WASM bindings with `wasm-bindgen`

**Files**:
```
core/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── activitypub/
│   │   ├── mod.rs
│   │   ├── inbox.rs
│   │   ├── outbox.rs
│   │   └── delivery.rs
│   ├── atproto/
│   │   ├── mod.rs
│   │   └── sync.rs
│   └── traits/
│       ├── mod.rs
│       ├── database.rs
│       ├── storage.rs
│       ├── queue.rs
│       └── http.rs
```

### Phase 2: Cloudflare Platform Bindings (Week 2-3)

**Goal**: Implement traits for Cloudflare services

- [ ] Create `platforms/cloudflare/bindings/`
- [ ] Implement `D1Provider` (DatabaseProvider)
- [ ] Implement `R2Provider` (StorageProvider)
- [ ] Implement `CloudflareQueueProvider` (QueueProvider)
- [ ] Implement `WorkerHttpProvider` (HttpProvider)
- [ ] Test each binding independently

**Files**:
```
platforms/cloudflare/
├── Cargo.toml
└── bindings/
    ├── mod.rs
    ├── d1.rs
    ├── r2.rs
    ├── queues.rs
    └── http.rs
```

### Phase 3: Migrate Existing Workers (Week 3-5)

**Goal**: Refactor current workers to use core WASM + platform bindings

- [ ] Refactor `actor` worker to use core
- [ ] Refactor `inbox` worker to use core
- [ ] Refactor `outbox` worker to use core
- [ ] Refactor `webfinger` worker to use core
- [ ] Refactor `delivery-queue` worker to use core
- [ ] Refactor `pds` worker to use core

**Pattern for each worker**:
1. Strip out business logic (move to core)
2. Keep only platform setup + core invocation
3. Test against production (feature parity)

### Phase 4: Database Schema Abstraction (Week 5-6)

**Goal**: Make SQL queries portable across databases

**Current**: Direct D1 SQL queries
```rust
db.prepare("INSERT INTO posts (id, content) VALUES (?1, ?2)")
```

**Target**: Query builder or portable SQL
```rust
// Option A: Query builder
db.insert("posts")
    .values(&[("id", id), ("content", content)])
    .execute()
    .await?

// Option B: Portable SQL with migrations
// Detect database type and use appropriate SQL dialect
match db.dialect() {
    Dialect::SQLite => "INSERT INTO posts ...",
    Dialect::Postgres => "INSERT INTO posts ... RETURNING id",
}
```

**Tasks**:
- [ ] Audit all SQL queries in codebase
- [ ] Identify Cloudflare-specific SQL (SQLite dialect)
- [ ] Create query abstraction layer or migration strategy
- [ ] Test on D1 (SQLite)

### Phase 5: Testing & Validation (Week 6-7)

**Goal**: Ensure refactored architecture works identically to v1.0

- [ ] Deploy refactored workers to Cloudflare staging
- [ ] Test ActivityPub federation (follow, post, reply)
- [ ] Test AT Protocol sync (Bluesky integration)
- [ ] Test media uploads (R2 storage)
- [ ] Test delivery queue (background jobs)
- [ ] Run integration tests against production
- [ ] Performance comparison (v1.0 vs v1.1)

**Success Criteria**:
- All v1.0 features work identically
- No performance regression (<10% overhead acceptable)
- Cleaner codebase (less Cloudflare-specific code in workers)

### Phase 6: Documentation (Week 7-8)

**Goal**: Document new architecture for platform developers

- [ ] Write `ARCHITECTURE.md` (trait system, WASM core)
- [ ] Write `ADDING_PLATFORMS.md` (guide for v1.2 Vercel)
- [ ] Update `CONTRIBUTING.md` (new project structure)
- [ ] Update CLI documentation (no user-facing changes)
- [ ] Create platform provider interface documentation

### Phase 7: Release v1.1.0 (Week 8)

- [ ] Tag v1.1.0
- [ ] Create release notes highlighting refactor
- [ ] Note: "Cloudflare-only release, foundation for multi-platform"
- [ ] Push to GitHub

## v1.2.0 Preview: Adding Vercel

With v1.1 architecture, adding Vercel becomes:

```rust
// platforms/vercel/bindings/neon.rs
use dais_core::traits::DatabaseProvider;

pub struct NeonProvider {
    client: tokio_postgres::Client,
}

#[async_trait(?Send)]
impl DatabaseProvider for NeonProvider {
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>> {
        self.client.query(sql, params).await?
    }
}
```

```typescript
// platforms/vercel/api/inbox/route.ts
import { ActivityPubCore } from '@dais/core';  // WASM module
import { NeonProvider, BlobProvider } from '@dais/vercel';

export async function POST(req: Request) {
    const db = new NeonProvider(process.env.DATABASE_URL);
    const storage = new BlobProvider();

    const core = new ActivityPubCore(db, storage);
    const result = await core.handleInbox(actor, body);

    return Response.json(result);
}
```

**Effort for v1.2**: 2-3 weeks (just platform bindings, core already done)

## Success Metrics

- [ ] All v1.0 features work on refactored v1.1
- [ ] Workers use shared WASM core (no duplicated logic)
- [ ] Platform traits cleanly separate concerns
- [ ] Adding Vercel in v1.2 requires <500 LOC (just bindings)
- [ ] Performance overhead <10%
- [ ] Codebase is more maintainable

## Technical Decisions

### Language Choices

- **Core logic**: Rust (compiles to WASM)
- **Cloudflare bindings**: Rust (worker-rs library)
- **Future Vercel bindings**: TypeScript (Node.js)
- **CLI**: Python (unchanged)

### Why Rust Core?

- ✅ Compiles to WASM (runs anywhere)
- ✅ Type safety for protocol logic
- ✅ Performance (critical for ActivityPub delivery)
- ✅ Already used in v1.0 workers

### Why Not Full Rust Everywhere?

- ❌ Vercel doesn't support Rust workers
- ❌ Each platform has different runtime (Workers, Node.js, Deno)
- ✅ Solution: Rust core → WASM, thin platform-specific shims

### Database Portability Strategy

**Challenge**: SQLite (D1) vs Postgres (Neon) have different SQL dialects

**Options**:
1. **Query builder** - Programmatic queries (portable)
2. **ORM** - Diesel/SeaORM (heavy for WASM)
3. **Dual migrations** - Maintain SQLite + Postgres schemas
4. **Lowest common denominator SQL** - Avoid platform-specific features

**Decision for v1.1**: Option 3 (dual migrations)
- Keep SQLite for Cloudflare/D1
- Add Postgres migrations in v1.2
- Use feature flags to select SQL dialect at compile time

## Breaking Changes

**User-facing**: None. v1.1 is drop-in replacement for v1.0.

**Developer-facing**:
- Workers now depend on `dais-core` WASM module
- Business logic moved from workers to core
- Workers become thin shims (100-200 LOC each)

## Migration Path

**For users**:
```bash
# Update CLI
cd cli
git pull
pip install -e .

# Deploy refactored workers
dais deploy workers
```

**For developers**:
- Update imports to use `dais-core`
- Platform-specific code goes in `platforms/{platform}/bindings/`
- Business logic goes in `core/src/`

## Risks & Mitigation

### Risk 1: WASM Performance Overhead
**Mitigation**: Benchmark early, optimize hot paths, accept <10% overhead

### Risk 2: Complex Trait Design
**Mitigation**: Start simple, iterate, test with Cloudflare first

### Risk 3: Breaking Existing Deployments
**Mitigation**: Extensive testing, staged rollout, keep v1.0 workers as fallback

### Risk 4: WASM Binary Size
**Mitigation**: Use `wasm-opt`, strip debug symbols, lazy loading

## Timeline

| Week | Phase | Deliverable |
|------|-------|-------------|
| 1-2  | Core Abstraction | Traits + WASM exports |
| 2-3  | Cloudflare Bindings | Platform implementations |
| 3-5  | Worker Migration | Refactored workers |
| 5-6  | Database Abstraction | Portable queries |
| 6-7  | Testing | Validation against v1.0 |
| 7-8  | Documentation | Architecture docs |
| 8    | Release | v1.1.0 tag |

**Total**: 6-8 weeks

## Open Questions

1. **WASM module size**: How large will core WASM be? (Target: <500KB)
2. **Async trait performance**: Does `async_trait` macro add overhead?
3. **Database migrations**: Generate from Rust structs or hand-write SQL?
4. **Error handling**: How to pass platform errors through trait boundary?
5. **Testing strategy**: Unit tests for traits, integration tests for platforms?

## Next Steps (After v1.1 Release)

1. **v1.2.0**: Add Vercel platform support (2-3 weeks)
2. **v1.3.0**: Add Netlify/Railway support
3. **v2.0.0**: Web-based setup wizard, managed hosting exploration
4. **v3.0.0**: Multi-user support (if requested)

## Related Documents

- `ROADMAP_v1.1.md` - Original plan (Vercel-first approach, superseded)
- `STRATEGY_NON_TECHNICAL_USERS.md` - Long-term managed hosting strategy
- `ARCHITECTURE.md` - Will be created in Phase 6
