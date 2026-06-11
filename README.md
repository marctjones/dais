# dais

dais is a single-user, private-by-default social server for Cloudflare. It
federates over ActivityPub, treats Bluesky / AT Protocol as the public broadcast
surface, and is moving toward end-to-end encrypted private messages over open
federation.

Live instance: `@social@dais.social`

## Current Shape

- Rust core: `core/`
- Cloudflare Workers: `platforms/cloudflare/workers/`
- Cloudflare bindings: `platforms/cloudflare/bindings/`
- Rust CLI/TUI client: `client/`
- D1 migrations: `cli/migrations/`

The old Python CLI and legacy `workers/` tree have been retired. Use the Rust
client and the core-based Cloudflare worker tree.

## Product Direction

The source of truth for the product is:

- [docs/POSITIONING.md](docs/POSITIONING.md)
- [docs/design/PRIVATE_MODE.md](docs/design/PRIVATE_MODE.md)
- [docs/design/PROTOCOL_ADAPTERS.md](docs/design/PROTOCOL_ADAPTERS.md)
- [docs/design/E2EE_WIRE_FORMAT.md](docs/design/E2EE_WIRE_FORMAT.md)

GitHub issues under epic #70 track roadmap, decisions, and active work.

## Rust Client

Run the client locally:

```bash
cargo run --manifest-path client/Cargo.toml -- --help
```

Common commands:

```bash
cargo run --manifest-path client/Cargo.toml -- post create "private by default"
cargo run --manifest-path client/Cargo.toml -- post create "public broadcast" --visibility public --protocol both
cargo run --manifest-path client/Cargo.toml -- timeline home --env production
cargo run --manifest-path client/Cargo.toml -- friends list --env production
cargo run --manifest-path client/Cargo.toml -- tui --env production
```

Private/followers visibility is the default. Public posting is explicit.

## Development

Install local prerequisites:

```bash
./scripts/setup-dev.sh
```

Seed local D1 state:

```bash
./scripts/seed-local-db.sh
```

Run checks:

```bash
cargo test --manifest-path core/Cargo.toml
cargo test --manifest-path client/Cargo.toml
cargo check --manifest-path platforms/cloudflare/workers/actor/Cargo.toml
```

Worker builds use current `worker-build` with the rustup toolchain path set in
each worker `wrangler.toml`.

## Deploy

Use the repository deploy script:

```bash
scripts/deploy.sh build
scripts/deploy.sh deploy --env production --yes
```

Deploy individual workers when needed:

```bash
scripts/deploy.sh deploy --env production --only actor --yes
scripts/deploy.sh deploy --env production --only inbox --yes
scripts/deploy.sh deploy --env production --only outbox --yes
scripts/deploy.sh deploy --env production --only router --yes
```

See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for the longer operational guide.

## Documentation

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/ROADMAP.md](docs/ROADMAP.md)
- [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)
- [docs/guides/DEVELOPMENT.md](docs/guides/DEVELOPMENT.md)
- [docs/guides/OPERATIONAL_RUNBOOK.md](docs/guides/OPERATIONAL_RUNBOOK.md)
- [docs/guides/PRIVACY_GUIDE.md](docs/guides/PRIVACY_GUIDE.md)
- [docs/guides/FEDERATION_GUIDE.md](docs/guides/FEDERATION_GUIDE.md)
- [docs/reference/API_DOCUMENTATION.md](docs/reference/API_DOCUMENTATION.md)

Historical v1.0/v1.1 snapshots live in [docs/archive](docs/archive/).
