#!/usr/bin/env bash
set -euo pipefail

# Runs inside Ubuntu rolling container — needs libadwaita >= 1.6

export DEBIAN_FRONTEND=noninteractive

echo ">>> Installing build dependencies"
apt-get update -qq
apt-get install -y --no-install-recommends \
  curl wget file git \
  build-essential pkg-config \
  meson ninja-build \
  libgtk-4-dev libadwaita-1-dev \
  libgtk-3-dev libayatana-appindicator3-dev \
  libdbus-1-dev \
  gettext \
  libglib2.0-bin \
  python3-pip python3-setuptools \
  fuse libfuse2 \
  patchelf \
  desktop-file-utils \
  libgdk-pixbuf2.0-bin \
  libgtk-4-media-gstreamer \
  ca-certificates \
  zsync

echo ">>> Installing Rust"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
source "$HOME/.cargo/env"

echo ">>> Downloading linuxdeploy + GTK plugin"
wget -q "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage" \
  -O /usr/local/bin/linuxdeploy
chmod +x /usr/local/bin/linuxdeploy

wget -q "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh" \
  -O /usr/local/bin/linuxdeploy-plugin-gtk.sh
chmod +x /usr/local/bin/linuxdeploy-plugin-gtk.sh

wget -q "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage" \
  -O /usr/local/bin/appimagetool
chmod +x /usr/local/bin/appimagetool

SCRIPT_DIR="/src/packaging/appimage"
BUILD_DIR="$SCRIPT_DIR/build"
APPDIR="$SCRIPT_DIR/AppDir"

rm -rf "$BUILD_DIR" "$APPDIR"

echo ">>> Meson build"
meson setup "$BUILD_DIR" /src \
  --prefix=/usr \
  --buildtype=release \
  -Dtray=true

ninja -C "$BUILD_DIR"
DESTDIR="$APPDIR" ninja -C "$BUILD_DIR" install

echo ">>> Compiling GSettings schemas"
glib-compile-schemas "$APPDIR/usr/share/glib-2.0/schemas/"

echo ">>> Running linuxdeploy (deploy only)"
mkdir -p /usr/lib/x86_64-linux-gnu/gtk-4.0
export APPIMAGE_EXTRACT_AND_RUN=1
export DEPLOY_GTK_VERSION=4

linuxdeploy \
  --appdir "$APPDIR" \
  --plugin gtk \
  --desktop-file "$APPDIR/usr/share/applications/io.github.weversonl.GnomeQuickShare.desktop" \
  --icon-file "$APPDIR/usr/share/icons/hicolor/128x128/apps/io.github.weversonl.GnomeQuickShare.png"

echo ">>> Patching apprun-hook for libadwaita + locale"
HOOK="$APPDIR/apprun-hooks/linuxdeploy-plugin-gtk.sh"
# GTK_THEME forces GTK3 legacy stylesheet — breaks libadwaita completely
sed -i '/^export GTK_THEME=/d' "$HOOK"
sed -i '/^APPIMAGE_GTK_THEME=/d' "$HOOK"
# Allow Wayland (plugin hardcodes x11)
sed -i 's/^export GDK_BACKEND=x11/export GDK_BACKEND="${GDK_BACKEND:-wayland,x11}"/' "$HOOK"
# Point locale to bundled .mo files (LOCALE_DIR is baked in at compile time)
echo 'export GNOMEQS_LOCALE_DIR="$APPDIR/usr/share/locale"' >> "$HOOK"

echo ">>> Packaging with appimagetool"
cd "$SCRIPT_DIR"
APPIMAGE_EXTRACT_AND_RUN=1 appimagetool \
  --comp zstd \
  "$APPDIR" \
  "GnomeQS-${GNOMEQS_VERSION:-1.0.0}-x86_64.AppImage"

mkdir -p /src/dist
mv "$SCRIPT_DIR"/GnomeQS-*.AppImage* /src/dist/ 2>/dev/null || true

echo ">>> Done"
