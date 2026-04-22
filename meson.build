project(
  'gnome-quick-share',
  version: '1.4.0',
  license: 'AGPL-3.0-only',
  meson_version: '>= 1.0',
)

app_id = 'io.github.weversonl.GnomeQuickShare'

i18n = import('i18n')
gnome = import('gnome')

# Data assets (schemas, desktop entry, metainfo, icons)
subdir('app/gtk/data')

# Translations (.po → .mo)
subdir('app/gtk/po')

# ── Rust / Cargo ──────────────────────────────────────────────────────────────

cargo = find_program('cargo', required: true)
sh    = find_program('sh',    required: true)

cargo_build_script = files('build-aux/cargo-build.sh')

profile = get_option('buildtype')
if profile == 'release' or profile == 'plain'
  cargo_profile = 'release'
else
  cargo_profile = 'debug'
endif

cargo_target_dir = meson.project_build_root() / 'target'

cargo_env = environment()
cargo_env.set('CARGO_TARGET_DIR', cargo_target_dir)
cargo_env.set('GNOMEQS_LOCALE_DIR',
  get_option('prefix') / get_option('localedir'))

if get_option('tray')
  # Single custom_target that builds both binaries in one cargo invocation.
  cargo_bins = custom_target(
    'cargo-build-all',
    build_by_default: true,
    build_always_stale: true,
    output: ['gnomeqs', 'gnomeqs-tray'],
    console: true,
    install: true,
    install_dir: [get_option('bindir'), get_option('bindir')],
    install_mode: ['rwxr-xr-x', 'rwxr-xr-x'],
    env: cargo_env,
    command: [
      sh, cargo_build_script,
      cargo.full_path(),
      meson.project_source_root(),
      cargo_profile,
      '@OUTPUT0@',
      '@OUTPUT1@',
    ],
  )
else
  cargo_bins = custom_target(
    'cargo-build',
    build_by_default: true,
    build_always_stale: true,
    output: ['gnomeqs'],
    console: true,
    install: true,
    install_dir: get_option('bindir'),
    install_mode: 'rwxr-xr-x',
    env: cargo_env,
    command: [
      sh, cargo_build_script,
      cargo.full_path(),
      meson.project_source_root(),
      cargo_profile,
      '@OUTPUT0@',
    ],
  )
endif

# Post-install hooks (skipped automatically when DESTDIR is set)
gnome.post_install(
  glib_compile_schemas: true,
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)

# ── Uninstall ─────────────────────────────────────────────────────────────────

run_target(
  'full-uninstall',
  command: [
    files('build-aux/uninstall.sh'),
    meson.project_build_root(),
    get_option('prefix')
  ]
)
