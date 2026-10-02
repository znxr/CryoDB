#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?version required}"
TARGET="${2:-}"

BINARY="target/release/cryodb"
[ -n "$TARGET" ] && BINARY="target/${TARGET}/release/cryodb"
[ -x "$BINARY" ] || { echo "missing $BINARY — build first" >&2; exit 1; }

APPDIR="packaging/linux/CryoDB.AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" dist

install -m 755 "$BINARY" "$APPDIR/usr/bin/cryodb"

cp icons/icon.png "$APPDIR/cryodb.png"
cp "$APPDIR/cryodb.png" "$APPDIR/.DirIcon"
install -Dm644 icons/icon.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/cryodb.png"
install -Dm644 packaging/linux/cryodb.desktop "$APPDIR/cryodb.desktop"

cat > "$APPDIR/AppRun" << 'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/cryodb" "$@"
APPRUN
chmod +x "$APPDIR/AppRun"

install -d "$APPDIR/usr/share/metainfo"
cat > "$APPDIR/usr/share/metainfo/dev.znxr.cryodb.metainfo.xml" << METAINFO
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>dev.znxr.cryodb</id>
  <name>CryoDB</name>
  <summary>Multi-driver SQL client</summary>
  <developer id="dev.znxr"><name>Mauricio Orquin</name></developer>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>MIT</project_license>
  <launchable type="desktop-id">cryodb.desktop</launchable>
  <url type="homepage">https://cryodb.znxr.dev</url>
  <description>
    <p>
      CryoDB is a desktop SQL client for MySQL, MariaDB, PostgreSQL and
      SQLite, with a terminal UI and a scriptable CLI in the same binary.
    </p>
  </description>
  <releases>
    <release version="${VERSION}" date="$(date -u +%Y-%m-%d)"/>
  </releases>
</component>
METAINFO

if [ ! -x ./appimagetool ]; then
  curl -fsSL -o appimagetool \
    https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x appimagetool
fi

export APPIMAGE_EXTRACT_AND_RUN=1
ARCH=x86_64 ./appimagetool "$APPDIR" dist/cryodb-linux-x64.AppImage

echo "built:"
ls -la dist
