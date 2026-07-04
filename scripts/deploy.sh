#!/usr/bin/env bash
#
# dais deployment tool — reusable across the development lifecycle.
#
# Deploys the active Cloudflare Worker tree (platforms/cloudflare/workers).
# Landing, router, and PDS are the default deploy targets. Other split workers
# remain available for compatibility/emergency rollback, but must be requested
# explicitly.
#
# Usage:
#   scripts/deploy.sh [ACTION] [OPTIONS]
#
# Actions:
#   deploy            Deploy workers (default)
#   build             Build/package only, no upload (wrangler --dry-run) - CI-friendly
#   list              List workers and their config/entrypoint status
#   tail              Stream live logs for a worker (requires --only)
#
# Options:
#   -e, --env ENV     Target environment: dev (default) | production | skpt
#   -w, --only NAME   Act on a single worker (e.g. --only actor)
#   -n, --dry-run     Deploy as a dry run (build + validate, no upload)
#   -k, --keep-going  Continue after a worker fails (default: stop on first error)
#   --include-legacy  Include legacy split workers in all-worker deploy/build
#   -y, --yes         Skip the confirmation prompt for production/skpt deploys
#   -h, --help        Show this help
#
# Examples:
#   scripts/deploy.sh list                       # what's deployable
#   scripts/deploy.sh build                       # validate active workers build for deploy
#   scripts/deploy.sh deploy                       # deploy active workers to dev
#   scripts/deploy.sh deploy --only router          # redeploy one active worker to dev
#   scripts/deploy.sh deploy --env production       # deploy active workers to production
#   scripts/deploy.sh deploy --include-legacy       # deploy active + legacy workers
#   scripts/deploy.sh tail --only router --env production
#
set -euo pipefail

# --- config ------------------------------------------------------------------
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKERS_DIR="$ROOT/platforms/cloudflare/workers"
WRANGLER="${WRANGLER:-wrangler}"
BUILD_PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:$PATH"
BUILD_RUSTC=""
if command -v rustup >/dev/null 2>&1; then
  BUILD_RUSTC="$(rustup which rustc 2>/dev/null || true)"
fi

# Active deploy order.
WORKERS=(landing router pds)

# Legacy split workers. Router owns the ActivityPub/owner API surface, and PDS
# owns pds.dais.social; the rest are retained for compatibility, historical
# configs, and emergency rollback.
LEGACY_WORKERS=(webfinger actor inbox outbox delivery-queue auth)

# --- defaults ----------------------------------------------------------------
ACTION="deploy"
ENVIRONMENT="dev"
ONLY=""
DRY_RUN="false"
KEEP_GOING="false"
ASSUME_YES="false"
INCLUDE_LEGACY="false"

# --- colors ------------------------------------------------------------------
if [ -t 1 ]; then
  G='\033[0;32m'; B='\033[0;34m'; Y='\033[1;33m'; R='\033[0;31m'; D='\033[2m'; N='\033[0m'
else
  G=''; B=''; Y=''; R=''; D=''; N=''
fi
info() { echo -e "${B}$*${N}"; }
ok()   { echo -e "${G}$*${N}"; }
warn() { echo -e "${Y}$*${N}"; }
err()  { echo -e "${R}$*${N}" >&2; }

usage() { sed -n '2,40p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

# --- arg parsing -------------------------------------------------------------
while [ $# -gt 0 ]; do
  case "$1" in
    deploy|build|list|tail) ACTION="$1"; shift ;;
    -e|--env)        ENVIRONMENT="${2:-}"; shift 2 ;;
    -w|--only)       ONLY="${2:-}"; shift 2 ;;
    -n|--dry-run)    DRY_RUN="true"; shift ;;
    -k|--keep-going) KEEP_GOING="true"; shift ;;
    --include-legacy) INCLUDE_LEGACY="true"; shift ;;
    -y|--yes)        ASSUME_YES="true"; shift ;;
    -h|--help)       usage; exit 0 ;;
    *) err "Unknown argument: $1"; echo; usage; exit 2 ;;
  esac
done

case "$ENVIRONMENT" in
  dev|production|skpt) ;;
  *) err "Invalid --env '$ENVIRONMENT' (expected: dev | production | skpt)"; exit 2 ;;
esac

ALL_WORKERS=("${WORKERS[@]}" "${LEGACY_WORKERS[@]}")

# Resolve the worker list (single or all), validating --only.
if [ -n "$ONLY" ]; then
  found="false"; for w in "${ALL_WORKERS[@]}"; do [ "$w" = "$ONLY" ] && found="true"; done
  [ "$found" = "true" ] || { err "Unknown worker '$ONLY'. Known: ${ALL_WORKERS[*]}"; exit 2; }
  TARGETS=("$ONLY")
elif [ "$INCLUDE_LEGACY" = "true" ]; then
  TARGETS=("${LEGACY_WORKERS[@]}" "${WORKERS[@]}")
else
  TARGETS=("${WORKERS[@]}")
fi

env_flag() { [ "$ENVIRONMENT" != "dev" ] && printf -- "--env %s" "$ENVIRONMENT" || printf ""; }

