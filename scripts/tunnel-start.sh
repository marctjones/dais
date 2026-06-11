#!/usr/bin/env bash
set -euo pipefail

if ! command -v cloudflared >/dev/null 2>&1; then
  echo "cloudflared is not installed. Install with: brew install cloudflared" >&2
  exit 1
fi

PORT="${DAIS_TUNNEL_PORT:-8787}"
HOST="${DAIS_TUNNEL_HOST:-127.0.0.1}"

cat >&2 <<EOF
Starting a temporary Cloudflare tunnel for local federation testing.

Local target: http://${HOST}:${PORT}

Security notes:
- This exposes the selected local worker/router to the public internet.
- Use only for federation testing.
- Do not commit temporary trycloudflare URLs.
- Stop this process when the test run is finished.
EOF

exec cloudflared tunnel --url "http://${HOST}:${PORT}"
