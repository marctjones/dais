#!/usr/bin/env bash
#
# dais deployment tool — reusable across the development lifecycle.
#
# Deploys the core-based Cloudflare Worker tree (platforms/cloudflare/workers).
# Supersedes the old hardcoded workers/-tree deploy script.
#
# Usage:
#   scripts/deploy.sh [ACTION] [OPTIONS]
#
# Actions:
#   deploy            Deploy workers (default)
#   build             Build/package only, no upload (wrangler --dry-run) — CI-friendly
#   list              List workers and their config/entrypoint status
#   tail              Stream live logs for a worker (requires --only)
#
# Options:
#   -e, --env ENV     Target environment: dev (default) | production
#   -w, --only NAME   Act on a single worker (e.g. --only actor)
#   -n, --dry-run     Deploy as a dry run (build + validate, no upload)
#   -k, --keep-going  Continue after a worker fails (default: stop on first error)
#   -y, --yes         Skip the confirmation prompt for production
#   -h, --help        Show this help
#
# Examples:
#   scripts/deploy.sh list                       # what's deployable
#   scripts/deploy.sh build                       # validate all workers build for deploy
#   scripts/deploy.sh deploy                       # deploy all to dev (staging) env
#   scripts/deploy.sh deploy --only actor          # redeploy one worker to dev
#   scripts/deploy.sh deploy --env production       # deploy all to production (prompts)
#   scripts/deploy.sh tail --only inbox --env production
#
set -euo pipefail

# --- config ------------------------------------------------------------------
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKERS_DIR="$ROOT/platforms/cloudflare/workers"
WRANGLER="$ROOT/node_modules/.bin/wrangler"
[ -x "$WRANGLER" ] || WRANGLER="wrangler"
BUILD_PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:$PATH"

# Deploy order: backends first, router LAST (it proxies to the others' URLs).
WORKERS=(webfinger actor inbox outbox pds delivery-queue auth landing router)

# --- defaults ----------------------------------------------------------------
ACTION="deploy"
ENVIRONMENT="dev"
ONLY=""
DRY_RUN="false"
KEEP_GOING="false"
ASSUME_YES="false"

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
    -y|--yes)        ASSUME_YES="true"; shift ;;
    -h|--help)       usage; exit 0 ;;
    *) err "Unknown argument: $1"; echo; usage; exit 2 ;;
  esac
done

case "$ENVIRONMENT" in
  dev|production) ;;
  *) err "Invalid --env '$ENVIRONMENT' (expected: dev | production)"; exit 2 ;;
esac

# Resolve the worker list (single or all), validating --only.
if [ -n "$ONLY" ]; then
  found="false"; for w in "${WORKERS[@]}"; do [ "$w" = "$ONLY" ] && found="true"; done
  [ "$found" = "true" ] || { err "Unknown worker '$ONLY'. Known: ${WORKERS[*]}"; exit 2; }
  TARGETS=("$ONLY")
else
  TARGETS=("${WORKERS[@]}")
fi

env_flag() { [ "$ENVIRONMENT" = "production" ] && printf -- "--env production" || printf ""; }

require_wrangler() {
  if [ "$WRANGLER" = "wrangler" ] && ! command -v wrangler >/dev/null 2>&1; then
    err "wrangler not found (run: npm install)"
    exit 1
  fi
  if ! "$WRANGLER" whoami >/dev/null 2>&1; then
    warn "Not logged in to Cloudflare - run: npx wrangler login"; exit 1
  fi
}

confirm_production() {
  [ "$ENVIRONMENT" = "production" ] || return 0
  [ "$ACTION" = "build" ] && return 0
  [ "$DRY_RUN" = "true" ] && return 0
  [ "$ASSUME_YES" = "true" ] && return 0
  warn "About to deploy to PRODUCTION: ${TARGETS[*]}"
  read -r -p "Type 'yes' to continue: " reply
  [ "$reply" = "yes" ] || { err "Aborted."; exit 1; }
}

# --- actions -----------------------------------------------------------------
do_list() {
  info "Workers in $WORKERS_DIR (deploy order):"
  for w in "${WORKERS[@]}"; do
    dir="$WORKERS_DIR/$w"
    cfg="missing"; [ -f "$dir/wrangler.toml" ] && cfg="ok"
    entry="?"
    [ -f "$dir/src/index.js" ] && entry="js"
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
  ( cd "$dir" && PATH="$BUILD_PATH" "$WRANGLER" deploy $flags )
}

do_deploy() {
  require_wrangler
  confirm_production
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
