#!/usr/bin/env bash
# Third-party notices for the server and roast (docs/DEVELOPMENT.md, "Licences"): cargo-about
# lists the crates in roastwarden-server's graph (about.toml) and scripts/notices.py writes
# <dir>/server.txt with their licences, NOTICE files and the embedded fonts. The image build
# runs it (Dockerfile, the notices stage); the dashboard's build writes dashboard.txt beside it.
#
#   scripts/notices.sh [dir]     default apps/dashboard/build/licenses (a local dashboard build)
#
# Needs cargo-about (the dev shell has it) and the crates downloaded (cargo fetch); offline, so
# a crate's text comes from its own files, never from clearlydefined.io.
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:-apps/dashboard/build/licenses}"
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
cargo about generate --frozen -m crates/server/Cargo.toml --format json -o "$tmp"
python3 scripts/notices.py "$tmp" "$out/server.txt"
echo "wrote $out/server.txt ($(wc -c < "$out/server.txt") bytes)"
