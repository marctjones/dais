#!/usr/bin/env bash
#
# Create a portable dais server backup archive.
#
# The archive contains a manifest, D1 SQL export, optional Cloudflare backup
# metadata, optional R2 object inventory, and local owner/key material when it is
# present on the machine running the backup. It verifies the archive before
# reporting success.
#
# Usage:
#   scripts/backup.sh [OPTIONS]
#
# Options:
#   --env ENV          Cloudflare environment: production (default) | skpt
#   --output-dir DIR   Backup output directory (default: ~/.dais/backups)
#   --no-encrypt       Write a plain .tar.gz archive instead of .tar.gz.gpg
#   --skip-cloud       Skip Wrangler D1/R2 export; useful for local archive tests
#   --keep-temp        Keep the staging directory for inspection
#   -h, --help         Show this help
#
# Environment:
#   DAIS_BACKUP_PASSPHRASE_FILE  File containing the encryption passphrase
#   DAIS_BACKUP_PASSPHRASE       Encryption passphrase
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROUTER_DIR="$ROOT/platforms/cloudflare/workers/router"
VERIFY_SCRIPT="$ROOT/scripts/verify-backup-archive.sh"
ENVIRONMENT="production"
BACKUP_DIR="${HOME}/.dais/backups"
ENCRYPT="true"
SKIP_CLOUD="false"
KEEP_TEMP="false"
DATE="$(date -u +%Y%m%dT%H%M%SZ)"
WRANGLER="${WRANGLER:-wrangler}"

usage() { sed -n '2,28p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
  case "$1" in
    --env) ENVIRONMENT="${2:-}"; shift 2 ;;
    --output-dir) BACKUP_DIR="${2:-}"; shift 2 ;;
    --no-encrypt) ENCRYPT="false"; shift ;;
    --skip-cloud) SKIP_CLOUD="true"; shift ;;
    --keep-temp) KEEP_TEMP="true"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; echo >&2; usage >&2; exit 2 ;;
  esac
done

case "$ENVIRONMENT" in
  production|skpt) ;;
  *) echo "Invalid --env '$ENVIRONMENT' (expected production | skpt)" >&2; exit 2 ;;
esac

mkdir -p "$BACKUP_DIR"
TEMP_DIR="$(mktemp -d)"
if [ "$KEEP_TEMP" = "true" ]; then
  trap 'echo "staging_dir='"$TEMP_DIR"'"' EXIT
else
  trap 'rm -rf "$TEMP_DIR"' EXIT
fi

ARCHIVE_BASENAME="dais_${ENVIRONMENT}_backup_${DATE}.tar.gz"
PLAIN_ARCHIVE="$BACKUP_DIR/$ARCHIVE_BASENAME"
FINAL_ARCHIVE="$PLAIN_ARCHIVE"

mkdir -p "$TEMP_DIR/cloudflare" "$TEMP_DIR/local/dais" "$TEMP_DIR/local/dais-desk" "$TEMP_DIR/local/dais-owner"
INCLUDED=()
WARNINGS=()

include_file() {
  local source="$1" destination="$2" label="$3"
  if [ -f "$source" ]; then
    mkdir -p "$(dirname "$TEMP_DIR/$destination")"
    cp "$source" "$TEMP_DIR/$destination"
    INCLUDED+=("$label:$destination")
  else
    WARNINGS+=("missing:$label:$source")
  fi
}

json_array() {
  local first="true"
  printf '['
  for item in "$@"; do
    [ "$first" = "true" ] || printf ','
    first="false"
    printf '"%s"' "$(printf "%s" "$item" | sed 's/\\/\\\\/g; s/"/\\"/g')"
  done
  printf ']'
}

write_manifest() {
  {
    printf '{\n'
    printf '  "format": "dais-backup-v1",\n'
    printf '  "created_at_utc": "%s",\n' "$DATE"
    printf '  "environment": "%s",\n' "$ENVIRONMENT"
    printf '  "cloud_export_skipped": %s,\n' "$([ "$SKIP_CLOUD" = "true" ] && printf true || printf false)"
    printf '  "included": '
    json_array "${INCLUDED[@]}"
    printf ',\n'
    printf '  "warnings": '
    json_array "${WARNINGS[@]}"
    printf '\n}\n'
  } > "$TEMP_DIR/manifest.json"
}

echo "=== dais backup ($ENVIRONMENT) $DATE ==="

