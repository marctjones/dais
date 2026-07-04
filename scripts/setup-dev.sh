#!/bin/bash
set -e

echo "Setting up dais.social development environment..."
echo ""

# Check prerequisites
echo "Checking prerequisites..."

if ! command -v cargo &> /dev/null; then
    echo "Rust not found. Please install from https://rustup.rs/"
    exit 1
fi
echo "Rust found: $(cargo --version)"

if ! command -v wrangler &> /dev/null; then
    echo "wrangler not found."
    echo "Install the Cloudflare Wrangler CLI outside this repository and ensure it is on PATH:"
    echo "  npm install -g wrangler"
    echo "  https://developers.cloudflare.com/workers/wrangler/install-and-update/"
    exit 1
fi
echo "wrangler found: $(wrangler --version)"

echo ""
echo "Setting up Rust toolchain..."
rustup target add wasm32-unknown-unknown 2>/dev/null || true
echo "wasm32-unknown-unknown target ready"

if ! cargo install --list | grep -q worker-build; then
    echo "Installing worker-build..."
    cargo install worker-build
fi
echo "worker-build ready"

echo ""
echo "Development environment setup complete!"
echo ""
echo "Next steps:"
echo "  1. cargo run --manifest-path client/Cargo.toml -- --help"
echo "  2. ./scripts/seed-local-db.sh"
echo "  3. ./scripts/dev-start.sh"
echo "  4. ./scripts/deploy.sh build"
echo ""
echo "See CONTRIBUTING.md for development workflow."
