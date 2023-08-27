#!/usr/bin/env bash
#
# Uninstall Minotaiji Base Node for OSX pkg
#

# Debugging enabled
#set -x

# ToDo:
#  Force/Check/Files/PKG
#

if [ ! "$(uname)" == "Darwin" ]; then
  echo "Uninstaller script meant for OSX"
  echo " Please visit https://taiji.com/downloads/"
  echo "  and download the binary distro for your platform"
  exit 1
fi

#osascript -e 'tell application \"Terminal\" to do script \"cd directory\" in \
#  selected tab of the front window' > /dev/null 2>&1

# Check pgk
pkgutil --pkgs=com.taijilabs.pkg.basenode*

pkgutil --files com.taijilabs.pkg.basenode
# rm -fr /usr/local/bin/minotaiji_node
# rm -fr /usr/local/share/minotaiji_node
# rm -fr /usr/local/share/doc/minotaiji_node

#taijiLabsReceipts=$(pkgutil --pkgs=com.taijilabs.pkg.basenode*)
#for myReceipt in $taijiLabsReceipts; do
#   pkgutil --forget $myReceipt
#done

sudo pkgutil --forget com.taijilabs.pkg.basenode
