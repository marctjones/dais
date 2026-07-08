#!/usr/bin/env bash
#
# Verify that a dais backup archive can be restored into a fresh local SQLite
# database and still contains the operational data families required for owner
# portability.
#
set -euo pipefail

ARCHIVE=""
ALLOW_PLACEHOLDER_SQL="false"
KEEP_TEMP="false"
SELF_TEST="false"

usage() {
  cat <<'USAGE'
Usage:
  scripts/verify-backup-restore.sh ARCHIVE.tar.gz[.gpg]
  scripts/verify-backup-restore.sh --self-test

Options:
  --allow-placeholder-sql   Permit --skip-cloud backup placeholders; skips SQL restore
  --keep-temp               Print and retain the restore staging directory
  --self-test               Build and verify a minimal fixture archive
  -h, --help                Show this help

Encrypted archives use DAIS_BACKUP_PASSPHRASE or DAIS_BACKUP_PASSPHRASE_FILE.
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --allow-placeholder-sql) ALLOW_PLACEHOLDER_SQL="true"; shift ;;
    --keep-temp) KEEP_TEMP="true"; shift ;;
    --self-test) SELF_TEST="true"; shift ;;
    -h|--help) usage; exit 0 ;;
    -*)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      if [ -n "$ARCHIVE" ]; then
        echo "Only one archive may be supplied" >&2
        exit 2
      fi
      ARCHIVE="$1"
      shift
      ;;
  esac
done

TMP_DIR="$(mktemp -d)"
if [ "$KEEP_TEMP" = "true" ]; then
  trap 'echo "restore_staging_dir='"$TMP_DIR"'"' EXIT
else
  trap 'rm -rf "$TMP_DIR"' EXIT
fi

json_manifest() {
  cat > "$TMP_DIR/fixture/manifest.json" <<'JSON'
{
  "format": "dais-backup-v1",
  "created_at_utc": "self-test",
  "environment": "self-test",
  "cloud_export_skipped": false,
  "included": ["d1-sql:database.sql"],
  "warnings": []
}
JSON
}

self_test_archive() {
  mkdir -p "$TMP_DIR/fixture"
  json_manifest
  cat > "$TMP_DIR/fixture/database.sql" <<'SQL'
CREATE TABLE actors (id TEXT PRIMARY KEY, username TEXT);
CREATE TABLE followers (id TEXT PRIMARY KEY, actor_id TEXT, follower_actor_id TEXT);
CREATE TABLE following (id TEXT PRIMARY KEY, actor_id TEXT, target_actor_id TEXT);
CREATE TABLE posts (id TEXT PRIMARY KEY, content TEXT, media_attachments TEXT);
CREATE TABLE instance_settings (id INTEGER PRIMARY KEY, default_visibility TEXT);
CREATE TABLE moderation_settings (id INTEGER PRIMARY KEY, reply_policy TEXT);
CREATE TABLE blocks (id TEXT PRIMARY KEY, actor_id TEXT, blocked_domain TEXT);
CREATE TABLE audience_lists (id TEXT PRIMARY KEY, name TEXT);
CREATE TABLE audience_list_members (list_id TEXT, actor_id TEXT);
CREATE TABLE source_subscriptions (id TEXT PRIMARY KEY, source_type TEXT, url TEXT);
CREATE TABLE source_items (id TEXT PRIMARY KEY, source_id TEXT, title TEXT);
CREATE TABLE e2ee_devices (id TEXT PRIMARY KEY, actor_id TEXT, device_id TEXT);
CREATE TABLE e2ee_peer_devices (id TEXT PRIMARY KEY, actor_id TEXT, device_id TEXT);
CREATE TABLE e2ee_messages (id TEXT PRIMARY KEY, conversation_id TEXT, ciphertext TEXT);
SQL
  ARCHIVE="$TMP_DIR/dais_restore_self_test.tar.gz"
  tar -czf "$ARCHIVE" -C "$TMP_DIR/fixture" .
}

