#!/usr/bin/env bash
# Builds dist/Shizuka.dmg as a universal (Apple Silicon + Intel) app bundle.
set -euo pipefail
cd "$(dirname "$0")/.."

# Prefer the rustup toolchain: Homebrew's rust does not have the Intel target.
if command -v rustup >/dev/null 2>&1; then
    export PATH="$(dirname "$(rustup which cargo)"):$PATH"
fi

APP_NAME="Shizuka"
BUNDLE_ID="com.mlam31.shizuka"
VERSION="0.1.0"
DIST="dist"
APP="$DIST/$APP_NAME.app"
DMG="$DIST/$APP_NAME.dmg"

mkdir -p "$DIST"

cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

lipo -create \
    "target/aarch64-apple-darwin/release/mobile_app" \
    "target/x86_64-apple-darwin/release/mobile_app" \
    -output "$DIST/mobile_app-universal"

rm -rf "$APP" "$DMG"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$DIST/mobile_app-universal" "$APP/Contents/MacOS/$APP_NAME"
chmod +x "$APP/Contents/MacOS/$APP_NAME"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleExecutable</key>
    <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.education</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF
printf 'APPL????' > "$APP/Contents/PkgInfo"

# Ad-hoc signature: required on Apple Silicon. Not notarized (no paid account),
# so first launch needs right-click -> Open on downloaded copies.
codesign --force --deep --sign - "$APP"

# Drag-to-Applications layout.
STAGING="$DIST/dmg-root"
rm -rf "$STAGING"
mkdir -p "$STAGING"
cp -R "$APP" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

hdiutil create -volname "$APP_NAME" -srcfolder "$STAGING" -ov -format UDZO "$DMG"
rm -rf "$STAGING" "$DIST/mobile_app-universal"

echo "Built $DMG"
