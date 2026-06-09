#!/bin/bash
set -e

echo "🚀 Setting up dais development environment..."
echo ""

# Check prerequisites
echo "Checking prerequisites..."

if ! command -v cargo &> /dev/null; then
    echo "❌ Rust not found. Please install from https://rustup.rs/"
    exit 1
fi
echo "✅ Rust found: $(cargo --version)"

if ! command -v wrangler &> /dev/null; then
    echo "❌ wrangler not found. Installing..."
    npm install -g wrangler
fi
echo "✅ wrangler found: $(wrangler --version)"

echo ""
echo "Setting up Rust toolchain..."
rustup target add wasm32-unknown-unknown 2>/dev/null || true
echo "✅ wasm32-unknown-unknown target ready (for the Workers)"

if ! cargo install --list | grep -q worker-build; then
    echo "Installing worker-build..."
    cargo install worker-build
fi
echo "✅ worker-build ready"

echo ""
echo "Building the native client..."
( cd client && cargo build )
echo "✅ client built"

echo ""
echo "✅ Development environment setup complete!"
echo ""
echo "Next steps:"
echo "  • Client:  cd client && cargo run -p dais -- tui   (or: dais --help)"
echo "  • Workers: cd platforms/cloudflare/workers/<name> && wrangler dev"
echo ""
echo "See CONTRIBUTING.md for the development workflow."
