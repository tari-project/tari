#!/usr/bin/env bash
#
# Uninastall Tari Base Node for OSX
#

# ToDo:
#  Force/Check/Files/PKG
#

if [ ! "$(uname)" == "Darwin" ]; then
  echo "Uninstaller script meant for OSX"
  echo " Please visit https://tari.com/downloads/"
  echo "  and download the binary distro for your platform"
  exit 1
fi

#osascript -e 'tell application \"Terminal\" to do script \"cd directory\" in \
#  selected tab of the front window' > /dev/null 2>&1

# Check pgk
pkgutil --pkgs=com.tarilabs.pkg.basenode*

pkgutil --files com.tarilabs.pkg.basenode
# rm -fr /usr/local/bin/tari_base_node
# rm -fr /usr/local/share/tari_base_node
# rm -fr /usr/local/share/doc/tari_base_node

#tariLabsReceipts=$(pkgutil --pkgs=com.tarilabs.pkg.basenode*)
#for myReceipt in $tariLabsReceipts; do
#   pkgutil --forget $myReceipt
#done

#pkgutil --forget com.tarilabs.pkg.basenode
