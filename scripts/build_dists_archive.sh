#!/usr/bin/env bash
#
# build zip for tagged releases
#

#shopt -s extglob

# ToDo
#  Check options
#

if [ "$1" = "-h" ] || [ "$1" = "--help" ]; then
  echo "$0 (latest-tag|latest-tagv|'any-string-version')"
  echo " 'latest-tag' pull and switch to latest git tag"
  echo " 'latest-tagv' pull and switch to latest git tag starting with 'v'"
  echo " 'any-string-version' archive string to tag with"
  echo "   ie: $0 nightly-development-test-\$(date +'%Y-%m-%d')"
  exit 1
fi

# Env
distName="tari_base_node"
sName=$(basename $0)
#sPath=$(realpath $0)
sPath=$(dirname $0)
tsstamp=$(date +'%Y%m%dT%Hh%M')

envFile="$sPath/.env"
if [ -f "$envFile" ]; then
  echo "Overriding Environment with $envFile file for settings ..."
  source "$envFile"
fi

if [ -f "Cargo.toml" ]; then
    echo "Cleaning Cargo ... "
    cargo clean
else
  echo "Can't find Cargo.toml, exiting"
  exit 2
fi

if [ "$(uname)" = "Darwin" ]; then
  osname="osx"
  # Retrive friendly name for mac
  osstring=$(awk '/SOFTWARE LICENSE AGREEMENT FOR macOS/' '/System/Library/CoreServices/Setup Assistant.app/Contents/Resources/en.lproj/OSXSoftwareLicense.rtf' | awk -F 'macOS ' '{print $NF}' | awk '{print substr($0, 0, length($0)-1)}')
  # Remove spaces if exist, e.g mojave, catalina, big_sur, etc
  osversion="${osstring// /_}"
  osarch="x64"
  #system_profiler SPSoftwareDataType
  #sw_vers
  osversion=$(sw_vers -productVersion | cut -d '.' -f 1,2)
  # Static link OpenSSL
  export OPENSSL_STATIC=1
elif [ "$(expr substr $(uname -s) 1 5)" = "Linux" ]; then
  # Make sure it is Ubuntu and not other Linux
  if [  -n "$(uname -a | grep Ubuntu)" ]; then
    osname="ubuntu"
    # Retrieve version
    osversion="$(uname -r)"
    osarch="x64"

    osname=$(echo $osname | tr '[:upper:]' '[:lower:]' )

    case $(uname -m) in
      x86_64)
        osarch="x64"  # or AMD64 or Intel64 or whatever
        ;;
      i*86)
        echo "Unsupported Architecture"
        exit 3
        ;;
      *)
        echo "Unsupported Architecture"
        exit 3
        ;;
    esac
  else
    echo "Unsupported OS"
    exit 3
  fi
else
  echo "Unsupported OS"
  exit 3
fi

distFullName="$distName-$osname-$osversion-$osarch"
echo $distFullName

if [ $(git rev-parse --is-inside-work-tree) ]; then
  echo "Git repo ..."
else
  echo "Not a git repo, this might not work!"
#  exit 3
fi

# Just a basic clean check
if [ -n "$(git status --porcelain)" ]; then
  echo "There are uncommited changes?"
  echo  "Suggest commit and push before re-running $0";
  gitclean="uncommitted"
#  exit 4
else
  echo "No changes";
  gitclean="gitclean"
fi

if [[ "$1" =~ ^latest* ]]; then
  if [ "$gitclean" == "uncommitted" ]; then
    echo "Can't use latest options with uncommitted changes"
    echo  "Suggest commit and push before re-running $0";
    exit 5
  fi
  git fetch --all --tags
  if [ "$1" == "latest-tag" ]; then
    gitTagVersion=$(git describe --tags `git rev-list --tags --max-count=1`)
  fi

  if [ "$1" == "latest-tagv" ]; then
    #gitTagVersion=$(git describe --tags --match "v[0-9]*" --abbrev=4 HEAD)
    #gitTagVersion=$(git describe --tags `git rev-list --tags=v[0-9].[0-9].[0-9]* --max-count=1`)
    # git match/tag does not do RegEx!
    gitTagVersion=$(git tag --list --sort=-version:refname "v*" | head -n 1)
  fi
  git checkout tags/$gitTagVersion -B $gitTagVersion-build
else
  gitTagVersion="${1}"
fi

gitBranch="$(git rev-parse --symbolic-full-name --abbrev-ref HEAD)"
gitCommitHash="$(git rev-parse --short HEAD)"

#if [ "git branch --list ${gitTagVersion-build}" ]; then
#  git checkout tags/$gitTagVersion
#else
#  git checkout tags/$gitTagVersion -b $gitTagVersion-build
#fi

#git checkout tags/$gitTagVersion -b $gitTagVersion-build
# As we will not push anything up using git, we don't need to track the branch?
#git branch --set-upstream-to=origin/$gitTagVersion $gitTagVersion-build
#git pull

# Build
cargo build --release

# ToDo: Might have multiple consts.rs files?
rustConsts=$(find target -name "consts.rs" | grep -i "tari_base_node")
if [ -f "$rustConsts" ];then
  rustVer=$(grep -i 'VERSION' "$rustConsts" | cut -d "\"" -f 2)
  archiveBase="$distFullName-$rustVer"
else
  rustVer="unversion"
  archiveBase="$distFullName-$rustVer-$gitTagVersion-$gitclean"
fi

echo "git Tag Version $gitTagVersion"
echo "git Branch $gitBranch"
echo "git Commit Hash $gitCommitHash"
echo "Rust Version $rustVer"
if [ "$gitTagVersion" == "$rustVer" ]; then
  echo "git Tag and rust version match"
