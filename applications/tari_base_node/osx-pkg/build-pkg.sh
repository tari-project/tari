#!/bin/bash
#
# build OSX pkg and submit too Apple for signing and notarization
#

# ToDo
#  Check options

# Env
instName="tari_base_node"
sName=$(basename $0)
#sPath=$(realpath $0)
sPath=$(dirname $0)

if [ $# -lt 3 ];then
  echo "Usage: $0 {packageRoot} {packageVersion} {destDir}"
  echo "   ie: $0 /tmp/packageRoot 1.2.3.4 /tmp/destDir"
  exit 1
else
  pkgRoot="$1"
  pkgVersion="$2"
  destDir="$3"
fi

envFile="$sPath/.env"
if [ -f "$envFile" ]; then
  echo "Overriding Enviroment with $envFile file for settings ..."
  source "$envFile"
fi

# Some Error checking
if [ "$(uname)" == "Darwin" ]; then
  echo "Building OSX pkg ..."
else
  echo "Not OSX!"
  exit 2
fi

mkdir -p "$destDir/pkgRoot"
mkdir -p "$destDir/pkgRoot/usr/local/bin/"
# Verify signed?
codesign --verify --deep --display --verbose=4 \
  "$destDir/dist/$instName"
#spctl -a -v "$destDir/dist/$instName"
spctl -vvv --assess --type exec "$destDir/dist/$instName"

cp "$destDir/dist/$instName" "$destDir/pkgRoot/usr/local/bin/"

mkdir -p "$destDir/pkgRoot/usr/local/share/$instName"
COPY_SHARE_FILES=(
  *.sh
)
for COPY_SHARE_FILE in "${COPY_SHARE_FILES[@]}"; do
  cp "$destDir/dist/"$COPY_SHARE_FILE "$destDir/pkgRoot/usr/local/share/$instName/"
done

mkdir -p "$destDir/pkgRoot/usr/local/share/doc/$instName"
COPY_DOC_FILES=(
  "rincewind-simple.toml"
  "tari_config_sample.toml"
#  "log4rs.yml"
  "log4rs-sample.yml"
  "README.md"
)
for COPY_DOC_FILE in "${COPY_DOC_FILES[@]}"; do
  cp "$destDir/dist/"$COPY_DOC_FILE "$destDir/pkgRoot/usr/local/share/doc/$instName/"
done

pkgbuild --root $destDir/pkgRoot \
  --identifier "com.tarilabs.pkg.basenode" \
  --version "$pkgVersion" --install-location "/" \
  --sign "$osxSigDevIDInstaller" $destDir/$instName-$pkgVersion.pkg

echo "Submitting package, please wait ..."
RequestUUIDR=$(xcrun altool --notarize-app \
  --primary-bundle-id "com.tarilabs.com" \
  --username "$osxUsername" --password "$osxPassword" \
  --asc-provider "$osxASCProvider" \
  --file $destDir/$instName-$pkgVersion.pkg)

RequestUUID=${RequestUUIDR#RequestUUID\ =\ }
echo "Our request UUID is $RequestUUID ..."

RequestResult=$(xcrun altool --notarization-info "$RequestUUID" \
  --username "$osxUsername" --password "$osxPassword")

echo "Our $RequestResult ..."
