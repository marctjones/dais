#!/usr/bin/env bash
#
# Apply the D1 schema updates needed by the current server release.
#
# The production/skpt D1 databases predate Wrangler's migration tracking, so
# `wrangler d1 migrations apply` would try to replay the whole migration tree.
# This script intentionally applies only the selected idempotent SQL files.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WRANGLER="${WRANGLER:-wrangler}"
ENVIRONMENT="production"
DRY_RUN="false"
CONFIG="platforms/cloudflare/workers/router/wrangler.toml"
MIGRATIONS=()

usage() {
  sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
  cat <<'USAGE'

Usage:
  scripts/apply-release-d1-migrations.sh --env production [migration.sql ...]
  scripts/apply-release-d1-migrations.sh --env skpt [migration.sql ...]

Options:
  -e, --env ENV     production | skpt
  -n, --dry-run     Print commands without applying SQL
  -h, --help        Show this help

If no migration files are provided, D1_RELEASE_MIGRATIONS is used when set;
otherwise the current release defaults are applied.
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    -e|--env) ENVIRONMENT="${2:-}"; shift 2 ;;
    -n|--dry-run) DRY_RUN="true"; shift ;;
    -h|--help) usage; exit 0 ;;
    --) shift; break ;;
    -*) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
    *) MIGRATIONS+=("$1"); shift ;;
  esac
done

while [ $# -gt 0 ]; do
  MIGRATIONS+=("$1")
  shift
done

case "$ENVIRONMENT" in
  production) DATABASE="dais-social" ;;
  skpt) DATABASE="dais-skpt" ;;
  *) echo "Invalid --env '$ENVIRONMENT' (expected production | skpt)" >&2; exit 2 ;;
esac

if [ "${#MIGRATIONS[@]}" -eq 0 ]; then
  if [ -n "${D1_RELEASE_MIGRATIONS:-}" ]; then
    read -r -a MIGRATIONS <<< "$D1_RELEASE_MIGRATIONS"
  else
    MIGRATIONS=(
      "cli/migrations/030_mutes.sql"
      "cli/migrations/031_atproto_sync_commits.sql"
      "cli/migrations/032_private_groups.sql"
    )
  fi
fi

for migration in "${MIGRATIONS[@]}"; do
  path="$ROOT/$migration"
  if [ ! -f "$path" ]; then
    echo "Migration file not found: $migration" >&2
    exit 1
  fi
done

echo "D1 release migrations for $ENVIRONMENT ($DATABASE):"
for migration in "${MIGRATIONS[@]}"; do
  echo "  - $migration"
done

migration_already_applied() {
  local migration="$1"
  case "$migration" in
    cli/migrations/032_private_groups.sql)
      (cd "$ROOT" && "$WRANGLER" d1 execute "$DATABASE" --remote --config "$CONFIG" --env "$ENVIRONMENT" --command "SELECT group_type, membership_visibility, posting_policy FROM audience_lists LIMIT 0") >/dev/null 2>&1
      ;;
    *)
      return 1
      ;;
  esac
}

for migration in "${MIGRATIONS[@]}"; do
  if [ "$DRY_RUN" = "true" ]; then
    echo "DRY RUN: $WRANGLER d1 execute $DATABASE --remote --config $CONFIG --env $ENVIRONMENT --file $migration"
  elif migration_already_applied "$migration"; then
    echo "Already applied: $migration"
  else
    (cd "$ROOT" && "$WRANGLER" d1 execute "$DATABASE" --remote --config "$CONFIG" --env "$ENVIRONMENT" --file "$migration")
  fi
done
