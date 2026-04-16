#!/bin/sh
# Called by Meson to build Rust binaries via Cargo.
#
# Usage: cargo-build.sh <cargo> <source-root> <profile> <out-gnomeqs> [out-gnomeqs-tray]
#
# Environment (set by Meson via cargo_env):
#   CARGO_TARGET_DIR    — where cargo writes build artifacts
#   GNOMEQS_LOCALE_DIR  — injected into build.rs for LOCALE_DIR constant
set -eu

CARGO="$1"
SOURCE_ROOT="$2"
PROFILE="$3"
OUTPUT_GNOMEQS="$4"
OUTPUT_TRAY="${5:-}"

set -- build \
    --manifest-path "$SOURCE_ROOT/Cargo.toml" \
    -p gnomeqs

if [ -n "$OUTPUT_TRAY" ]; then
    set -- "$@" -p gnomeqs-tray
fi

if [ "$PROFILE" = "release" ]; then
    set -- "$@" --release
fi

"$CARGO" "$@"

cp "$CARGO_TARGET_DIR/$PROFILE/gnomeqs"       "$OUTPUT_GNOMEQS"

if [ -n "$OUTPUT_TRAY" ]; then
    cp "$CARGO_TARGET_DIR/$PROFILE/gnomeqs-tray"  "$OUTPUT_TRAY"
fi
