# dais Architecture Guide

**Status:** Cloudflare-only production architecture. Provider traits remain for
testability and clean module boundaries, not for alternate hosting targets.

## Table of Contents

1. [Overview](#overview)
2. [Cloudflare Architecture](#cloudflare-architecture)
3. [Core Abstraction Layer](#core-abstraction-layer)
4. [Cloudflare Bindings](#cloudflare-bindings)
5. [Database Abstraction](#database-abstraction)
6. [Worker Pattern](#worker-pattern)
7. [Testing With Provider Traits](#testing-with-provider-traits)
8. [Migration System](#migration-system)
9. [Best Practices](#best-practices)

## Overview

dais runs on **Cloudflare Workers, D1, R2, and Queues**. The core Rust library
keeps platform traits so protocol logic can be tested without Cloudflare, but
Cloudflare is the only supported deployment target. Owner/operator workflows are
currently handled by the Rust CLI and TUI using local credentials and secrets;
there is no privileged owner web login in the active product.

**Key Benefits**:
- Shared Rust core for ActivityPub, AT Protocol, security policy, and tests.
- Thin Cloudflare worker shims for routing, request handling, and bindings.
- In-memory/mock providers for integration tests.
- Cloudflare-specific deployment and operational assumptions are explicit.

**Supported Deployment Target**:
- ✅ Cloudflare Workers (D1 SQLite)
- ❌ Other hosting platforms are not supported deployment targets.

## Cloudflare Architecture

### Three-Layer Design

```
┌─────────────────────────────────────────────┐
│         Workers (Platform Shims)            │
│  webfinger, actor, inbox, outbox, etc.      │
│         (10-15% of code)                    │
└─────────────────────────────────────────────┘
                    ▼
┌─────────────────────────────────────────────┐
│      Cloudflare Bindings                    │
│   D1Provider, CloudflareQueueProvider       │
│         (5-10% of code)                     │
└─────────────────────────────────────────────┘
                    ▼
┌─────────────────────────────────────────────┐
│     Testable Core (dais-core)               │
│  Protocol Logic, Security Policy, Tests     │
│         (85-90% of code)                    │
└─────────────────────────────────────────────┘
```

### Directory Structure

```
dais/
├── core/                       # Shared protocol and policy library
│   ├── src/
│   │   ├── types/             # ActivityPub types
│   │   ├── traits/            # Platform abstraction traits
│   │   ├── sql/               # SQL abstraction layer
│   │   ├── migrations.rs      # Migration system
│   │   ├── webfinger.rs       # WebFinger protocol
│   │   ├── inbox.rs           # Inbox processing
│   │   ├── actor.rs           # Actor profile logic
│   │   └── ...
│   └── Cargo.toml
│
├── platforms/
│   ├── cloudflare/
│   │   ├── bindings/          # Cloudflare-specific providers
│   │   │   ├── src/
│   │   │   │   ├── d1.rs      # D1Provider (SQLite)
│   │   │   │   ├── queues.rs  # CloudflareQueueProvider
│   │   │   │   └── http.rs    # WorkerHttpProvider
│   │   │   └── Cargo.toml
│   │   └── workers/           # Active workers plus legacy split workers
│   │       ├── landing/        # Active homepage worker
│   │       ├── router/         # Active protocol/API worker
│   │       ├── README.md       # Active vs legacy support boundary
│   │       └── ...             # Legacy compatibility worker sources
└── client/                    # Rust CLI/TUI client
```

## Core Abstraction Layer

### Platform Traits

The core library defines traits that each platform must implement:

#### DatabaseProvider

```rust
#[async_trait]
pub trait DatabaseProvider: Send + Sync {
    /// Execute a query that returns rows
    async fn query(&self, sql: &str, params: &[Value]) -> CoreResult<Vec<Row>>;

    /// Execute a statement (INSERT, UPDATE, DELETE)
    async fn execute(&self, sql: &str, params: &[Value]) -> CoreResult<u64>;

    /// Get the database dialect (SQLite, PostgreSQL, MySQL)
    fn dialect(&self) -> DatabaseDialect;
}
```

#### StorageProvider

```rust
#[async_trait]
pub trait StorageProvider: Send + Sync {
    /// Upload a file
    async fn put(&self, key: &str, data: &[u8]) -> CoreResult<String>;

    /// Download a file
    async fn get(&self, key: &str) -> CoreResult<Vec<u8>>;

    /// Delete a file
    async fn delete(&self, key: &str) -> CoreResult<()>;
}
```

#### QueueProvider

```rust
#[async_trait]
pub trait QueueProvider: Send + Sync {
    /// Send a message to a queue
    async fn send(&self, queue: &str, message: &QueueMessage) -> CoreResult<()>;
}
```

#### HttpProvider

```rust
#[async_trait]
pub trait HttpProvider: Send + Sync {
    /// Make an HTTP request
    async fn fetch(&self, request: HttpRequest) -> CoreResult<HttpResponse>;
}
```

### DaisCore

The main entry point for all platform-agnostic logic:

```rust
pub struct DaisCore {
    db: Box<dyn DatabaseProvider>,
    storage: Box<dyn StorageProvider>,
    queue: Box<dyn QueueProvider>,
    http: Box<dyn HttpProvider>,
}

impl DaisCore {
    pub fn new(
        db: Box<dyn DatabaseProvider>,
        storage: Box<dyn StorageProvider>,
        queue: Box<dyn QueueProvider>,
        http: Box<dyn HttpProvider>,
    ) -> Self {
        Self { db, storage, queue, http }
    }

    /// WebFinger protocol implementation
    pub async fn webfinger(&self, resource: &str) -> CoreResult<WebFingerResponse> {
        // Platform-agnostic business logic
        let username = extract_username(resource)?;
        let user = self.db.query(/* ... */).await?;
        Ok(WebFingerResponse { /* ... */ })
    }

    /// Get actor profile
    pub async fn get_actor(&self, username: &str) -> CoreResult<Person> {
        // Platform-agnostic business logic
    }

    /// Process inbox activity
    pub async fn process_inbox(&self, activity: Activity) -> CoreResult<()> {
        // Platform-agnostic business logic
    }
}
```

## Cloudflare Bindings

### Cloudflare Example

Cloudflare worker shims implement the core traits using Worker APIs.

**D1Provider** (SQLite for Cloudflare):

```rust
use dais_core::traits::{DatabaseProvider, DatabaseDialect};
use worker::D1Database;

pub struct D1Provider {
    db: D1Database,
}

impl D1Provider {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl DatabaseProvider for D1Provider {
    async fn query(&self, sql: &str, params: &[Value]) -> CoreResult<Vec<Row>> {
        let stmt = self.db.prepare(sql);
        let results = stmt.bind(params)?.all().await?;
        Ok(results.results())
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> CoreResult<u64> {
        let stmt = self.db.prepare(sql);
        let result = stmt.bind(params)?.run().await?;
        Ok(result.changes() as u64)
    }

    fn dialect(&self) -> DatabaseDialect {
        DatabaseDialect::SQLite
    }
}
```

## Database Abstraction

### SQL Dialect Support

Production uses Cloudflare D1/SQLite. The core library retains SQL dialect
helpers so tests can exercise query generation and so protocol logic remains
decoupled from Worker bindings:

```rust
pub enum DatabaseDialect {
    SQLite,     // Cloudflare D1
    PostgreSQL, // test/helper dialect
    MySQL,      // test/helper dialect
}
```

### Parameter Placeholders

Different databases use different placeholder syntax:

| Database | Placeholder Syntax | Example |
|----------|-------------------|---------|
| SQLite | `?1, ?2, ?3` | `SELECT * FROM users WHERE id = ?1` |
| PostgreSQL | `$1, $2, $3` | `SELECT * FROM users WHERE id = $1` |
| MySQL | `?, ?, ?` | `SELECT * FROM users WHERE id = ?` |

The core library automatically converts placeholders:

```rust
use dais_core::sql::convert_placeholders;

let sql = "SELECT * FROM users WHERE id = ?1 AND active = ?2";

// For SQLite (no conversion needed)
let sqlite_sql = convert_placeholders(sql, DatabaseDialect::SQLite);
// "SELECT * FROM users WHERE id = ?1 AND active = ?2"

// For PostgreSQL
let postgres_sql = convert_placeholders(sql, DatabaseDialect::PostgreSQL);
// "SELECT * FROM users WHERE id = $1 AND active = $2"

// For MySQL
let mysql_sql = convert_placeholders(sql, DatabaseDialect::MySQL);
// "SELECT * FROM users WHERE id = ? AND active = ?"
```

### Query Builder

Build portable queries using the fluent API:

```rust
use dais_core::sql::QueryBuilder;

let query = QueryBuilder::new(dialect)
    .select(&["id", "username", "created_at"])
    .from("users")
    .where_clause("active = ?1")
    .limit(10)
    .build();
```

### Schema Builder

Create portable table schemas:

```rust
use dais_core::sql::{SchemaBuilder, ColumnDef, ColumnType};

let builder = SchemaBuilder::new(dialect);
let columns = vec![
    ColumnDef::new("id", ColumnType::Integer).auto_increment(),
    ColumnDef::new("username", ColumnType::Text).not_null().unique(),
    ColumnDef::new("email", ColumnType::Text).not_null(),
    ColumnDef::new("created_at", ColumnType::Timestamp).default_now(),
];

let sql = builder.create_table("users", &columns);

// SQLite output:
// CREATE TABLE IF NOT EXISTS users (
//   id INTEGER PRIMARY KEY AUTOINCREMENT,
//   username TEXT NOT NULL UNIQUE,
//   email TEXT NOT NULL,
//   created_at TEXT DEFAULT CURRENT_TIMESTAMP
// )

// PostgreSQL output:
// CREATE TABLE IF NOT EXISTS users (
//   id SERIAL PRIMARY KEY,
//   username TEXT NOT NULL UNIQUE,
//   email TEXT NOT NULL,
//   created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
// )
```

### Type Mappings

Different databases have different type systems:

| Core Type | SQLite | PostgreSQL | MySQL |
|-----------|--------|------------|-------|
| Boolean | `INTEGER` (0/1) | `BOOLEAN` | `TINYINT(1)` |
| JSON | `TEXT` | `JSONB` | `JSON` |
| UUID | `TEXT` | `UUID` | `CHAR(36)` |
| Timestamp | `TEXT` | `TIMESTAMP` | `TIMESTAMP` |

The schema builder handles this automatically:

```rust
ColumnDef::new("settings", ColumnType::Json)
// SQLite: settings TEXT
// PostgreSQL: settings JSONB
// MySQL: settings JSON
```

## Active Worker Pattern

### Router-Centered Workers

The supported Cloudflare deployment surface is intentionally small:

- `router`: ActivityPub, ATProto/PDS, owner API, media, E2EE/MLS, and compatibility routes.
- `landing`: the project/instance homepage.

Legacy split-worker sources such as `actor`, `inbox`, `outbox`, `webfinger`,
`pds`, `auth`, and `delivery-queue` remain in-tree for compatibility and
historical reference, but the default deploy path does not publish them. Use
`platforms/cloudflare/workers/README.md` and `scripts/deploy.sh --include-legacy`
when a legacy compatibility deployment is explicitly required.

The shared Cloudflare binding crate exposes D1, Queue, and HTTP providers. Media
storage is currently implemented inside the active router worker because private
media, signed access, metadata, and encrypted attachment semantics have to be
handled together. The crate does not expose a partial `R2Provider` that would
return dummy signed URLs or hide unsupported operations.

**Example: Router Worker Provider Setup**

```rust
use dais_cloudflare::{CloudflareQueueProvider, D1Provider, WorkerHttpProvider};
use worker::*;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let db = D1Provider::new(env.d1("DB")?);
    let queue = CloudflareQueueProvider::new(env.queue("delivery")?);
    let http = WorkerHttpProvider::new();

    route_request(req, env, db, queue, http).await
}
```

### Business Logic in Core

All business logic lives in the core library:

```rust
// core/src/webfinger.rs
impl DaisCore {
    pub async fn webfinger(&self, resource: &str) -> CoreResult<WebFingerResponse> {
        // Extract username from resource
        let username = resource
            .strip_prefix("acct:")
            .and_then(|s| s.split('@').next())
            .ok_or(CoreError::InvalidResource)?;

        // Query database (platform-agnostic)
        let sql = "SELECT username, domain FROM users WHERE username = ?1";
        let rows = self.db.query(sql, &[Value::from(username)]).await?;

        if rows.is_empty() {
            return Err(CoreError::NotFound);
        }

        let row = &rows[0];
        let domain = row.get("domain")?;

        // Build WebFinger response
        Ok(WebFingerResponse {
            subject: format!("acct:{}@{}", username, domain),
            links: vec![
                Link {
                    rel: "self".to_string(),
                    type_: Some("application/activity+json".to_string()),
                    href: format!("https://{}/users/{}", domain, username),
                },
            ],
        })
    }
}
```

## Testing With Provider Traits

Provider traits let the core protocol code run against in-memory and mock
implementations in tests. Production bindings live under
`platforms/cloudflare/`.

## Migration System

### Creating Migrations

Migrations are SQL files in `cli/migrations/`:

```sql
-- cli/migrations/001_initial_schema.sql
-- Create users table
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,  -- SQLite syntax
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

-- Create posts table
CREATE TABLE IF NOT EXISTS posts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id)
);
```

### Applying Migrations

The migration system automatically converts syntax for the target database:

```rust
use dais_core::migrations::{Migration, MigrationRunner};

let migration = Migration {
    version: 1,
    name: "initial_schema".to_string(),
    up_sql: include_str!("../migrations/001_initial_schema.sql"),
    down_sql: None,
};

let runner = MigrationRunner::new(&db);
runner.apply(&migration).await?;
```

### Automatic Conversion

The migration system converts:
- **SQLite** → **PostgreSQL**: `INTEGER` → `SERIAL`, `TEXT` timestamps → `TIMESTAMP`
- **SQLite** → **MySQL**: `INTEGER PRIMARY KEY AUTOINCREMENT` → `INT AUTO_INCREMENT PRIMARY KEY`
- Parameter placeholders automatically converted

### Tracking Applied Migrations

Migrations are tracked in the `schema_migrations` table:

```sql
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

## Best Practices

### 1. Keep Business Logic in Core

✅ **Good**: Core library handles all business logic
```rust
// core/src/inbox.rs
impl DaisCore {
    pub async fn process_follow(&self, activity: Follow) -> CoreResult<()> {
        // All logic here
    }
}
```

❌ **Bad**: Business logic in worker shim
```rust
// platforms/cloudflare/workers/inbox/src/lib.rs
async fn main(req: Request, env: Env) -> Result<Response> {
    // DON'T put business logic here
    let activity = parse_activity(req)?;
    if activity.type_ == "Follow" {
        // Logic should be in core
    }
}
```

### 2. Use SQL Abstraction

✅ **Good**: Use QueryBuilder or convert placeholders
```rust
let query = QueryBuilder::new(dialect)
    .select(&["id", "username"])
    .from("users")
    .where_clause("id = ?1")
    .build();
```

❌ **Bad**: Hardcode PostgreSQL syntax
```rust
let query = "SELECT id, username FROM users WHERE id = $1";  // Won't work on SQLite/MySQL
```

### 3. Handle Platform Differences Gracefully

✅ **Good**: Check dialect when needed
```rust
let returning = match db.dialect() {
    DatabaseDialect::PostgreSQL => "RETURNING id",
    _ => "",  // SQLite and MySQL don't support RETURNING
};
```

### 4. Write Portable Migrations

✅ **Good**: Use SQLite syntax as base (most portable)
```sql
CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL
);
```

System will convert to:
- PostgreSQL: `id SERIAL PRIMARY KEY`
- MySQL: `id INT AUTO_INCREMENT PRIMARY KEY`

### 5. Test on Multiple Databases

Before releasing, test migrations on all supported databases:

```bash
# SQLite (Cloudflare D1)
wrangler d1 execute DB --file=migrations/001_initial_schema.sql

# PostgreSQL (local)
psql -U postgres -d dais -f migrations/001_initial_schema.sql

# MySQL (local)
mysql -u root -p dais < migrations/001_initial_schema.sql
```

### 6. Keep Workers Thin

Workers should be ~100-300 LOC:
- Parse platform-specific requests
- Create providers
- Call core library
- Return platform-specific responses

### 7. Use Trait Objects for Flexibility

```rust
pub struct DaisCore {
    db: Box<dyn DatabaseProvider>,  // Any database implementation
    storage: Box<dyn StorageProvider>,  // Any storage implementation
}
```

This allows:
- Easy testing with mock providers
- Platform switching without code changes
- Future platform additions

## Conclusion

The dais v1.1 architecture achieves **85-90% code reuse** across platforms through:

1. **Platform-agnostic core library** - All business logic
2. **Abstraction traits** - Database, Storage, Queue, HTTP
3. **SQL dialect support** - SQLite, PostgreSQL, MySQL
4. **Router-centered Cloudflare workers** - Platform-specific glue code
5. **Portable migrations** - Automatic syntax conversion

Adding a new platform requires:
- Implementing 4 traits (~500-1000 LOC)
- Creating platform worker entrypoints (~1000-2000 LOC)
- **Total**: ~2-3 weeks vs 6-8 weeks for full rewrite

**Next Steps**:
- Read `TESTING_v1.1.md` for testing procedures
- Read `MIGRATION_GUIDE_v1.0_to_v1.1.md` for upgrade instructions
- Check `../DEPLOYMENT.md` for platform-specific deployment guides
