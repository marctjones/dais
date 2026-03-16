# dais v1.1 Architecture Guide

**Date**: March 15, 2026
**Version**: v1.1.0

## Table of Contents

1. [Overview](#overview)
2. [Multi-Platform Architecture](#multi-platform-architecture)
3. [Core Abstraction Layer](#core-abstraction-layer)
4. [Platform Bindings](#platform-bindings)
5. [Database Abstraction](#database-abstraction)
6. [Worker Pattern](#worker-pattern)
7. [Adding a New Platform](#adding-a-new-platform)
8. [Migration System](#migration-system)
9. [Best Practices](#best-practices)

## Overview

dais v1.1 introduces a **multi-platform architecture** that enables running the same ActivityPub server logic on different cloud platforms with minimal code duplication.

**Key Benefits**:
- 85-90% code reuse across platforms
- Support for multiple databases (SQLite, PostgreSQL, MySQL)
- Platform-specific optimizations possible
- Easy to add new platforms

**Supported Platforms**:
- ✅ Cloudflare Workers (D1 SQLite)
- 🔜 Vercel Edge Functions (Neon PostgreSQL) - Planned v1.2
- 🔜 Netlify Edge Functions (Turso SQLite / Neon PostgreSQL) - Planned v1.3

## Multi-Platform Architecture

### Three-Layer Design

```
┌─────────────────────────────────────────────┐
│         Workers (Platform Shims)            │
│  webfinger, actor, inbox, outbox, etc.      │
│         (10-15% of code)                    │
└─────────────────────────────────────────────┘
                    ▼
┌─────────────────────────────────────────────┐
│      Platform Bindings (Per-Platform)       │
│   D1Provider, CloudflareQueueProvider       │
│         (5-10% of code)                     │
└─────────────────────────────────────────────┘
                    ▼
┌─────────────────────────────────────────────┐
│     Platform-Agnostic Core (dais-core)      │
│  Business Logic, ActivityPub Protocol       │
│         (85-90% of code)                    │
└─────────────────────────────────────────────┘
```

### Directory Structure

```
dais/
├── core/                       # Platform-agnostic library
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
│   │   │   │   ├── queue.rs   # CloudflareQueueProvider
│   │   │   │   ├── http.rs    # WorkerHttpProvider
│   │   │   │   └── r2.rs      # R2Provider (storage)
│   │   │   └── Cargo.toml
│   │   └── workers/           # Thin worker shims
│   │       ├── webfinger/
│   │       ├── actor/
│   │       ├── inbox/
│   │       └── ...
│   └── vercel/                # Future platform
│       ├── bindings/
│       └── functions/
│
└── cli/                       # dais CLI tool
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

## Platform Bindings

### Cloudflare Example

Each platform implements the core traits using platform-specific APIs.

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

### Vercel Example (Future)

**NeonProvider** (PostgreSQL for Vercel):

```rust
use dais_core::traits::{DatabaseProvider, DatabaseDialect};
use vercel_postgres::Client;

pub struct NeonProvider {
    client: Client,
}

#[async_trait]
impl DatabaseProvider for NeonProvider {
    async fn query(&self, sql: &str, params: &[Value]) -> CoreResult<Vec<Row>> {
        let rows = self.client.query(sql, params).await?;
        Ok(convert_rows(rows))
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> CoreResult<u64> {
        let result = self.client.execute(sql, params).await?;
        Ok(result)
    }

    fn dialect(&self) -> DatabaseDialect {
        DatabaseDialect::PostgreSQL
    }
}
```

## Database Abstraction

### SQL Dialect Support

The core library supports three database dialects:

```rust
pub enum DatabaseDialect {
    SQLite,     // Cloudflare D1, Turso
    PostgreSQL, // Neon, Railway, Supabase
    MySQL,      // PlanetScale
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

## Worker Pattern

### Thin Worker Shims

Each worker is a thin shim (~100-300 LOC) that:
1. Receives platform-specific requests
2. Creates platform providers
3. Calls core library functions
4. Returns platform-specific responses

**Example: WebFinger Worker**

```rust
use dais_core::DaisCore;
use dais_cloudflare::{D1Provider, CloudflareQueueProvider, WorkerHttpProvider, R2Provider};
use worker::*;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // 1. Parse request
    let url = req.url()?;
    let resource = url.query_pairs()
        .find(|(key, _)| key == "resource")
        .map(|(_, value)| value.to_string())
        .ok_or_else(|| "Missing resource parameter")?;

    // 2. Create platform providers
    let db = D1Provider::new(env.d1("DB")?);
    let storage = R2Provider::new(env.bucket("MEDIA")?);
    let queue = CloudflareQueueProvider::new(env.queue("delivery")?);
    let http = WorkerHttpProvider::new();

    // 3. Call core library
    let core = DaisCore::new(
        Box::new(db),
        Box::new(storage),
        Box::new(queue),
        Box::new(http),
    );

    let response = core.webfinger(&resource).await
        .map_err(|e| format!("WebFinger error: {}", e))?;

    // 4. Return platform-specific response
    Response::from_json(&response)
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

## Adding a New Platform

### Step-by-Step Guide

**1. Create Platform Bindings Directory**

```bash
mkdir -p platforms/vercel/bindings/src
cd platforms/vercel/bindings
```

**2. Create Cargo.toml**

```toml
[package]
name = "dais-vercel"
version = "1.1.0"
edition = "2021"

[dependencies]
dais-core = { path = "../../../core" }
async-trait = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
vercel-postgres = "0.5"  # Example dependency
```

**3. Implement DatabaseProvider**

```rust
// platforms/vercel/bindings/src/neon.rs
use dais_core::traits::{DatabaseProvider, DatabaseDialect};
use async_trait::async_trait;

pub struct NeonProvider {
    connection_string: String,
}

impl NeonProvider {
    pub fn new(connection_string: String) -> Self {
        Self { connection_string }
    }
}

#[async_trait]
impl DatabaseProvider for NeonProvider {
    async fn query(&self, sql: &str, params: &[Value]) -> CoreResult<Vec<Row>> {
        // Use vercel-postgres or tokio-postgres
        // Convert placeholders: SQLite ?1 -> PostgreSQL $1
        let postgres_sql = convert_placeholders(sql, DatabaseDialect::PostgreSQL);

        // Execute query using Vercel's Postgres API
        // ...
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> CoreResult<u64> {
        // Execute statement
        // ...
    }

    fn dialect(&self) -> DatabaseDialect {
        DatabaseDialect::PostgreSQL
    }
}
```

**4. Implement Other Providers**

```rust
// platforms/vercel/bindings/src/blob.rs - Vercel Blob Storage
pub struct VercelBlobProvider { /* ... */ }

// platforms/vercel/bindings/src/http.rs - Vercel Edge Runtime
pub struct VercelHttpProvider { /* ... */ }

// platforms/vercel/bindings/src/queue.rs - Vercel Queue (or alternative)
pub struct VercelQueueProvider { /* ... */ }
```

**5. Create Worker Functions**

```typescript
// platforms/vercel/functions/webfinger.ts
import { createWasmInstance } from './wasm-loader';

export default async function handler(req: Request) {
  const url = new URL(req.url);
  const resource = url.searchParams.get('resource');

  if (!resource) {
    return new Response('Missing resource parameter', { status: 400 });
  }

  // Initialize WASM module with Vercel providers
  const instance = await createWasmInstance({
    database: process.env.POSTGRES_URL,
    storage: process.env.BLOB_READ_WRITE_TOKEN,
  });

  const response = await instance.webfinger(resource);

  return new Response(JSON.stringify(response), {
    headers: { 'Content-Type': 'application/jrd+json' },
  });
}
```

**6. Test Compilation**

```bash
cd platforms/vercel/bindings
cargo check
```

**7. Deploy**

```bash
vercel deploy
```

### Platform Requirements

For a new platform to work with dais, it needs:

1. **Database Support**:
   - SQLite, PostgreSQL, or MySQL
   - Async query interface
   - Transaction support (recommended)

2. **HTTP Client**:
   - Async HTTP requests
   - Support for custom headers (for ActivityPub)
   - TLS/HTTPS support

3. **Object Storage** (optional):
   - File upload/download
   - Public URLs for media

4. **Background Jobs** (optional):
   - Queue for delivery retries
   - Can be replaced with alternative mechanism

5. **WASM Support**:
   - Platform must support WebAssembly
   - Rust compilation to wasm32-unknown-unknown

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
// workers/inbox/src/lib.rs
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
4. **Thin worker shims** - Platform-specific glue code
5. **Portable migrations** - Automatic syntax conversion

Adding a new platform requires:
- Implementing 4 traits (~500-1000 LOC)
- Creating worker shims (~1000-2000 LOC)
- **Total**: ~2-3 weeks vs 6-8 weeks for full rewrite

**Next Steps**:
- Read `TESTING_v1.1.md` for testing procedures
- Read `MIGRATION_GUIDE_v1.0_to_v1.1.md` for upgrade instructions
- Check `DEPLOYMENT.md` for platform-specific deployment guides
