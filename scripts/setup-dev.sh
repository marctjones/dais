#!/bin/bash
set -e

echo "🚀 Setting up dais.social development environment..."
echo ""

# Check prerequisites
echo "Checking prerequisites..."

if ! command -v python3 &> /dev/null; then
    echo "❌ Python 3 not found. Please install Python 3.10+"
    exit 1
fi
echo "✅ Python 3 found: $(python3 --version)"

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
echo "Setting up Python virtual environment..."
python3 -m venv venv
source venv/bin/activate
pip install --upgrade pip -q
pip install -e "cli/[dev]" -q
echo "✅ Python CLI installed"

echo ""
echo "Setting up Rust toolchain..."
rustup target add wasm32-unknown-unknown 2>/dev/null || true
echo "✅ wasm32-unknown-unknown target ready"

if ! cargo install --list | grep -q worker-build; then
    echo "Installing worker-build..."
    cargo install worker-build
fi
echo "✅ worker-build ready"

echo ""
echo "✅ Development environment setup complete!"
echo ""
echo "Next steps:"
echo "  1. source venv/bin/activate"
echo "  2. dais setup init"
echo "  3. cd workers/webfinger && wrangler dev"
echo ""
echo "See CONTRIBUTING.md for development workflow."
