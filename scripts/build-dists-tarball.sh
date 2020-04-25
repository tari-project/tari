#!/bin/bash
#
# build tar ball for tagged releases
#

#shopt -s extglob

# ToDo
#  Check options
#
# ./$0 (version|latest-tag)
#  ie: ./$0 nightly-development-test-$(date +'%Y-%m-%d')

# Env
distName="tari_base_node"
sName=$(basename $0)
#sPath=$(realpath $0)
sPath=$(dirname $0)

envFile="$sPath/.env"
if [ -f "$envFile" ]; then
  echo "Overriding Enviroment with $envFile file for settings ..."
  source "$envFile"
fi

if [ -f "Cargo.toml" ]; then
  cargo build --release
else
  echo "Can't find Cargo.toml, exiting"
  exit 1
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

if [ -f "applications/tari_base_node/src/consts.rs" ];then
  rustver=$(grep -i 'VERSION' applications/tari_base_node/src/consts.rs | cut -d "\"" -f 2)
else
  rustver="unversion"
fi

# Just a basic clean check
if [ -n "$(git status --porcelain)" ]; then
  echo "There are changes, please clean up before re-running $0";
  gitclean="uncommited"
#  exit 2
else
  echo "No changes";
  gitclean="gitclean"
fi

git fetch --all --tags

if [ "$1" == "latest-tag" ]; then
  gitTagVersion=$(git describe --tags `git rev-list --tags --max-count=1`)
  git checkout tags/$gitTagVersion -B $gitTagVersion-build
else
  gitTagVersion="${1}"
fi

echo $gitTagVersion

git rev-parse --symbolic-full-name --abbrev-ref HEAD

#if [ "git branch --list ${gitTagVersion-build}" ]; then
#  git checkout tags/$gitTagVersion
#else
#  git checkout tags/$gitTagVersion -b $gitTagVersion-build
#fi

#git checkout tags/$gitTagVersion -b $gitTagVersion-build
# As we will not push anything up using git, we don't need to track the branch?
#git branch --set-upstream-to=origin/$gitTagVersion $gitTagVersion-build
#git pull

shaSumVal="256"

hashFile="$distFullName-$rustver-$gitTagVersion-$gitclean.sha${shaSumVal}sum"
archiveFile="$distFullName-$rustver-$gitTagVersion-$gitclean.zip"
echo $hashFile

distDir=$(mktemp -d)
if [ -d $distDir ]; then
  echo "Temporary directory $distDir exists"
else
  echo "Temporary directory $distDir does not exist"
  exit 3
fi

mkdir $distDir/dist

COPY_FILES=(
  "target/release/tari_base_node"
  "common/config/presets/rincewind-simple.toml"
  "common/config/tari_config_sample.toml"
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
  echo "Signing OSX Binary ..."
  codesign --options runtime --force --verify --verbose --sign "${osxsign}" "${distDir}/dist/tari_base_node"
  echo "Verify signed OSX Binary ..."
  codesign --verify --deep --display --verbose=4 "${distDir}/dist/tari_base_node"
  spctl -a -v "${distDir}/dist/tari_base_node"
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
