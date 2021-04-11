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
#sPath=$(realpath $0)
sPath=$(dirname "$0")

if [ $# -lt 3 ];then
  echo "Usage: $0 {packageVersion} {destDir}"
  echo "   ie: $0 1.2.3.4 /tmp/destDir"
  exit 1
else
  pkgVersion="$1"
  destDir="$2"
fi

envFile="${sPath}/.env"
if [ -f "$envFile" ]; then
  echo "Overriding Enviroment with $envFile file for settings ..."
  # shellcheck disable=SC1090
  source "${envFile}"
fi

# Some Error checking
if [ "$(uname)" == "Darwin" ]; then
  echo "Building OSX pkg ..."
else
  echo "Not OSX!"
  exit 2
fi

mkdir -p "${destDir}/pkgRoot"
mkdir -p "${destDir}/pkgRoot/usr/local/tari/runtime"
mkdir -p "${destDir}/pkgRoot/usr/local/tari/config"

VERIFY_BIN_FILES=(
  "tari_base_node"
  "tari_console_wallet"
  "tari_merge_mining_proxy"
  "tari_mining_node"
)
for VERIFY_BIN_FILE in "${VERIFY_BIN_FILES[@]}"; do
  # Verify signed?
  codesign --verify --deep --display --verbose=4 "${destDir}/runtime/$VERIFY_BIN_FILE"
  spctl -vvv --assess --type exec "${destDir}/runtime/$VERIFY_BIN_FILE"
  # Should probably exit here if either fails at any point
done

# One click miner
cp "${destDir}/start_all" "${destDir}/pkgRoot/usr/local/tari/"
cp "${destDir}/runtime/start_all.sh" "${destDir}/pkgRoot/usr/local/tari/runtime/"

# Base Node
cp "${destDir}/start_tari_base_node" "${destDir}/pkgRoot/usr/local/tari/"
cp "${destDir}/start_tor" "${destDir}/pkgRoot/usr/local/tari/"
cp "${destDir}/runtime/start_tari_base_node.sh" "${destDir}/pkgRoot/usr/local/tari/runtime/"
cp "${destDir}/runtime/start_tor.sh" "${destDir}/pkgRoot/usr/local/tari/runtime/"
cp "${destDir}/runtime/tari_base_node" "${destDir}/pkgRoot/usr/local/tari/runtime/"

# Console Wallet
cp "${destDir}/start_tari_console_wallet" "${destDir}/pkgRoot/usr/local/tari/"
cp "${destDir}/runtime/start_tari_console_wallet.sh" "${destDir}/pkgRoot/usr/local/tari/runtime/"
cp "${destDir}/runtime/tari_console_wallet" "${destDir}/pkgRoot/usr/local/tari/runtime/"

# Mining Node
cp "${destDir}/start_tari_mining_node" "${destDir}/pkgRoot/usr/local/tari/"
cp "${destDir}/runtime/start_tari_mining_node.sh" "${destDir}/pkgRoot/usr/local/tari/runtime/"
cp "${destDir}/runtime/tari_mining_node" "${destDir}/pkgRoot/usr/local/tari/runtime/"

# Merge Mining Proxy
cp "${destDir}/start_tari_merge_mining_proxy" "${destDir}/pkgRoot/usr/local/tari/"
cp "${destDir}/start_xmrig" "${destDir}/pkgRoot/usr/local/tari/"
cp "${destDir}/runtime/start_tari_merge_mining_proxy.sh" "${destDir}/pkgRoot/usr/local/tari/runtime/"
cp "${destDir}/runtime/start_xmrig.sh" "${destDir}/pkgRoot/usr/local/tari/runtime/"
cp "${destDir}/runtime/tari_merge_mining_proxy" "${destDir}/pkgRoot/usr/local/tari/runtime/"

# 3rd party install
cp "${destDir}/runtime/install_xmrig.sh" "${destDir}/pkgRoot/usr/local/tari/runtime/"

# Config
cp "${destDir}/config/config.toml" "${destDir}/pkgRoot/usr/local/tari/config/"
cp "${destDir}/config/xmrig_config_example_stagenet.json" "${destDir}/pkgRoot/usr/local/tari/config/"
cp "${destDir}/config/xmrig_config_example_mainnet.json" "${destDir}/pkgRoot/usr/local/tari/config/"
cp "${destDir}/config/xmrig_config_example_mainnet_self_select.json" "${destDir}/pkgRoot/usr/local/tari/config/"

#ReadMe
cp "${destDir}/README.md" "${destDir}/pkgRoot/usr/local/tari/README.md"

#Post-install Script
cp "${PWD}/applications/tari_base_node/osx/post_install.sh" "${destDir}/post_install.sh"

#Un-installer
cp "${PWD}/applications/tari_base_node/osx/uninstall_pkg.sh" "${destDir}/uninstall_pkg.sh"

# Pkg Scripts
mkdir -p "${destDir}/scripts"
cp "${PWD}/applications/tari_base_node/osx-pkg/scripts/preinstall" "${destDir}/scripts/preinstall"
cp "${PWD}/applications/tari_base_node/osx-pkg/scripts/postinstall" "${destDir}/scripts/postinstall"

# shellcheck disable=SC2154
pkgbuildResult=$(pkgbuild --root "${destDir}/pkgRoot" \
  --identifier "com.tarilabs.pkg.basenode" \
  --version "$pkgVersion" --install-location "/" \
  --scripts "${destDir}/scripts" \
  --sign "$osxSigDevIDInstaller" "${destDir}/${instName}-${pkgVersion}.pkg")

echo "${pkgbuildResult}"

echo "Submitting package, please wait ..."
# shellcheck disable=SC2154
RequestUUIDR=$(xcrun altool --notarize-app \
  --primary-bundle-id "com.tarilabs.pkg" \
  --username "$osxUsername" --password "$osxPassword" \
  --asc-provider "$osxASCProvider" \
  --file "${destDir}/${instName}-${pkgVersion}.pkg")

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
  echo "${RequestUUIDR}"
  exit 1
fi

shaSumVal="256"
shasum -a $shaSumVal "${destDir}/${instName}-${pkgVersion}.pkg" >> "${destDir}/${instName}-$pkgVersion.pkg.sha${shaSumVal}sum"
# shellcheck disable=SC2005
echo "$(cat "${destDir}"/${instName}-"${pkgVersion}".pkg.sha${shaSumVal}sum)" | shasum -a $shaSumVal --check

RequestResult=$(xcrun altool --notarization-info "$RequestUUID" \
  --username "$osxUsername" --password "$osxPassword")

echo "Our $RequestResult ..."
