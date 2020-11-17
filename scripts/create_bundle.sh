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
target/release/tari_base_node
scripts/install_tor.sh
common/config/presets/tari-sample.toml
common/logging/log4rs-sample-base-node.yml
applications/tari_base_node/install-osx.sh
applications/tari_base_node/start_tor.sh
applications/tari_base_node/README.md
'

# Create a zip file, stripping out paths (-j)
zip -j - $BUNDLE > $OUTFILE
