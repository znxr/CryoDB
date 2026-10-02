#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:?rust target triple required}"
VERSION="${2:?version required}"
ARCH_LABEL="${3:?arch label required (arm64 or x64)}"

BINARY="target/${TARGET}/release/cryodb"
[ -x "$BINARY" ] || { echo "missing $BINARY — build first" >&2; exit 1; }

ROOT="$(pwd)"
STAGE="packaging/macos/stage"
APP="${STAGE}/root/Applications/CryoDB.app"
SCRIPTS="${STAGE}/scripts"

rm -rf "$STAGE"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" \
         "${STAGE}/root/usr/local/bin" "$SCRIPTS" dist

cp "$BINARY" "$APP/Contents/MacOS/cryodb"
chmod +x "$APP/Contents/MacOS/cryodb"

ICONSET="${STAGE}/cryodb.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" icons/icon.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z "$((size * 2))" "$((size * 2))" icons/icon.png \
    --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/cryodb.icns"
test -s "$APP/Contents/Resources/cryodb.icns"

cat > "$APP/Contents/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>cryodb</string>
  <key>CFBundleIdentifier</key><string>dev.znxr.cryodb</string>
  <key>CFBundleName</key><string>CryoDB</string>
  <key>CFBundleDisplayName</key><string>CryoDB</string>
  <key>CFBundleIconFile</key><string>cryodb.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHumanReadableCopyright</key><string>Copyright © Mauricio Orquin — https://cryodb.znxr.dev</string>
</dict></plist>
PLIST

codesign --force --deep -s - "$APP"

ln -sf /Applications/CryoDB.app/Contents/MacOS/cryodb \
  "${STAGE}/root/usr/local/bin/cryodb"

cat > "${SCRIPTS}/postinstall" << 'POSTINSTALL'
#!/bin/bash
set -e

BUNDLE="${2%/}/Applications/CryoDB.app"
OWNER="$(stat -f %Su /dev/console 2>/dev/null || true)"
[ -n "$OWNER" ] || OWNER="${USER:-}"

if [ -d "$BUNDLE" ] && [ -n "$OWNER" ] && [ "$OWNER" != "root" ]; then
  chown -R "${OWNER}:staff" "$BUNDLE"
fi

exit 0
POSTINSTALL
chmod +x "${SCRIPTS}/postinstall"

pkgbuild \
  --root "${STAGE}/root" \
  --scripts "$SCRIPTS" \
  --identifier dev.znxr.cryodb \
  --version "$VERSION" \
  --install-location / \
  "dist/CryoDB-macos-${ARCH_LABEL}.pkg"

tar -czf "${ROOT}/dist/cryodb-macos-${ARCH_LABEL}.tar.gz" \
  -C "${STAGE}/root/Applications" CryoDB.app

echo "built:"
ls -la dist
