#!/usr/bin/env bash
#
# build tar ball for tagged releases
#

#shopt -s extglob

# ToDo
#  Check options
#

if [ "$1" == "-h" ] || [ "$1" == "--help" ]; then
  echo "$0 (clean|latest-tag|'any-string-version')"
  echo " 'clean' cargo clean and lock remove"
  echo " 'latest-tag' pull and switch too latest git tag"
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
  echo "Overriding Enviroment with $envFile file for settings ..."
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

if [ "$1" == "latest-tag" ]; then
  if [ "$gitclean" == "uncommitted" ]; then
    echo "Can't use latest-tag with uncommitted changes"
    echo  "Suggest commit and push before re-running $0";
    exit 5
  fi
  git fetch --all --tags
  gitTagVersion=$(git describe --tags `git rev-list --tags --max-count=1`)
  git checkout tags/$gitTagVersion -B $gitTagVersion-build
else
  gitTagVersion="${1}"
fi

gitBranch="$(git rev-parse --symbolic-full-name --abbrev-ref HEAD)"
gitCommitHash="$(git rev-parse --short HEAD)"

echo $gitTagVersion
echo $gitBranch
echo $gitCommitHash

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

echo $rustVer

shaSumVal="256"

#archiveBase="$distFullName-$rustVer-$gitTagVersion-$gitclean"
hashFile="$archiveBase.sha${shaSumVal}sum"
archiveFile="$archiveBase.zip"
echo $archiveBase
echo $hashFile
echo $archiveFile

distDir=$(mktemp -d)
if [ -d $distDir ]; then
  echo "Temporary directory $distDir exists"
else
  echo "Temporary directory $distDir does not exist"
  exit 6
fi

mkdir $distDir/dist

COPY_FILES=(
  "target/release/tari_base_node"
  "target/release/tari_merge_mining_proxy"
  "common/config/presets/tari-sample.toml"
  "common/config/tari_config_example.toml"
#  "log4rs.yml"
  "common/logging/log4rs-sample.yml"
  "applications/tari_base_node/README.md"
  applications/tari_base_node/$osname/*
  "scripts/install_tor.sh"
)

for COPY_FILE in "${COPY_FILES[@]}"; do
  cp "$COPY_FILE" "$distDir/dist/"
done

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
