#!/bin/sh
# Create a verified, universal release archive and SHA-256 manifest.
set -eu
cd "$(dirname "$0")/.."
app_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml)
if [ -n "${RELEASE_TAG:-}" ] && [ "$RELEASE_TAG" != "v$app_version" ]; then
    echo "Release tag must match Cargo.toml: v$app_version" >&2
    exit 1
fi
sh scripts/bundle-macos.sh --universal
bundle="target/ShrimpAPI Email.app"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$bundle/Contents/Info.plist")" = "$app_version"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$bundle/Contents/Info.plist")" = com.shrimpapi.email
lipo "$bundle/Contents/MacOS/shrimpapi-email" -verify_arch arm64
lipo "$bundle/Contents/MacOS/shrimpapi-email" -verify_arch x86_64
codesign --verify --deep --strict "$bundle"
mkdir -p dist
archive="ShrimpAPI.Email-$app_version-macos-universal.zip"
ditto -c -k --sequesterRsrc --keepParent "$bundle" "dist/$archive"
(
    cd dist
    shasum -a 256 "$archive" > SHA256SUMS
    shasum -a 256 -c SHA256SUMS
)
echo "Release package: dist/$archive"
