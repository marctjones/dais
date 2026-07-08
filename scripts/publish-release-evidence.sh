#!/usr/bin/env bash
#
# Package release-gate evidence and optionally upload it to a GitHub Release.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG=""
REPORT_DIR=""
DRY_RUN="false"

usage() {
  cat <<'USAGE'
Usage:
  scripts/publish-release-evidence.sh --tag TAG --report-dir DIR [--dry-run]

Options:
  --tag TAG         GitHub release tag that should receive the evidence archive
  --report-dir DIR  Directory containing release-gate reports/logs
  --dry-run         Create the archive but do not upload it
  -h, --help        Show this help
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --tag) TAG="${2:-}"; shift 2 ;;
    --report-dir) REPORT_DIR="${2:-}"; shift 2 ;;
    --dry-run) DRY_RUN="true"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ -z "$TAG" ] || [ -z "$REPORT_DIR" ]; then
  usage >&2
  exit 2
fi
if [ ! -d "$REPORT_DIR" ]; then
  echo "Report directory not found: $REPORT_DIR" >&2
  exit 2
fi

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_DIR="$ROOT/tmp/release-evidence"
mkdir -p "$OUT_DIR"
ARCHIVE="$OUT_DIR/dais-${TAG}-release-evidence-${STAMP}.tar.gz"

tar -czf "$ARCHIVE" -C "$(dirname "$REPORT_DIR")" "$(basename "$REPORT_DIR")"
echo "Created evidence archive: $ARCHIVE"

if [ "$DRY_RUN" = "true" ]; then
  echo "Dry run: not uploading evidence archive."
  exit 0
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "gh is required to upload release evidence" >&2
  exit 2
fi

gh release upload "$TAG" "$ARCHIVE" --clobber
echo "Uploaded evidence archive to GitHub Release $TAG"
