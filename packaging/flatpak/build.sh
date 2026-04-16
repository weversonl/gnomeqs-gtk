#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
manifest="$root/packaging/flatpak/io.github.weversonl.GnomeQuickShare.json"
local_manifest="$root/packaging/flatpak/.local-io.github.weversonl.GnomeQuickShare.json"
source_dir="$root/packaging/out/flatpak/source"
build_dir="$root/packaging/out/flatpak/build"
repo_dir="$root/packaging/out/flatpak/repo"
bundle="$root/packaging/out/flatpak/io.github.weversonl.GnomeQuickShare.flatpak"

cleanup() {
  rm -f "$local_manifest"
}
trap cleanup EXIT

rm -rf "$source_dir" "$build_dir" "$repo_dir" "$bundle"

rsync -a --delete \
  --exclude '.git/' \
  --exclude '.flatpak-builder/' \
  --exclude 'packaging/out/' \
  --exclude 'target/' \
  "$root/" "$source_dir/"

jq \
  --arg source_dir "$source_dir" \
  '.modules[0].sources[0] = {"type": "dir", "path": $source_dir}' \
  "$manifest" > "$local_manifest"

flatpak-builder \
  --force-clean \
  --repo="$repo_dir" \
  "$build_dir" \
  "$local_manifest"

flatpak build-bundle "$repo_dir" "$bundle" io.github.weversonl.GnomeQuickShare
echo "Built $bundle"
