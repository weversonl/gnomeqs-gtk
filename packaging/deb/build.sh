#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/app/gtk/Cargo.toml" | head -n1)"
arch="$(dpkg --print-architecture)"
pkgroot="$root/packaging/out/deb/gnomeqs_${version}_${arch}"
artifact="$root/packaging/out/deb/gnomeqs_${version}_${arch}.deb"

rm -rf "$pkgroot" "$artifact"
mkdir -p "$pkgroot/DEBIAN"

cargo build --release -p gnomeqs -p gnomeqs-tray

install -Dm755 "$root/target/release/gnomeqs" "$pkgroot/usr/bin/gnomeqs"
install -Dm755 "$root/target/release/gnomeqs-tray" "$pkgroot/usr/bin/gnomeqs-tray"

install -Dm644 "$root/app/gtk/data/io.github.weversonl.GnomeQS.desktop" \
  "$pkgroot/usr/share/applications/io.github.weversonl.GnomeQS.desktop"
install -Dm644 "$root/app/gtk/data/io.github.weversonl.GnomeQS.metainfo.xml" \
  "$pkgroot/usr/share/metainfo/io.github.weversonl.GnomeQS.metainfo.xml"
install -Dm644 "$root/app/gtk/data/io.github.weversonl.GnomeQS.gschema.xml" \
  "$pkgroot/usr/share/glib-2.0/schemas/io.github.weversonl.GnomeQS.gschema.xml"

install -Dm644 "$root/app/gtk/data/icons/32x32.png" \
  "$pkgroot/usr/share/icons/hicolor/32x32/apps/io.github.weversonl.GnomeQS.png"
install -Dm644 "$root/app/gtk/data/icons/128x128.png" \
  "$pkgroot/usr/share/icons/hicolor/128x128/apps/io.github.weversonl.GnomeQS.png"
install -Dm644 "$root/app/gtk/data/icons/128x128@2x.png" \
  "$pkgroot/usr/share/icons/hicolor/256x256@2/apps/io.github.weversonl.GnomeQS.png"
install -Dm644 "$root/app/gtk/data/icons/tray_mono.png" \
  "$pkgroot/usr/share/icons/hicolor/32x32/apps/io.github.weversonl.GnomeQS-symbolic.png"
install -Dm644 "$root/app/gtk/data/icons/hicolor/scalable/actions/io.github.weversonl.GnomeQS-airdrop-symbolic.svg" \
  "$pkgroot/usr/share/icons/hicolor/scalable/actions/io.github.weversonl.GnomeQS-airdrop-symbolic.svg"
install -Dm644 "$root/app/gtk/data/icons/hicolor/scalable/status/io.github.weversonl.GnomeQS-tray-symbolic.svg" \
  "$pkgroot/usr/share/icons/hicolor/scalable/status/io.github.weversonl.GnomeQS-tray-symbolic.svg"

for lang in pt_BR; do
  install -dm755 "$pkgroot/usr/share/locale/$lang/LC_MESSAGES"
  msgfmt -o "$pkgroot/usr/share/locale/$lang/LC_MESSAGES/gnomeqs.mo" "$root/app/gtk/po/$lang.po"
done

cat > "$pkgroot/DEBIAN/control" <<EOF
Package: gnomeqs
Version: $version
Section: utils
Priority: optional
Architecture: $arch
Maintainer: weversonl
Depends: libgtk-4-1, libadwaita-1-0, libgtk-3-0, libayatana-appindicator3-1, libdbus-1-3, libglib2.0-bin
Description: GnomeQS - QuickShare client for GNOME
 GTK4 and Libadwaita desktop client for nearby file sharing.
EOF

install -Dm755 "$root/packaging/deb/postinst" "$pkgroot/DEBIAN/postinst"
install -Dm755 "$root/packaging/deb/postrm" "$pkgroot/DEBIAN/postrm"

dpkg-deb --build --root-owner-group "$pkgroot" "$artifact"
echo "Built $artifact"
