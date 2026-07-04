#!/usr/bin/env bash
#
# Verify a dais backup archive contains the minimum portable restore payload.
#
# Usage:
#   scripts/verify-backup-archive.sh ARCHIVE.tar.gz[.gpg]
#
set -euo pipefail

ARCHIVE="${1:-}"
if [ -z "$ARCHIVE" ]; then
  echo "Usage: scripts/verify-backup-archive.sh ARCHIVE.tar.gz[.gpg]" >&2
  exit 2
fi
if [ ! -f "$ARCHIVE" ]; then
  echo "Backup archive not found: $ARCHIVE" >&2
  exit 2
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

case "$ARCHIVE" in
  *.gpg)
    if ! command -v gpg >/dev/null 2>&1; then
      echo "gpg is required to verify encrypted backup archives" >&2
      exit 2
    fi
    passphrase="${DAIS_BACKUP_PASSPHRASE:-}"
    if [ -n "${DAIS_BACKUP_PASSPHRASE_FILE:-}" ]; then
      if [ ! -f "$DAIS_BACKUP_PASSPHRASE_FILE" ]; then
        echo "DAIS_BACKUP_PASSPHRASE_FILE does not exist: $DAIS_BACKUP_PASSPHRASE_FILE" >&2
        exit 2
      fi
      passphrase="$(cat "$DAIS_BACKUP_PASSPHRASE_FILE")"
    fi
    if [ -z "$passphrase" ]; then
      echo "DAIS_BACKUP_PASSPHRASE or DAIS_BACKUP_PASSPHRASE_FILE is required to verify encrypted backup archives" >&2
      exit 2
    fi
    gpg --decrypt --batch --yes --passphrase "$passphrase" \
      --output "$TMP_DIR/archive.tar.gz" "$ARCHIVE"
    TAR="$TMP_DIR/archive.tar.gz"
    ;;
  *)
    TAR="$ARCHIVE"
    ;;
esac

tar -xzf "$TAR" -C "$TMP_DIR"

missing=0
for required in manifest.json database.sql; do
  if [ ! -f "$TMP_DIR/$required" ]; then
    echo "Missing required backup member: $required" >&2
    missing=1
  fi
done

if [ "$missing" -ne 0 ]; then
  exit 1
fi

if ! grep -q '"format": "dais-backup-v1"' "$TMP_DIR/manifest.json"; then
  echo "Backup manifest has an unknown format" >&2
  exit 1
fi

if ! grep -q '"included":' "$TMP_DIR/manifest.json"; then
  echo "Backup manifest does not list included payloads" >&2
  exit 1
fi

echo "Verified dais backup archive: $ARCHIVE"