if [ "$SKIP_CLOUD" = "true" ]; then
  WARNINGS+=("cloud:skipped-by-operator")
  printf '%s\n' "--skip-cloud used; no D1 export captured" > "$TEMP_DIR/database.sql"
  INCLUDED+=("d1-placeholder:database.sql")
else
  if ! command -v "$WRANGLER" >/dev/null 2>&1 && [ ! -x "$WRANGLER" ]; then
    echo "wrangler not found. Set WRANGLER=/path/to/wrangler or use --skip-cloud for local archive tests." >&2
    exit 1
  fi

  echo "[1/5] Exporting D1 database"
  (
    cd "$ROUTER_DIR"
    "$WRANGLER" d1 export DB --remote --env "$ENVIRONMENT" --output "$TEMP_DIR/database.sql"
  )
  INCLUDED+=("d1-sql:database.sql")

  echo "[2/5] Recording Cloudflare backup metadata"
  if (
    cd "$ROUTER_DIR"
    "$WRANGLER" d1 backup create DB --remote --env "$ENVIRONMENT"
  ) > "$TEMP_DIR/cloudflare/d1-backup-create.txt" 2>&1; then
    INCLUDED+=("d1-backup-metadata:cloudflare/d1-backup-create.txt")
  else
    WARNINGS+=("d1-backup-create-failed:cloudflare/d1-backup-create.txt")
  fi

  echo "[3/5] Recording R2 media inventory"
  bucket="dais-media"
  [ "$ENVIRONMENT" = "skpt" ] && bucket="dais-media-skpt"
  if "$WRANGLER" r2 object list "$bucket" --json > "$TEMP_DIR/cloudflare/media-objects.json" 2>"$TEMP_DIR/cloudflare/media-objects.err"; then
    INCLUDED+=("r2-media-inventory:cloudflare/media-objects.json")
  else
    WARNINGS+=("r2-media-inventory-failed:cloudflare/media-objects.err")
  fi
fi

echo "[4/5] Collecting local owner and key material"
include_file "$HOME/.dais/config.toml" "local/dais/config.toml" "server-config"
include_file "$HOME/.dais/keys/private.pem" "local/dais/keys/private.pem" "activitypub-private-key"
include_file "$HOME/.dais/keys/public.pem" "local/dais/keys/public.pem" "activitypub-public-key"
include_file "$HOME/.dais/pds-password.txt" "local/dais/pds-password.txt" "pds-password"
include_file "$HOME/Library/Application Support/social.dais.desk/owner-settings.json" "local/dais-desk/owner-settings.json" "desk-owner-settings"
include_file "$HOME/Library/Application Support/social.dais.owner/owner-settings.json" "local/dais-owner/owner-settings.json" "legacy-owner-settings"

write_manifest

echo "[5/5] Creating archive"
tar -czf "$PLAIN_ARCHIVE" -C "$TEMP_DIR" .

if [ "$ENCRYPT" = "true" ]; then
  if command -v gpg >/dev/null 2>&1; then
    passphrase="${DAIS_BACKUP_PASSPHRASE:-}"
    if [ -n "${DAIS_BACKUP_PASSPHRASE_FILE:-}" ]; then
      if [ ! -f "$DAIS_BACKUP_PASSPHRASE_FILE" ]; then
        echo "DAIS_BACKUP_PASSPHRASE_FILE does not exist: $DAIS_BACKUP_PASSPHRASE_FILE" >&2
        exit 1
      fi
      passphrase="$(cat "$DAIS_BACKUP_PASSPHRASE_FILE")"
    fi
    if [ -z "$passphrase" ]; then
      echo "DAIS_BACKUP_PASSPHRASE or DAIS_BACKUP_PASSPHRASE_FILE is required for encrypted backups. Use --no-encrypt for local tests." >&2
      exit 1
    fi
    gpg --symmetric --cipher-algo AES256 --batch --yes \
      --passphrase "$passphrase" \
      --output "$PLAIN_ARCHIVE.gpg" \
      "$PLAIN_ARCHIVE"
    rm -f "$PLAIN_ARCHIVE"
    FINAL_ARCHIVE="$PLAIN_ARCHIVE.gpg"
  else
    echo "gpg not found; refusing to write unencrypted secret-bearing backup without --no-encrypt" >&2
    exit 1
  fi
fi

"$VERIFY_SCRIPT" "$FINAL_ARCHIVE"

echo
echo "Backup complete: $FINAL_ARCHIVE"
du -h "$FINAL_ARCHIVE" | awk '{print "Size: " $1}'

find "$BACKUP_DIR" -name "dais_*_backup_*.tar.gz*" -mtime +30 -delete 2>/dev/null || true
