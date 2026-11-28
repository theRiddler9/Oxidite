#!/usr/bin/env bash
# Builds the web client and gateway, starts a throwaway `oxidited` (fresh DB, random token, loopback),
# runs ui-smoke.py against it, then stops it. Needs: bun, cargo, python3 with `playwright` installed.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PORT="${OXIDITE_E2E_PORT:-3030}"
WORK="$(mktemp -d)"
TOKEN="e2e-$(head -c 12 /dev/urandom | od -An -tx1 | tr -d ' \n')"
GW_PID=""
cleanup() {
  [ -n "$GW_PID" ] && kill "$GW_PID" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

(cd "$ROOT/web" && bun install --frozen-lockfile && bun run build)
(cd "$ROOT" && cargo build --locked -p oxidite-server --bin oxidited)

cd "$ROOT"
OXIDITE_TOKEN="$TOKEN" OXIDITE_DB_PATH="$WORK/e2e.db" OXIDITE_LOG_DIR="$WORK/logs" \
  OXIDITE_ALLOW_LOCAL_PTY=1 OXIDITE_WORKSPACE_ROOTS="$ROOT" OXIDITE_PORT="$PORT" \
  ./target/debug/oxidited >"$WORK/gateway.log" 2>&1 &
GW_PID=$!

for _ in $(seq 1 50); do
  curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 && break
  kill -0 "$GW_PID" 2>/dev/null || { echo "gateway exited early:"; cat "$WORK/gateway.log"; exit 1; }
  sleep 0.5
done

status=0
OXIDITE_E2E_URL="http://127.0.0.1:$PORT" OXIDITE_E2E_TOKEN="$TOKEN" \
  OXIDITE_E2E_OUT="${OXIDITE_E2E_OUT:-$ROOT/e2e-out}" \
  python3 "$ROOT/scripts/e2e/ui-smoke.py" || status=$?
[ "$status" -ne 0 ] && { echo "--- gateway log (tail) ---"; tail -n 30 "$WORK/gateway.log"; }
exit "$status"
