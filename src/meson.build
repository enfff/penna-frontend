pkgdatadir = get_option('prefix') / get_option('datadir') / meson.project_name()
gnome = import('gnome')

gnome.compile_resources('penna-frontend',
  'penna-frontend.gresource.xml',
  gresource_bundle: true,
  install: true,
  install_dir: pkgdatadir,
)

conf = configuration_data()
conf.set_quoted('VERSION', meson.project_version())
conf.set_quoted('GETTEXT_PACKAGE', 'penna-frontend')
conf.set_quoted('LOCALEDIR', get_option('prefix') / get_option('localedir'))
conf.set_quoted('PKGDATADIR', pkgdatadir)

configure_file(
    input: 'config.rs.in',
    output: 'config.rs',
    configuration: conf
)

# Copy the config.rs output to the source directory.
run_command(
  'cp',
  meson.project_build_root() / 'src' / 'config.rs',
  meson.project_source_root() / 'src' / 'config.rs',
  check: true
)

cargo_bin  = find_program('cargo')
cargo_opt  = [ '--manifest-path', meson.project_source_root() / 'Cargo.toml' ]
cargo_opt += [ '--target-dir', meson.project_build_root()  / 'src' ]
# Project-local CARGO_HOME: sandboxed builds (e.g. flatpak-builder) do not
# share ~/.cargo, so keep the registry and git-dep cache inside the project
# tree where the sandbox can see it (seed via `cargo fetch` with the same
# CARGO_HOME if an offline build is needed).
cargo_env  = [ 'CARGO_HOME=' + meson.project_source_root() / '.cargo',
             # Out-of-tree build dirs sit outside the repo, so
             # rust-toolchain.toml is not seen; pin explicitly.
             'RUSTUP_TOOLCHAIN=stable' ]

if get_option('buildtype') == 'release'
  cargo_opt += [ '--release' ]
  rust_target = 'release'
else
  rust_target = 'debug'
endif

cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  command: [
    'env', cargo_env,
    cargo_bin, 'build',
    cargo_opt, '&&', 'cp', 'src' / rust_target / meson.project_name(), '@OUTPUT@',
  ]
)

cargo_test_bin = find_program('cargo', required: true)
test('rust tests',
     cargo_test_bin,
     args: ['test'] + cargo_opt,
     env: cargo_env,
     timeout: 1800)
