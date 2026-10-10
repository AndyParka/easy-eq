#!/usr/bin/env sh
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
npm run build
cargo build --manifest-path src-tauri/Cargo.toml --locked --features custom-protocol "$@"
