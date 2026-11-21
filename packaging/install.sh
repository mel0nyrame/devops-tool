#!/bin/sh
set -eu

repo=mel0nyrame/devops-tool
tool=${1:-all}
version=${2:-latest}
case "$tool" in all|fsz|rmem) ;; *) echo 'Usage: sh install.sh [all|fsz|rmem] [latest|vX.Y.Z]' >&2; exit 2 ;; esac
case "$(uname -s):$(uname -m)" in
  Linux:x86_64) platform=linux-x64; suffix= ;;
  Darwin:arm64) platform=darwin-arm64; suffix= ;;
  MINGW64*:x86_64|MSYS*:x86_64|UCRT64*:x86_64) platform=win32-x64; suffix=.exe ;;
  *) echo 'Unsupported platform' >&2; exit 1 ;;
esac
if [ "$version" = latest ]; then
  # Pin latest to one Release before fetching assets.
  release=$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")
  version=${release##*/}
fi
printf '%s\n' "$version" | grep -Eq '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' || exit 2
url=https://github.com/$repo/releases/download/$version
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
bindir=${INSTALL_DIR:-"$HOME/.local/bin"}
mkdir -p "$bindir"
curl -fsSL "$url/SHA256SUMS" -o "$scratch/SHA256SUMS"
if [ "$tool" = all ]; then set -- fsz rmem; else set -- "$tool"; fi
for tool do
  asset=$tool-$platform$suffix
  curl -fsSL "$url/$asset" -o "$scratch/$asset"
  expected=$(awk -v asset="$asset" '$2 == asset { print $1 }' "$scratch/SHA256SUMS")
  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$scratch/$asset")
  else
    actual=$(shasum -a 256 "$scratch/$asset")
  fi
  [ "$expected" = "${actual%% *}" ] || { echo "Checksum mismatch: $asset" >&2; exit 1; }
done
for tool do
  install -m 755 "$scratch/$tool-$platform$suffix" "$bindir/$tool$suffix"
  printf 'Installed %s\n' "$bindir/$tool$suffix"
done
printf 'Add %s to PATH if needed.\n' "$bindir"
