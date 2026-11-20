#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
icon_workdir=$(mktemp -d "${TMPDIR:-/tmp}/shrimpapi-icon.XXXXXX")
trap 'rm -rf "$icon_workdir"' EXIT HUP INT TERM
iconset="$icon_workdir/AppIcon.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    retina=$((size * 2))
    sips -z "$size" "$size" assets/brand/icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$retina" "$retina" assets/brand/icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o assets/brand/AppIcon.icns
echo 'Created assets/brand/AppIcon.icns'
