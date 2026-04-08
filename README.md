# GnomeQS

GnomeQS is a GNOME-first Quick Share client for Linux.

It is a Rust-based port built specifically with GNOME in mind, using GTK4 and libadwaita for the main application and a separate tray helper for Linux environments that still rely on AppIndicator-style integration.

## Why This Project Exists

GnomeQS was created as a modern GNOME-oriented continuation of ideas that were already explored in **rQuickShare**.

That project was extremely important to the existence of this one and deserves explicit credit:

- https://henriqueclaranhan.github.io/rquickshare/

Many implementation and product decisions in GnomeQS were informed by the work behind rQuickShare.

## What GnomeQS Includes

- a GTK4 + libadwaita desktop application
- a Rust core library for discovery, transfers, and protocol handling
- a separate Linux tray helper
- local packaging for Flatpak, Arch, Debian, and RPM-based workflows

## Tech Stack

- Rust 2024 edition
- GTK4
- libadwaita
- Tokio
- gettext-rs
- Protocol Buffers via `prost`
- mDNS discovery via `mdns-sd`
- Bluetooth support via Linux-specific crates such as `bluer` and `btleplug`
- Ayatana AppIndicator for the tray helper

## Project Structure

- [`core_lib`](./core_lib): transfer logic, discovery, networking, Bluetooth, inbound/outbound flow
- [`app/gtk`](./app/gtk): GTK4/libadwaita application
- [`app/tray-helper`](./app/tray-helper): tray helper process
- [`aur`](./aur): Arch packaging files
- [`packaging/flatpak`](./packaging/flatpak): Flatpak manifest and helpers
- [`packaging/deb`](./packaging/deb): local Debian packaging helper
- [`packaging/rpm`](./packaging/rpm): local RPM packaging helper

## Platform Scope

GnomeQS is intentionally built for:

- Linux
- GNOME
- modern GTK/libadwaita environments

The primary target is a GNOME desktop running on Wayland, although X11 or non-GNOME setups may still work depending on the rest of the environment.

## Limitations

Before using or packaging the application, keep these limits in mind:

- Linux-only
- GNOME-first, not a generic cross-platform desktop app
- the tray implementation is Linux-specific
- tray behavior depends on desktop shell support and AppIndicator compatibility
- Flatpak builds intentionally ship without the tray helper
- desktop environments outside GNOME are not the primary support target

In short: this project is designed to feel right on GNOME first, and broad desktop portability is a secondary concern.

## Local Development Requirements

You need a Linux system with the Rust toolchain and GNOME-related development packages.

Typical requirements:

- Rust and Cargo
- GTK4 development files
- libadwaita development files
- GTK3 development files
- `glib2`
- `gettext`
- `libayatana-appindicator`

### Arch Linux Example

```bash
sudo pacman -S rust cargo gtk4 libadwaita gtk3 libayatana-appindicator glib2 gettext
```

## Running Locally

Run the app directly from the workspace:

```bash
cargo run -p gnomeqs
```

If an old instance is still alive and holding the port or tray helper:

```bash
pkill -f gnomeqs-tray
pkill -f target/debug/gnomeqs
```

Then start it again:

```bash
cargo run -p gnomeqs
```

## Building Local Binaries

Build release binaries locally:

```bash
cargo build --release -p gnomeqs
cargo build --release -p gnomeqs-tray
```

Generated binaries:

- `target/release/gnomeqs`
- `target/release/gnomeqs-tray`

For a clean rebuild:

```bash
cargo clean
cargo build --release -p gnomeqs
cargo build --release -p gnomeqs-tray
```

## Packaging

### Flatpak

Build the Flatpak bundle:

```bash
./packaging/flatpak/build.sh
```

Output:

- `packaging/out/flatpak/io.github.weversonl.GnomeQS.flatpak`

Install locally:

```bash
flatpak install --user packaging/out/flatpak/io.github.weversonl.GnomeQS.flatpak
```

Run:

```bash
flatpak run io.github.weversonl.GnomeQS
```

### Arch Linux / PKGBUILD

Build the package locally:

```bash
cd aur
makepkg -f
```

### Debian / Ubuntu

Build a local `.deb`:

```bash
./packaging/deb/build.sh
```

Output:

- `packaging/out/deb/*.deb`

Install locally:

```bash
sudo apt install ./packaging/out/deb/*.deb
```

If you prefer `dpkg`, you may need an extra dependency resolution step afterward:

```bash
sudo dpkg -i packaging/out/deb/*.deb
sudo apt-get install -f
```

Typical extra local tooling on Debian-based systems:

- `dpkg-dev`
- `gettext`
- Rust toolchain
- GTK4 development packages / libraries
- libadwaita development packages / libraries
- GTK3 development packages / libraries
- Ayatana AppIndicator development packages / libraries
- `glib2`

### Fedora / RHEL / Other RPM-Based Systems

Build a local RPM:

```bash
./packaging/rpm/build.sh
```

Output:

- `packaging/out/rpm/rpmbuild/RPMS/**/*.rpm`

Install locally with dependency resolution:

```bash
sudo dnf install packaging/out/rpm/rpmbuild/RPMS/*/*.rpm
```

Typical extra local tooling on RPM-based systems:

- `rpm-build`
- `gettext`
- Rust toolchain
- GTK4 development packages / libraries
- libadwaita development packages / libraries
- GTK3 development packages / libraries
- Ayatana AppIndicator development packages / libraries
- `glib2`

## Notes

- The main app and the tray helper are separate binaries by design.
- The tray helper exists because GTK4/libadwaita and Linux tray integration have different constraints.
- Flatpak intentionally excludes the tray helper.
- Packaging choices prioritize GNOME behavior and Linux desktop integration over broad platform reach.

## License

AGPL-3.0
