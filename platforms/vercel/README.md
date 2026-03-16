# dais for Vercel Edge Functions

This directory contains the Vercel Edge Functions implementation of dais, using the platform-agnostic core library.

## Structure

```
platforms/vercel/
├── bindings/              # Vercel-specific platform bindings
│   ├── src/
│   │   ├── lib.rs        # Main exports
│   │   ├── neon.rs       # Neon PostgreSQL provider
│   │   ├── blob.rs       # Vercel Blob storage provider
│   │   ├── http.rs       # HTTP client provider
│   │   └── queue.rs      # Queue provider (Upstash Redis)
│   └── Cargo.toml
│
├── functions/             # Vercel Edge Functions
│   └── webfinger/        # WebFinger function
│       ├── src/lib.rs
│       └── Cargo.toml
│
├── vercel.json           # Vercel configuration
├── DEPLOYMENT_VERCEL.md  # Deployment guide
└── README.md             # This file
```

## Quick Start

### Prerequisites

- Vercel account (https://vercel.com/signup)
- Neon PostgreSQL database (https://neon.tech)
- Upstash Redis (https://upstash.com)

### Deploy

```bash
# Clone repository
git clone https://github.com/daisocial/dais.git
cd dais
git checkout v1.2.0

# Configure environment variables
cd platforms/vercel
vercel env add DATABASE_URL production
vercel env add UPSTASH_REDIS_REST_URL production
vercel env add UPSTASH_REDIS_REST_TOKEN production

# Deploy
vercel --prod
```

See `DEPLOYMENT_VERCEL.md` for complete deployment instructions.

## Platform Bindings

### NeonProvider

Implements `DatabaseProvider` for Neon PostgreSQL (serverless PostgreSQL).

```rust
use dais_vercel::NeonProvider;

let connection_string = std::env::var("DATABASE_URL")?;
let db = NeonProvider::new(&connection_string).await?;
```

**Features**:
- Connection pooling
- Automatic parameter conversion (SQLite → PostgreSQL)
- Async/await support
- SSL/TLS connections

### VercelBlobProvider

Implements `StorageProvider` for Vercel Blob (S3-compatible object storage).

```rust
use dais_vercel::VercelBlobProvider;

let token = std::env::var("BLOB_READ_WRITE_TOKEN")?;
let storage = VercelBlobProvider::new(&token);
```

**Features**:
- Global CDN delivery
- Automatic content-type detection
- PUT/GET/DELETE operations
- Public and private blobs

### VercelHttpProvider

Implements `HttpProvider` for HTTP requests using reqwest.

```rust
use dais_vercel::VercelHttpProvider;

let http = VercelHttpProvider::new();
```

**Features**:
- Automatic retries
- Timeout handling
- Custom headers
- Streaming support

### VercelQueueProvider

Implements `QueueProvider` using Upstash Redis or HTTP webhooks.

```rust
use dais_vercel::{VercelQueueProvider, QueueStrategy};

// Option 1: Upstash Redis (recommended)
let provider = VercelQueueProvider::new(QueueStrategy::UpstashRedis {
    redis_url: std::env::var("UPSTASH_REDIS_REST_URL")?,
    redis_token: std::env::var("UPSTASH_REDIS_REST_TOKEN")?,
});

// Option 2: HTTP webhooks
let provider = VercelQueueProvider::new(QueueStrategy::HttpWebhook {
    webhook_url: "https://your-app.vercel.app/api/queue".to_string(),
});

// Option 3: Auto-detect from environment
let provider = VercelQueueProvider::from_env();
```

**Strategies**:
- **Upstash Redis**: Recommended for production (persistent, scalable)
- **HTTP Webhooks**: Call another Vercel function for processing
- **In-Memory**: Development/testing only (not persistent)

## Functions

### WebFinger Function

Handles `.well-known/webfinger` requests for ActivityPub discovery.

**Endpoint**: `/.well-known/webfinger?resource=acct:username@domain`

**Example**:
```bash
curl "https://social.example.com/.well-known/webfinger?resource=acct:alice@social.example.com"
```

**Implementation**: `functions/webfinger/src/lib.rs`

## Configuration

### vercel.json

```json
{
  "version": 2,
  "builds": [
    {
      "src": "functions/webfinger/Cargo.toml",
      "use": "@vercel/rust"
    }
  ],
  "routes": [
    {
      "src": "/.well-known/webfinger",
      "dest": "/functions/webfinger"
    }
  ],
  "env": {
    "DATABASE_URL": "@database-url",
    "BLOB_READ_WRITE_TOKEN": "@blob-token",
    "UPSTASH_REDIS_REST_URL": "@redis-url",
    "UPSTASH_REDIS_REST_TOKEN": "@redis-token"
  }
}
```

### Environment Variables

Required:
- `DATABASE_URL` - Neon PostgreSQL connection string
- `BLOB_READ_WRITE_TOKEN` - Vercel Blob token (auto-configured)

Optional:
- `UPSTASH_REDIS_REST_URL` - Upstash Redis URL
- `UPSTASH_REDIS_REST_TOKEN` - Upstash Redis token
- `QUEUE_WEBHOOK_URL` - Webhook URL for queue processing

## Development

### Local Testing

```bash
# Install Vercel CLI
npm install -g vercel

# Start local development server
vercel dev

# Test WebFinger locally
curl "http://localhost:3000/.well-known/webfinger?resource=acct:test@example.com"
```

### Adding New Functions

1. Create function directory:
```bash
mkdir -p functions/my-function/src
```

2. Create `Cargo.toml`:
```toml
[package]
name = "my-function-vercel"
version = "1.2.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
dais-core = { path = "../../../../core" }
dais-vercel = { path = "../../bindings" }
vercel_runtime = "1.1"
```

3. Create `src/lib.rs`:
```rust
use dais_core::DaisCore;
use dais_vercel::*;
use vercel_runtime::{run, Body, Error, Request, Response};

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(handler).await
}

async fn handler(req: Request) -> Result<Response<Body>, Error> {
    // Initialize providers
    let db = NeonProvider::new(&std::env::var("DATABASE_URL")?).await?;
    let storage = VercelBlobProvider::new(&std::env::var("BLOB_READ_WRITE_TOKEN")?);
    let queue = VercelQueueProvider::from_env();
    let http = VercelHttpProvider::new();

    // Create core
    let core = DaisCore::new(
        Box::new(db),
        Box::new(storage),
        Box::new(queue),
        Box::new(http),
    );

    // Process request using core library
    // ...

    Ok(Response::builder().body("OK".into())?)
}
```

4. Update `vercel.json`:
```json
{
  "builds": [
    {
      "src": "functions/my-function/Cargo.toml",
      "use": "@vercel/rust"
    }
  ],
  "routes": [
    {
      "src": "/my-endpoint",
      "dest": "/functions/my-function"
    }
  ]
}
```

## Testing

### Unit Tests

```bash
cd bindings
cargo test
```

### Integration Tests

```bash
# Set environment variables
export DATABASE_URL="postgresql://..."
export BLOB_READ_WRITE_TOKEN="..."
export UPSTASH_REDIS_REST_URL="..."
export UPSTASH_REDIS_REST_TOKEN="..."

# Run tests
cargo test -- --ignored
```

## Performance

### Metrics

- **Cold start**: ~200-300ms (first request)
- **Warm start**: ~50-100ms (subsequent requests)
- **Database query**: ~10-30ms (Neon)
- **Blob upload**: ~100-200ms (depending on size)
- **Queue operation**: ~5-10ms (Upstash Redis)

### Optimization Tips

1. **Reduce cold starts**:
   - Minimize dependencies
   - Use `opt-level = "z"` in Cargo.toml
   - Enable LTO: `lto = true`

2. **Database connections**:
   - Use connection pooling (PgBouncer)
   - Reuse connections when possible
   - Keep queries simple

3. **Blob storage**:
   - Compress images before upload
   - Use appropriate image formats (WebP)
   - Set proper cache headers

4. **Queue processing**:
   - Batch operations when possible
   - Use Redis pipelining
   - Handle failures gracefully

## Cost Estimation

### Free Tier Limits

- **Vercel Hobby**: 100 GB-hours/month, 100 GB bandwidth
- **Neon Free**: 3 GB storage, 191 hours compute/month
- **Upstash Free**: 10,000 commands/day, 256 MB storage

### Typical Usage (Single User)

- Function invocations: ~10,000/month
- Database queries: ~50,000/month
- Blob storage: ~500 MB
- Queue operations: ~5,000/month

**Estimated cost**: $0/month (within free tier)

### Scaling Costs

If exceeding free tier:
- Vercel Pro: $20/month
- Neon Scale: $19/month (3 GB+)
- Upstash: ~$10/month (100K commands/day)

**Estimated cost at scale**: ~$50/month

## Comparison with Cloudflare

| Feature | Vercel | Cloudflare |
|---------|--------|------------|
| Database | Neon (PostgreSQL) | D1 (SQLite) |
| Storage | Vercel Blob | R2 |
| Queue | Upstash Redis | Cloudflare Queues |
| Cold start | ~200ms | ~50ms |
| Free tier | 100 GB-hours | 100K requests/day |
| Global edge | Yes | Yes |
| Cost (free tier) | $0 | $0 |
| Cost (paid) | ~$50/month | ~$5/month |

**When to use Vercel**:
- ✅ Prefer PostgreSQL over SQLite
- ✅ Already using Vercel for frontend
- ✅ Need integrated analytics
- ✅ Want simpler deployment

**When to use Cloudflare**:
- ✅ Prefer SQLite simplicity
- ✅ Need lower cold start times
- ✅ Lower cost at scale
- ✅ Want more control

## Resources

- **Vercel Documentation**: https://vercel.com/docs
- **Neon Documentation**: https://neon.tech/docs
- **Upstash Documentation**: https://upstash.com/docs
- **dais Core Library**: `../../core/README.md`
- **Architecture Guide**: `../../ARCHITECTURE_v1.1.md`

## Support

- **GitHub Issues**: https://github.com/daisocial/dais/issues
- **Discussions**: https://github.com/daisocial/dais/discussions
- **Matrix**: #dais:matrix.org

## License

See LICENSE file in repository root.