else
  echo "Warning, git Tag Version does not match rust Version!"
  #exit 6
fi

shaSumVal="256"

#archiveBase="$distFullName-$rustVer-$gitTagVersion-$gitclean"
hashFile="$archiveBase.sha${shaSumVal}sum"
archiveFile="$archiveBase.zip"
echo "Archive Base $archiveBase"
echo "Hash file $hashFile"
echo "Archive file $archiveFile"

distDir=$(mktemp -d)
if [ -d ${distDir} ]; then
  echo "Temporary directory ${distDir} exists"
  mkdir -p ${distDir}/runtime
  mkdir -p ${distDir}/config
else
  echo "Temporary directory ${distDir} does not exist"
  exit 6
fi

# One click miner
cp -f -P "${PWD}/applications/tari_base_node/${osname}/start_all" "${distDir}/start_all"
cp -f "${PWD}/applications/tari_base_node/${osname}/runtime/start_all.sh" "${distDir}/runtime/start_all.sh"

# Base Node
cp -f -P "${PWD}/applications/tari_base_node/${osname}/start_tari_base_node" "${distDir}/start_tari_base_node"
cp -f -P "${PWD}/applications/tari_base_node/${osname}/start_tor" "${distDir}/start_tor"
cp -f "${PWD}/applications/tari_base_node/${osname}/runtime/start_tari_base_node.sh" "${distDir}/runtime/start_tari_base_node.sh"
cp -f "${PWD}/applications/tari_base_node/${osname}/runtime/start_tor.sh" "${distDir}/runtime/start_tor.sh"
cp -f "${PWD}/target/release/tari_base_node" "${distDir}/runtime/tari_base_node"

# Console Wallet
cp -f -P "${PWD}/applications/tari_console_wallet/${osname}/start_tari_console_wallet" "${distDir}/start_tari_console_wallet"
cp -f "${PWD}/applications/tari_console_wallet/${osname}/runtime/start_tari_console_wallet.sh" "${distDir}/runtime/start_tari_console_wallet.sh"
cp -f "${PWD}/target/release/tari_console_wallet" "${distDir}/runtime/tari_console_wallet"

# Mining Node
cp -f -P "${PWD}/applications/tari_mining_node/${osname}/start_tari_mining_node" "${distDir}/start_tari_mining_node"
cp -f "${PWD}/applications/tari_mining_node/${osname}/runtime/start_tari_mining_node.sh" "${distDir}/runtime/start_tari_mining_node.sh"
cp -f "${PWD}/target/release/tari_mining_node" "${distDir}/runtime/tari_mining_node"

# Merge Mining Proxy
cp -f -P "${PWD}/applications/tari_merge_mining_proxy/${osname}/start_tari_merge_mining_proxy" "${distDir}/start_tari_merge_mining_proxy"
cp -f -P "${PWD}/applications/tari_merge_mining_proxy/${osname}/start_xmrig" "${distDir}/start_xmrig"
cp -f "${PWD}/applications/tari_merge_mining_proxy/${osname}/runtime/start_tari_merge_mining_proxy.sh" "${distDir}/runtime/start_tari_merge_mining_proxy.sh"
cp -f "${PWD}/applications/tari_merge_mining_proxy/${osname}/runtime/start_xmrig.sh" "${distDir}/runtime/start_xmrig.sh"
cp -f "${PWD}/target/release/tari_merge_mining_proxy" "${distDir}/runtime/tari_merge_mining_proxy"

# 3rd party install
cp -f "${PWD}/buildtools/install_xmrig.sh" "${distDir}/runtime/install_xmrig.sh"

# Config
cp -f "${PWD}/common/config/presets/tari_sample.toml" "${distDir}/config/config.toml"
cp -f "${PWD}/common/xmrig_config/config_example_stagenet.json" "${distDir}/config/xmrig_config_example_stagenet.json"
cp -f "${PWD}/common/xmrig_config/config_example_mainnet.json" "${distDir}/config/xmrig_config_example_mainnet.json"
cp -f "${PWD}/common/xmrig_config/config_example_mainnet_self_select.json" "${distDir}/config/xmrig_config_example_mainnet_self_select.json"

oldPath="${PWD}"
cd ${distDir}
if [ "$osname" = "osx" ]  && [ -n "${osxsign}" ]; then
  echo "Setup OSX Binaries signing ..."
  for SIGN_FILE in $(find "${distDir}/runtime" -maxdepth 1 -name "tari_*" -type f -perm +111 ); do
    echo "Signing OSX Binary - $SIGN_FILE ..."
    codesign --options runtime --force --verify --verbose --sign "${osxsign}" "$SIGN_FILE"
    echo "Verify signed OSX Binary - $SIGN_FILE ..."
    codesign --verify --deep --display --verbose=4 "$SIGN_FILE"
    spctl -a -v "$SIGN_FILE"
  done
fi
shasum -a ${shaSumVal} * >> "${distDir}/${hashFile}"

echo "$(cat ${distDir}/${hashFile})" | shasum -a ${shaSumVal} --check

zip -r "${distDir}/${archiveFile}" *
echo ${distDir}/${archiveFile}
shasum -a ${shaSumVal} "${archiveFile}" >> "${distDir}/${archiveFile}.sha${shaSumVal}sum"

echo "$(cat ${distDir}/${archiveFile}.sha${shaSumVal}sum)" | shasum -a ${shaSumVal} --check
cd "${oldPath}"
echo "Delete $distDir"
