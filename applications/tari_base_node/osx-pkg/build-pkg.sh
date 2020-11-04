#!/bin/bash
#
# build OSX pkg and submit too Apple for signing and notarization
#

# Debugging enabled
#set -x

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

COPY_BIN_FILES=(
  "tari_base_node"
  "tari_merge_mining_proxy"
)
for COPY_BIN_FILE in "${COPY_BIN_FILES[@]}"; do
  # Verify signed?
  codesign --verify --deep --display --verbose=4 "$destDir/dist/$COPY_BIN_FILE"
  spctl -vvv --assess --type exec "$destDir/dist/$COPY_BIN_FILE"

  cp "$destDir/dist/$COPY_BIN_FILE" "$destDir/pkgRoot/usr/local/bin/"
done

mkdir -p "$destDir/pkgRoot/usr/local/share/$instName"
COPY_SHARE_FILES=(
  *.sh
)
for COPY_SHARE_FILE in "${COPY_SHARE_FILES[@]}"; do
  cp "$destDir/dist/"$COPY_SHARE_FILE "$destDir/pkgRoot/usr/local/share/$instName/"
done

mkdir -p "$destDir/pkgRoot/usr/local/share/doc/$instName"
COPY_DOC_FILES=(
  "tari-sample.toml"
  "tari_config_example.toml"
  "log4rs-sample-base-node.yml"
  "README.md"
)
for COPY_DOC_FILE in "${COPY_DOC_FILES[@]}"; do
  cp "$destDir/dist/"$COPY_DOC_FILE "$destDir/pkgRoot/usr/local/share/doc/$instName/"
done

mkdir -p "$destDir/scripts"
cp -r "${sPath}/scripts/"* "$destDir/scripts/"

pkgbuildResult=$(pkgbuild --root $destDir/pkgRoot \
  --identifier "com.tarilabs.pkg.basenode" \
  --version "$pkgVersion" --install-location "/" \
  --scripts "$destDir/scripts" \
  --sign "$osxSigDevIDInstaller" $destDir/$instName-$pkgVersion.pkg)

echo $pkgbuildResult

echo "Submitting package, please wait ..."
RequestUUIDR=$(xcrun altool --notarize-app \
  --primary-bundle-id "com.tarilabs.pkg" \
  --username "$osxUsername" --password "$osxPassword" \
  --asc-provider "$osxASCProvider" \
  --file $destDir/$instName-$pkgVersion.pkg)

requestStatus=$?
if [ $requestStatus -eq 0 ]; then
  RequestLen=${#RequestUUIDR}
  echo "Let length of ... $RequestLen ..."
  echo "|$RequestUUIDR|"
  RequestUUID=${RequestUUIDR#*RequestUUID = }
  echo "Our request UUID is $RequestUUID ..."
  echo "|$RequestUUID|"
else
  echo "Error submitting ..."
  echo $RequestUUIDR
  exit 1
fi

shaSumVal="256"
shasum -a $shaSumVal $destDir/$instName-$pkgVersion.pkg >> "$destDir/$instName-$pkgVersion.pkg.sha${shaSumVal}sum"
echo "$(cat $destDir/$instName-$pkgVersion.pkg.sha${shaSumVal}sum)" | shasum -a $shaSumVal --check

RequestResult=$(xcrun altool --notarization-info "$RequestUUID" \
  --username "$osxUsername" --password "$osxPassword")

echo "Our $RequestResult ..."
