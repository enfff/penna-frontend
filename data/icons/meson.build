application_id = 'io.github.enfff.Diary'

scalable_dir = 'hicolor' / 'scalable' / 'apps'
if get_option('devel')
  install_data(
    'app' / ('@0@.Devel.svg').format(application_id),
    install_dir: get_option('datadir') / 'icons' / scalable_dir,
    rename: ['@0@.svg'.format(application_id)]
  )
else
  install_data(
    'app' / ('@0@.svg').format(application_id),
    install_dir: get_option('datadir') / 'icons' / scalable_dir
  )
endif

symbolic_dir = 'hicolor' / 'symbolic' / 'apps'
install_data(
  symbolic_dir / ('@0@-symbolic.svg').format(application_id),
  install_dir: get_option('datadir') / 'icons' / symbolic_dir
)
