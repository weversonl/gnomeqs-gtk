Name:           gnome-quick-share
Version:        %{?version_override}%{!?version_override:1.1.0}
Release:        1%{?dist}
Summary:        GNOME Quick Share client
License:        AGPL-3.0-only
URL:            https://github.com/weversonl/gnome-quick-share
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gettext
BuildRequires:  glib2
BuildRequires:  gtk4-devel
BuildRequires:  libadwaita-devel
BuildRequires:  gtk3-devel
BuildRequires:  libayatana-appindicator-devel
BuildRequires:  dbus-devel

Requires:       gtk4
Requires:       libadwaita
Requires:       gtk3
Requires:       libayatana-appindicator
Requires:       dbus
%description
GnomeQS is a GTK4 and Libadwaita desktop client for nearby file sharing.

%prep
%autosetup

%build
cargo build --release -p gnomeqs -p gnomeqs-tray

%install
install -Dm755 target/release/gnomeqs %{buildroot}%{_bindir}/gnomeqs
install -Dm755 target/release/gnomeqs-tray %{buildroot}%{_bindir}/gnomeqs-tray

install -Dm644 app/gtk/data/io.github.weversonl.GnomeQuickShare.desktop \
  %{buildroot}%{_datadir}/applications/io.github.weversonl.GnomeQuickShare.desktop
install -Dm644 app/gtk/data/io.github.weversonl.GnomeQuickShare.metainfo.xml \
  %{buildroot}%{_datadir}/metainfo/io.github.weversonl.GnomeQuickShare.metainfo.xml
install -Dm644 app/gtk/data/io.github.weversonl.GnomeQuickShare.gschema.xml \
  %{buildroot}%{_datadir}/glib-2.0/schemas/io.github.weversonl.GnomeQuickShare.gschema.xml

install -Dm644 app/gtk/data/icons/32x32.png \
  %{buildroot}%{_datadir}/icons/hicolor/32x32/apps/io.github.weversonl.GnomeQuickShare.png
install -Dm644 app/gtk/data/icons/128x128.png \
  %{buildroot}%{_datadir}/icons/hicolor/128x128/apps/io.github.weversonl.GnomeQuickShare.png
install -Dm644 app/gtk/data/icons/128x128@2x.png \
  %{buildroot}%{_datadir}/icons/hicolor/256x256@2/apps/io.github.weversonl.GnomeQuickShare.png
install -Dm644 app/gtk/data/icons/tray_mono.png \
  %{buildroot}%{_datadir}/icons/hicolor/32x32/apps/io.github.weversonl.GnomeQuickShare-symbolic.png
install -Dm644 app/gtk/data/icons/hicolor/scalable/actions/io.github.weversonl.GnomeQuickShare-airdrop-symbolic.svg \
  %{buildroot}%{_datadir}/icons/hicolor/scalable/actions/io.github.weversonl.GnomeQuickShare-airdrop-symbolic.svg
install -Dm644 app/gtk/data/icons/hicolor/scalable/status/io.github.weversonl.GnomeQuickShare-tray-symbolic.svg \
  %{buildroot}%{_datadir}/icons/hicolor/scalable/status/io.github.weversonl.GnomeQuickShare-tray-symbolic.svg

for lang in pt_BR; do
  install -dm755 %{buildroot}%{_datadir}/locale/${lang}/LC_MESSAGES
  msgfmt -o %{buildroot}%{_datadir}/locale/${lang}/LC_MESSAGES/gnomeqs.mo app/gtk/po/${lang}.po
done

%post
/usr/bin/glib-compile-schemas %{_datadir}/glib-2.0/schemas >/dev/null 2>&1 || :
/usr/bin/gtk-update-icon-cache -q %{_datadir}/icons/hicolor >/dev/null 2>&1 || :

%postun
/usr/bin/glib-compile-schemas %{_datadir}/glib-2.0/schemas >/dev/null 2>&1 || :
/usr/bin/gtk-update-icon-cache -q %{_datadir}/icons/hicolor >/dev/null 2>&1 || :

%files
%license
%{_bindir}/gnomeqs
%{_bindir}/gnomeqs-tray
%{_datadir}/applications/io.github.weversonl.GnomeQuickShare.desktop
%{_datadir}/metainfo/io.github.weversonl.GnomeQuickShare.metainfo.xml
%{_datadir}/glib-2.0/schemas/io.github.weversonl.GnomeQuickShare.gschema.xml
%{_datadir}/icons/hicolor/32x32/apps/io.github.weversonl.GnomeQuickShare.png
%{_datadir}/icons/hicolor/128x128/apps/io.github.weversonl.GnomeQuickShare.png
%{_datadir}/icons/hicolor/256x256@2/apps/io.github.weversonl.GnomeQuickShare.png
%{_datadir}/icons/hicolor/32x32/apps/io.github.weversonl.GnomeQuickShare-symbolic.png
%{_datadir}/icons/hicolor/scalable/actions/io.github.weversonl.GnomeQuickShare-airdrop-symbolic.svg
%{_datadir}/icons/hicolor/scalable/status/io.github.weversonl.GnomeQuickShare-tray-symbolic.svg
%{_datadir}/locale/pt_BR/LC_MESSAGES/gnomeqs.mo
