#!/bin/sh
# Explicit allowlist: never publish the workspace, vendor archives or local settings.
set -eu
cd "$(dirname "$0")/.."
mkdir -p dist
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT HUP INT TERM
root="$stage/aula-l99-linux"
mkdir -p "$root/src" "$root/scripts" "$root/.github/workflows"
cp Cargo.toml Cargo.lock README.md LICENSE PUBLISHING.md THIRD_PARTY.md .gitignore install-permissions.sh 70-aula-l99.rules "$root/"
cp src/main.rs src/panel.py src/remap.py src/test_panel.py src/test_remap.py "$root/src/"
cp scripts/security-check.sh scripts/package-source.sh scripts/build-macos.sh scripts/build-windows.ps1 "$root/scripts/"
cp .github/workflows/linux.yml "$root/.github/workflows/"
tar -czf dist/aula-l99-linux-source.tar.gz -C "$stage" aula-l99-linux
printf '%s\n' 'Ready: dist/aula-l99-linux-source.tar.gz'