require_wrangler() {
  if ! command -v "$WRANGLER" >/dev/null 2>&1 && [ ! -x "$WRANGLER" ]; then
    err "wrangler not found. Install the Cloudflare Wrangler CLI and ensure it is on PATH, or set WRANGLER=/path/to/wrangler."
    exit 1
  fi
  if ! "$WRANGLER" whoami >/dev/null 2>&1; then
    warn "Not logged in to Cloudflare - run: wrangler login"; exit 1
  fi
}

confirm_remote_deploy() {
  [ "$ENVIRONMENT" = "dev" ] && return 0
  [ "$ACTION" = "build" ] && return 0
  [ "$DRY_RUN" = "true" ] && return 0
  [ "$ASSUME_YES" = "true" ] && return 0
  warn "About to deploy to $ENVIRONMENT: ${TARGETS[*]}"
  read -r -p "Type 'yes' to continue: " reply
  [ "$reply" = "yes" ] || { err "Aborted."; exit 1; }
}

check_delivery_queue_consumer() {
  [ "$ENVIRONMENT" != "dev" ] || return 0
  [ "$ACTION" = "deploy" ] || return 0
  [ "$DRY_RUN" = "true" ] && return 0

  local needs_delivery_queue="false"
  for w in "${TARGETS[@]}"; do
    [ "$w" = "delivery-queue" ] && needs_delivery_queue="true"
  done
  [ "$needs_delivery_queue" = "true" ] || return 0

  local consumers
  local queue_name="delivery-queue"
  [ "$ENVIRONMENT" = "skpt" ] && queue_name="delivery-queue-skpt"

  consumers="$("$WRANGLER" queues consumer list "$queue_name" 2>/dev/null || true)"
  if printf "%s\n" "$consumers" | grep -q "${queue_name}[[:space:]]"; then
    err "$queue_name has a stale consumer attached to script '$queue_name'."
    err "Remove it before production deploy:"
    err "  wrangler queues consumer remove $queue_name $queue_name"
    err "Then redeploy delivery-queue-$ENVIRONMENT."
    exit 1
  fi
}

# --- actions -----------------------------------------------------------------
do_list() {
  info "Active workers in $WORKERS_DIR (default deploy order):"
  for w in "${WORKERS[@]}"; do
    dir="$WORKERS_DIR/$w"
    cfg="missing"; [ -f "$dir/wrangler.toml" ] && cfg="ok"
    entry="?"
    [ -f "$dir/Cargo.toml" ] && entry="rust"
    printf "  %-16s wrangler:%-8s entry:%s\n" "$w" "$cfg" "$entry"
  done
  echo
  info "Legacy split workers (deploy with --include-legacy or --only <worker>):"
  for w in "${LEGACY_WORKERS[@]}"; do
    dir="$WORKERS_DIR/$w"
    cfg="missing"; [ -f "$dir/wrangler.toml" ] && cfg="ok"
    entry="?"
    [ -f "$dir/Cargo.toml" ] && entry="rust"
    printf "  %-16s wrangler:%-8s entry:%s\n" "$w" "$cfg" "$entry"
  done
}

# Run one worker through deploy/build. Returns wrangler's exit code.
run_one() {
  local w="$1" mode="$2" dir="$WORKERS_DIR/$w"
  if [ ! -f "$dir/wrangler.toml" ]; then warn "  skip $w (no wrangler.toml)"; return 0; fi
  local flags; flags="$(env_flag)"
  [ "$DRY_RUN" = "true" ] && flags="$flags --dry-run"
  [ "$mode" = "build" ] && flags="$flags --dry-run"
  info ">> $w  (env=$ENVIRONMENT${flags:+,$flags})"
  if [ -n "$BUILD_RUSTC" ]; then
    ( cd "$dir" && PATH="$BUILD_PATH" RUSTC="$BUILD_RUSTC" "$WRANGLER" deploy $flags )
  else
    ( cd "$dir" && PATH="$BUILD_PATH" "$WRANGLER" deploy $flags )
  fi
}

do_deploy() {
  require_wrangler
  confirm_remote_deploy
  check_delivery_queue_consumer
  local failed=() deployed=()
  for w in "${TARGETS[@]}"; do
    if run_one "$w" "$ACTION"; then
      deployed+=("$w")
    else
      failed+=("$w")
      err "  ✗ $w failed"
      [ "$KEEP_GOING" = "true" ] || { err "Stopping (use --keep-going to continue)."; break; }
    fi
  done
  echo
  ok "Succeeded (${#deployed[@]}): ${deployed[*]:-none}"
  [ ${#failed[@]} -gt 0 ] && { err "Failed (${#failed[@]}): ${failed[*]}"; return 1; }
  return 0
}

do_tail() {
  require_wrangler
  [ -n "$ONLY" ] || { err "tail requires --only <worker>"; exit 2; }
  local dir="$WORKERS_DIR/$ONLY"
  info "Tailing $ONLY (env=$ENVIRONMENT) — Ctrl-C to stop"
  ( cd "$dir" && PATH="$BUILD_PATH" "$WRANGLER" tail $(env_flag) )
}

case "$ACTION" in
  list)          do_list ;;
  deploy|build)  do_deploy ;;
  tail)          do_tail ;;
  *) err "Unknown action: $ACTION"; usage; exit 2 ;;
esac
