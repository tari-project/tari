#!/usr/bin/env bash
#
# build tar ball for tagged releases
#

#shopt -s extglob

# ToDo
#  Check options
#

if [ "$1" == "-h" ] || [ "$1" == "--help" ]; then
  echo "$0 (clean|latest-tag|latest-tagv|'any-string-version')"
  echo " 'clean' cargo clean and lock remove"
  echo " 'latest-tag' pull and switch to latest git tag"
  echo " 'latest-tagv' pull and switch to latest git tag starting with 'v'"
  echo " 'any-string-version' archive string to tag with"
  echo "   ie: $0 nightly-development-test-\$(date +'%Y-%m-%d')"
  exit 1
fi

# Env
distName="minotari_node"
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
  if [ "$1" == "clean" ]; then
    shift
    echo "Cleaning Cargo ... "
    cargo clean
  fi
else
  echo "Can't find Cargo.toml, exiting"
  exit 2
fi

if [ "$(uname)" == "Darwin" ]; then
  osname="osx"
  osversion="catalina"
  osarch="x64"
  #system_profiler SPSoftwareDataType
  #sw_vers
  osversion=$(sw_vers -productVersion | cut -d '.' -f 1,2)

elif [ "$(expr substr $(uname -s) 1 5)" == "Linux" ]; then
  osname="ubuntu"
  osversion="18.04"
  osarch="x64"

  if [ -f /etc/os-release ]; then
    # freedesktop.org and systemd
    . /etc/os-release
    osname=$NAME
    osversion=$VERSION_ID
  elif type lsb_release >/dev/null 2>&1; then
    # linuxbase.org
    osname=$(lsb_release -si)
    osversion=$(lsb_release -sr)
  elif [ -f /etc/lsb-release ]; then
    # For some versions of Debian/Ubuntu without lsb_release command
    . /etc/lsb-release
    osname=$DISTRIB_ID
    osversion=$DISTRIB_RELEASE
  elif [ -f /etc/debian_version ]; then
    # Older Debian/Ubuntu/etc.
    osname=Debian
    osversion=$(cat /etc/debian_version)
  elif [ -f /etc/SuSe-release ]; then
    # Older SuSE/etc.
    echo "Suse?"
  elif [ -f /etc/redhat-release ]; then
    # Older Red Hat, CentOS, etc.
    echo "RedHat?"
  else
    # Fall back to uname, e.g. "Linux <version>", also works for BSD, etc.
    osname=$(uname -s)
    osversion=$(uname -r)
  fi

  osname=$(echo $osname | tr '[:upper:]' '[:lower:]' )

  case $(uname -m) in
    x86_64)
      osarch="x64"  # or AMD64 or Intel64 or whatever
      ;;
    i*86)
      osarch="x86"  # or IA32 or Intel32 or whatever
      ;;
    *)
      # leave ARCH as-is
      ;;
  esac

elif [ "$(expr substr $(uname -s) 1 10)" == "MINGW32_NT" ]; then
  # Do something under 32 bits Windows NT platform
  echo "32 bits Windows NT platform"
elif [ "$(expr substr $(uname -s) 1 10)" == "MINGW64_NT" ]; then
  # Do something under 64 bits Windows NT platform
  echo "64 bits Windows NT platform"
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
rustConsts=$(find target -name "consts.rs" | grep -i "minotari_node")
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
if [ -d $distDir ]; then
  echo "Temporary directory $distDir exists"
else
  echo "Temporary directory $distDir does not exist"
  exit 6
fi

mkdir $distDir/dist

COPY_FILES=(
  "target/release/minotari_node"
  "target/release/minotari_console_wallet"
  "target/release/minotari_merge_mining_proxy"
  "target/release/minotari_miner"
  "common/config/presets/*.toml"
  "common/logging/log4rs_sample_base_node.yml"
  "applications/minotari_node/README.md"
  applications/minotari_node/$osname/*
  "scripts/install_tor.sh"
)

for COPY_FILE in "${COPY_FILES[@]}"; do
  cp -vr "$COPY_FILE" "$distDir/dist/"
done

cat common/config/presets/*.toml >"$distDir/dist/tari_config_example.toml"

pushd $distDir/dist
if [ "$osname" == "osx" ]  && [ -n "${osxsign}" ]; then
  echo "Setup OSX Binaries signing ..."
  for SIGN_FILE in $(find "${distDir}/dist" -maxdepth 1 -name "tari_*" -type f -perm +111 ); do
    echo "Signing OSX Binary - $SIGN_FILE ..."
    codesign --options runtime --force --verify --verbose --sign "${osxsign}" "$SIGN_FILE"
    echo "Verify signed OSX Binary - $SIGN_FILE ..."
    codesign --verify --deep --display --verbose=4 "$SIGN_FILE"
    spctl -a -v "$SIGN_FILE"
  done
fi
shasum -a $shaSumVal * >> "$distDir/$hashFile"
#echo "$(cat $distDir/$hashFile)" | shasum -a $shaSumVal --check --status
echo "$(cat $distDir/$hashFile)" | shasum -a $shaSumVal --check
mv "$distDir/$hashFile" "$distDir/dist/"
#tar -cjpf "$distDir/$archiveFile" .
zip -j "$distDir/$archiveFile" *
cd ..
shasum -a $shaSumVal "$archiveFile" >> "$distDir/$archiveFile.sha${shaSumVal}sum"
#echo "$(cat $distDir/$archiveFile.sha${shaSumVal}sum) $distDir/$archiveFile" | shasum -a $shaSumVal --check
echo "$(cat $distDir/$archiveFile.sha${shaSumVal}sum)" | shasum -a $shaSumVal --check
popd
echo "Delete $distDir"
#rm -fr $distDir
