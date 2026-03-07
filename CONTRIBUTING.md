# Development Guide

## Development Environment Setup

### Prerequisites

- **Rust** 1.70+ with `wasm32-unknown-unknown` target
- **Python** 3.10+
- **Node.js** 20+ (for wrangler)
- **wrangler** 4.0+ (Cloudflare CLI)

### Initial Setup

```bash
# Clone the repository
git clone <repo-url>
cd dais

# Set up Python development environment
python3 -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate
pip install -e "cli/[dev]"

# Verify Rust toolchain
rustup target add wasm32-unknown-unknown
cargo install worker-build

# Verify installations
dais --version
wrangler --version
cargo --version
```

## Project Structure

```
dais/
├── venv/                  # Python virtual environment (gitignored)
├── workers/               # Rust Workers → WASM
│   ├── webfinger/        # WebFinger endpoint
│   ├── actor/            # ActivityPub Actor
│   ├── inbox/            # Receive activities
│   ├── outbox/           # Publish activities
│   └── shared/           # Shared Rust code
│       ├── activitypub/  # ActivityPub types
│       ├── crypto/       # HTTP signatures
│       └── db/           # D1 queries
├── cli/                  # Python CLI
│   └── dais_cli/         # CLI source
└── web/                  # Static landing page
```

## Development Workflow

### Python CLI Development

```bash
# Activate virtual environment
source venv/bin/activate

# Install in development mode
pip install -e "cli/[dev]"

# Make changes to cli/dais_cli/

# Test changes immediately
dais --help
dais setup init

# Run tests
cd cli
pytest

# Format code
black .
ruff check .
```

### Rust Worker Development

```bash
# Navigate to worker
cd workers/webfinger

# Local development with hot reload
wrangler dev

# Test locally
curl "http://localhost:8787/.well-known/webfinger?resource=acct:marc@dais.social"

# Build for production
worker-build --release

# Deploy to Cloudflare
wrangler deploy
```

### Testing WebFinger Worker Locally

```bash
# Terminal 1: Start worker
cd workers/webfinger
wrangler dev

# Terminal 2: Test endpoint
curl "http://localhost:8787/.well-known/webfinger?resource=acct:marc@dais.social" | jq

# Or use the CLI
source venv/bin/activate
dais test webfinger
```

## Making Changes

### Adding a New CLI Command

1. Create new command file in `cli/dais_cli/commands/`
2. Register command in `cli/dais_cli/cli.py`
3. Test with `dais <command> --help`

Example:
```python
# cli/dais_cli/commands/mycommand.py
import click

@click.command()
def mycommand():
    """My new command."""
    click.echo("Hello!")
```

```python
# cli/dais_cli/cli.py
from dais_cli.commands import mycommand

main.add_command(mycommand.mycommand)
```

### Adding a New Worker

1. Create directory: `workers/newworker/`
2. Add `Cargo.toml`, `wrangler.toml`, `src/lib.rs`
3. Follow pattern from `workers/webfinger/`
4. Test locally with `wrangler dev`
5. Deploy with `wrangler deploy`

### Adding Shared Rust Code

```bash
# Add types to shared library
cd workers/shared/src/activitypub/

# Use in worker Cargo.toml
[dependencies.shared]
path = "../shared"
```

## Testing

### Python Tests

```bash
cd cli
pytest                    # Run all tests
pytest -v                 # Verbose output
pytest tests/test_config.py  # Specific test file
```

### Rust Tests

```bash
cd workers/webfinger
cargo test
```

### Integration Tests

```bash
# Start worker locally
cd workers/webfinger
wrangler dev &

# Run integration tests
source venv/bin/activate
dais test webfinger
dais test actor
```

## Code Quality

### Python

```bash
cd cli

# Format code
black .

# Lint
ruff check .

# Type checking (if using mypy)
mypy dais_cli/
```

### Rust

```bash
cd workers/webfinger

# Format
cargo fmt

# Lint
cargo clippy

# Check compilation
cargo check
```

## Deployment

### Deploy All Workers

```bash
# WebFinger
cd workers/webfinger && wrangler deploy

# Actor (when ready)
cd workers/actor && wrangler deploy

# Inbox (when ready)
cd workers/inbox && wrangler deploy

# Outbox (when ready)
cd workers/outbox && wrangler deploy
```

### Deploy to Production

```bash
# Use production environment
wrangler deploy --env production
```

## Troubleshooting

### "worker-build not found"

```bash
cargo install worker-build
```

### "wasm32-unknown-unknown target not found"

```bash
rustup target add wasm32-unknown-unknown
```

### Python dependencies not found

```bash
source venv/bin/activate
pip install -e "cli/[dev]"
```

### wrangler authentication

```bash
wrangler login
```

## Environment Variables

Create `.env` file in project root (gitignored):

```bash
# Cloudflare
CLOUDFLARE_ACCOUNT_ID=your-account-id
CLOUDFLARE_API_TOKEN=your-api-token

# D1 Database
D1_DATABASE_ID=your-database-id

# R2 Bucket
R2_BUCKET_NAME=dais-media
```

## Git Workflow

```bash
# Feature branch
git checkout -b feature/my-feature

# Make changes, commit
git add .
git commit -m "feat: add new feature"

# Push
git push origin feature/my-feature
```

## Need Help?

- Check the [README.md](README.md) for project overview
- Review [ActivityPub spec](https://www.w3.org/TR/activitypub/)
- Check [Cloudflare Workers docs](https://developers.cloudflare.com/workers/)
