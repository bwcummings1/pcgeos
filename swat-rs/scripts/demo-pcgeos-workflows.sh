#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHOW_PATH="$(cd "$ROOT/../Appl/GeoPoint" && pwd -P)/show.goc"

cd "$ROOT"

echo "== PC/GEOS shell walkthrough =="
printf 'attach\nstatus\ndashboard control\npatient show geopoint\nhandle show geopoint.app:handle:ShowResource\nresource show geopoint.app:ShowResource\nobject show GeoPointDocument\nstack\nsource file %s\nresume\npump\nvalue show pcgeos.memory.slide:0x0020\nhistory 12\nexit\n' \
  "$SHOW_PATH" \
  | cargo run -q -p swat-command -- pcgeos

echo
echo "== PC/GEOS TUI headless walkthrough =="
cargo run -q -p swat-ui-tui -- --headless --ticks 2 pcgeos
