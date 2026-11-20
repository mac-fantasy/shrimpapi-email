#!/bin/sh
# Build the native .app; release archives use --universal.
set -eu
cd "$(dirname "$0")/.."
if [ "$(uname -s)" != Darwin ]; then
    echo 'This script must run on macOS.' >&2
    exit 1
fi
profile=release
universal=false
for option in "$@"; do
    case "$option" in
        --debug) profile=debug ;;
        --universal) universal=true ;;
        *) echo 'Usage: sh scripts/bundle-macos.sh [--debug] [--universal]' >&2; exit 1 ;;
    esac
done
export MACOSX_DEPLOYMENT_TARGET=11.0
build_target() {
    if [ "$profile" = release ]; then
        cargo build --locked --release --target "$1"
    else
        cargo build --locked --target "$1"
    fi
}
if [ "$universal" = true ]; then
    build_target aarch64-apple-darwin
    build_target x86_64-apple-darwin
else
    if [ "$profile" = release ]; then cargo build --locked --release; else cargo build --locked; fi
fi
bundle="target/ShrimpAPI Email.app"
mkdir -p "$bundle/Contents/MacOS"
if [ "$universal" = true ]; then
    lipo -create "target/aarch64-apple-darwin/$profile/shrimpapi-email" \
        "target/x86_64-apple-darwin/$profile/shrimpapi-email" \
        -output "$bundle/Contents/MacOS/shrimpapi-email"
else
    cp "target/$profile/shrimpapi-email" "$bundle/Contents/MacOS/shrimpapi-email"
fi
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>shrimpapi-email</string>
  <key>CFBundleIdentifier</key><string>com.shrimpapi.email</string>
  <key>CFBundleName</key><string>ShrimpAPI Email</string>
  <key>CFBundleDisplayName</key><string>ShrimpAPI Email</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.0.0</string>
  <key>CFBundleVersion</key><string>0.0.0</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
app_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml)
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $app_version" "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $app_version" "$bundle/Contents/Info.plist"
codesign --force --sign - --identifier com.shrimpapi.email "$bundle"
codesign --verify --deep --strict "$bundle"
echo "Created $bundle"
