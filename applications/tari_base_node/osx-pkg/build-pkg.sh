#!/bin/bash
#
# build OSX pkg and submit too Apple for signing and notarization
#

# ToDo
#  Check options
#
# ./$0 packageRoot packageVersion
#  ie: ./$0 /tmp/packageRoot 1.2.3.4

# Env
instName="tari_base_node"
sName=$(basename $0)
#sPath=$(realpath $0)
sPath=$(dirname $0)

envFile="$sPath/.env"
if [ -f "$envFile" ]; then
  echo "Overriding Enviroment with $envFile file for settings ..."
  source "$envFile"
fi

pkgRoot="$1"
pkgVersion="$2"

# Some Error checking
if [ "$(uname)" == "Darwin" ]; then
  echo "Building OSX pkg ..."
else
  echo "Not OSX!"
  exit 1
fi

pkgbuild --root "$pkgRoot" \
  --identifier "com.tarilabs.pkg.basenode" \
  --version "$pkgVersion" \
  --install-location "/" \
  --sign "$osxSigDevIDInstaller" \
   $instName-$pkgVersion.pkg

xcrun altool --notarize-app \
  --primary-bundle-id "com.tarilabs.com" \
  --username "$osxUsername" \
  --password "$osxPassword" \
  --asc-provider "$osxASCProvider" \
  --file "$instName-$pkgVersion.pkg"
