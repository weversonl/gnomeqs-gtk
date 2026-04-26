#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

VERSION="$(grep "^  version:" "$PROJECT_ROOT/meson.build" | head -1 | sed "s/.*version: *'//;s/'.*//;s/,//")"

echo ">>> Building GnomeQS $VERSION AppImage (inside Ubuntu container)"

docker run --rm \
  --privileged \
  -e GNOMEQS_VERSION="$VERSION" \
  -v "$PROJECT_ROOT":/src \
  -v "$SCRIPT_DIR/AppDir":/appdir \
  -w /src \
  ubuntu:rolling \
  bash /src/packaging/appimage/build-inside-container.sh

echo ""
echo ">>> Done: dist/GnomeQS-$VERSION-x86_64.AppImage"
