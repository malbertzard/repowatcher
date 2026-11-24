#!/usr/bin/env bash
# Build the Leptos UI: Tailwind CSS first, then trunk (needs a WASM linker, lld).
set -euo pipefail
cd "$(dirname "$0")"
npm run css
trunk build --release
