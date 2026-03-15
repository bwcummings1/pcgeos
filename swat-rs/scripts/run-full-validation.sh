#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

run() {
  echo "+ $*"
  "$@"
}

echo "== Queue and generated status =="
run python3 scripts/check-implementation-status.py
run python3 scripts/render-implementation-status.py --check

echo
echo "== Workspace tests =="
run cargo test

echo
echo "== v1 baseline demos =="
run cargo run -q -p swat-session --example mock_session
run cargo run -q -p swat-session --example local_process
run cargo run -q -p swat-session --example python_trace
run cargo run -q -p swat-session --example agent_trace
printf 'attach\nresume\npump\npump\ndashboard execution\nhistory 8\nexit\n' | run cargo run -q -p swat-command -- mock
run cargo run -q -p swat-ui-tui -- --headless --ticks 4 mock
run cargo run -q -p swat-agent-protocol --example emit_protocol
run python3 sdk/python/examples/emit_protocol.py
run bun run sdk/typescript/examples/emit_protocol.ts

echo
echo "== Full-completion PC/GEOS walkthroughs =="
run bash scripts/demo-pcgeos-workflows.sh
