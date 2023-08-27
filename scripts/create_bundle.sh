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
target/release/minotaiji_node
scripts/install_tor.sh
common/config/presets/taiji_config_example.toml
common/logging/log4rs_sample_base_node.yml
applications/minotaiji_node/osx/install.sh
applications/minotaiji_node/osx/osx_diag_report.sh
applications/minotaiji_node/osx/post_install.sh
applications/minotaiji_node/osx/start_tor.sh
applications/minotaiji_node/osx/uninstall_pkg.sh
applications/minotaiji_node/README.md
'

# Create a zip file, stripping out paths (-j)
zip -j - $BUNDLE > $OUTFILE
