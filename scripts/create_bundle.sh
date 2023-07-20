#!/bin/bash

# This script creates a zip bundle for distribution
# Right now, it only builds an OsX bundle

print_usage () {
  echo "Usage:
  create_bundle [filename]
"
}

OUTFILE=$1

if [ "$OUTFILE" = "" ]; then
  print_usage
  exit
fi

BUNDLE='
target/release/minotari_base_node
scripts/install_tor.sh
common/config/presets/tari_config_example.toml
common/logging/log4rs_sample_base_node.yml
applications/minotari_base_node/osx/install.sh
applications/minotari_base_node/osx/osx_diag_report.sh
applications/minotari_base_node/osx/post_install.sh
applications/minotari_base_node/osx/start_tor.sh
applications/minotari_base_node/osx/uninstall_pkg.sh
applications/minotari_base_node/README.md
'

# Create a zip file, stripping out paths (-j)
zip -j - $BUNDLE > $OUTFILE