extract_archive() {
  local archive="$1"
  local tar_file="$archive"
  case "$archive" in
    *.gpg)
      if ! command -v gpg >/dev/null 2>&1; then
        echo "gpg is required to verify encrypted backup archives" >&2
        exit 2
      fi
      local passphrase="${DAIS_BACKUP_PASSPHRASE:-}"
      if [ -n "${DAIS_BACKUP_PASSPHRASE_FILE:-}" ]; then
        if [ ! -f "$DAIS_BACKUP_PASSPHRASE_FILE" ]; then
          echo "DAIS_BACKUP_PASSPHRASE_FILE does not exist: $DAIS_BACKUP_PASSPHRASE_FILE" >&2
          exit 2
        fi
        passphrase="$(cat "$DAIS_BACKUP_PASSPHRASE_FILE")"
      fi
      if [ -z "$passphrase" ]; then
        echo "DAIS_BACKUP_PASSPHRASE or DAIS_BACKUP_PASSPHRASE_FILE is required for encrypted archives" >&2
        exit 2
      fi
      gpg --decrypt --batch --yes --passphrase "$passphrase" \
        --output "$TMP_DIR/archive.tar.gz" "$archive"
      tar_file="$TMP_DIR/archive.tar.gz"
      ;;
  esac
  mkdir -p "$TMP_DIR/extract"
  tar -xzf "$tar_file" -C "$TMP_DIR/extract"
}

require_file() {
  local path="$1"
  if [ ! -f "$TMP_DIR/extract/$path" ]; then
    echo "Missing required backup member: $path" >&2
    exit 1
  fi
}

sqlite_scalar() {
  sqlite3 "$TMP_DIR/restore.sqlite" "$1"
}

require_table() {
  local table="$1"
  local label="$2"
  local count
  count="$(sqlite_scalar "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name='$table';")"
  if [ "$count" != "1" ]; then
    echo "Restore missing $label table: $table" >&2
    exit 1
  fi
  echo "OK   $label table: $table"
}

require_column() {
  local table="$1"
  local column="$2"
  local label="$3"
  local count
  count="$(sqlite_scalar "SELECT COUNT(*) FROM pragma_table_info('$table') WHERE name='$column';")"
  if [ "$count" != "1" ]; then
    echo "Restore missing $label column: $table.$column" >&2
    exit 1
  fi
  echo "OK   $label column: $table.$column"
}

if [ "$SELF_TEST" = "true" ]; then
  self_test_archive
fi
if [ -z "$ARCHIVE" ]; then
  usage >&2
  exit 2
fi
if [ ! -f "$ARCHIVE" ]; then
  echo "Backup archive not found: $ARCHIVE" >&2
  exit 2
fi
if ! command -v sqlite3 >/dev/null 2>&1; then
  echo "sqlite3 is required for restore verification" >&2
  exit 2
fi

extract_archive "$ARCHIVE"
require_file manifest.json
require_file database.sql

if ! grep -q '"format": "dais-backup-v1"' "$TMP_DIR/extract/manifest.json"; then
  echo "Backup manifest has an unknown format" >&2
  exit 1
fi

if grep -q -- '--skip-cloud used; no D1 export captured' "$TMP_DIR/extract/database.sql"; then
  if [ "$ALLOW_PLACEHOLDER_SQL" = "true" ]; then
    echo "SKIP SQL restore: backup contains a --skip-cloud placeholder"
    exit 0
  fi
  echo "Backup contains placeholder SQL from --skip-cloud; rerun backup without --skip-cloud or pass --allow-placeholder-sql for archive-only checks" >&2
  exit 1
fi

sqlite3 "$TMP_DIR/restore.sqlite" < "$TMP_DIR/extract/database.sql"

require_table actors "actor/profile"
require_table posts "post"
require_column posts media_attachments "media metadata"
require_table followers "follower graph"
require_table following "following graph"
require_table instance_settings "instance settings"
require_table moderation_settings "moderation settings"
require_table blocks "block"
require_table audience_lists "audience"
require_table audience_list_members "audience membership"
require_table source_subscriptions "source/watch"
require_table source_items "source/watch item"
require_table e2ee_devices "local E2EE device"
require_table e2ee_peer_devices "peer E2EE device"
require_table e2ee_messages "encrypted message"

echo "Verified dais restore archive into fresh SQLite database: $ARCHIVE"
