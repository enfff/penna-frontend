application_id = 'io.github.enfff.Diary'

scalable_dir = 'hicolor' / 'scalable' / 'apps'
install_data(
  scalable_dir / ('@0@.svg').format(application_id),
  install_dir: get_option('datadir') / 'icons' / scalable_dir
)

symbolic_dir = 'hicolor' / 'symbolic' / 'apps'
install_data(
  symbolic_dir / ('@0@-symbolic.svg').format(application_id),
  install_dir: get_option('datadir') / 'icons' / symbolic_dir
)

action_symbolic_dir = 'hicolor' / 'symbolic' / 'actions'
install_data(
  action_symbolic_dir / 'cloud-deploy-symbolic.svg',
  install_dir: get_option('datadir') / 'icons' / action_symbolic_dir
)
