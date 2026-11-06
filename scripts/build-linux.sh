#!/usr/bin/env bash
# build-linux.sh -- Builds the Open Herd Linux packages (the Linux counterpart
# of build-portable.ps1, which stays the Windows build).
#
#   release/open-herd-vX.Y.Z-amd64.deb        Debian / Ubuntu  (sudo apt install ./file.deb)
#   release/open-herd-vX.Y.Z-x86_64.AppImage  any distro       (chmod +x and run)
#
# Data lives in ~/.phpenv, as with the Windows installer.
#
# Needs the Tauri system libraries once:
#   sudo apt install build-essential pkg-config libssl-dev libwebkit2gtk-4.1-dev \
#        libxdo-dev libayatana-appindicator3-dev librsvg2-dev file xz-utils xdg-utils
# plus Rust (rustup) and Node (same major as Windows, see package.json).
#
# Usage: scripts/build-linux.sh [--deb-only]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

bundles="deb,appimage"
case "${1:-}" in
  --deb-only) bundles="deb" ;;
  -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
  "") ;;
  *) echo "Unknown option: $1 (try --help)" >&2; exit 2 ;;
esac

# Version: Cargo.toml is the single source of truth (as in build-portable.ps1).
version="$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' src-tauri/Cargo.toml | head -1)"
if [ -z "$version" ]; then echo "ERROR: could not read the version from src-tauri/Cargo.toml" >&2; exit 1; fi

missing=()
for pc in webkit2gtk-4.1 gtk+-3.0 librsvg-2.0 openssl; do
  pkg-config --exists "$pc" 2>/dev/null || missing+=("$pc")
done
if [ ${#missing[@]} -gt 0 ]; then
  echo "ERROR: missing system libraries: ${missing[*]}" >&2
  sed -n '10,13p' "$0" >&2
  exit 1
fi
tools=(cargo node npm)
# The AppImage bundles xdg-open (package xdg-utils) to open links and folders.
[[ "$bundles" == *appimage* ]] && tools+=(xdg-open)
for tool in "${tools[@]}"; do
  command -v "$tool" >/dev/null || { echo "ERROR: $tool not found on the PATH (system packages: see the top of this script)" >&2; exit 1; }
done

echo "Open Herd -- Linux build v$version ($bundles)"
echo "  node  $(node --version)   npm $(npm --version)   $(cargo --version)"

[ -d node_modules ] || npm ci --no-audit --no-fund

# AppImage tooling (linuxdeploy) is itself an AppImage; this makes it run
# without FUSE (WSL, containers).
export APPIMAGE_EXTRACT_AND_RUN=1

npx tauri build --bundles "$bundles"

bundle_dir="src-tauri/target/release/bundle"
mkdir -p release
# Keep earlier releases: only replace this version's Linux artifacts.
rm -f "release/open-herd-v$version-amd64.deb" "release/open-herd-v$version-x86_64.AppImage"

deb="$(ls -t "$bundle_dir"/deb/*.deb 2>/dev/null | head -1 || true)"
if [ -n "$deb" ]; then
  cp "$deb" "release/open-herd-v$version-amd64.deb"
  echo "  .deb      release/open-herd-v$version-amd64.deb ($(du -h "$deb" | cut -f1))"
else
  echo "ERROR: no .deb in $bundle_dir/deb" >&2; exit 1
fi

if [[ "$bundles" == *appimage* ]]; then
  appimage="$(ls -t "$bundle_dir"/appimage/*.AppImage 2>/dev/null | head -1 || true)"
  if [ -n "$appimage" ]; then
    cp "$appimage" "release/open-herd-v$version-x86_64.AppImage"
    chmod +x "release/open-herd-v$version-x86_64.AppImage"
    echo "  AppImage  release/open-herd-v$version-x86_64.AppImage ($(du -h "$appimage" | cut -f1))"
  else
    echo "ERROR: no AppImage in $bundle_dir/appimage" >&2; exit 1
  fi
fi
