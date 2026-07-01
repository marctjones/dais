#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

cargo test --manifest-path core/Cargo.toml --features mls e2ee_mls:: -- --nocapture

echo "Local OpenMLS smoke passed"
