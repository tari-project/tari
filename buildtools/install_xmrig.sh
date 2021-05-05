#!/bin/bash
#
# - XMRig latest
#   - Download `XMRig` at `https://github.com/xmrig/xmrig/releases/`

if [ "$(uname)" == "Darwin" ]
then
     export xmrig_archive="xmrig-macos-x64.tar.gz"
else
     export xmrig_archive="xmrig-linux64.tar.gz"
fi

export xmrig_folder="${HOME}/xmrig"
export xmrig_runtime="xmrig"
export xmrig_repo="https://api.github.com/repos/xmrig/xmrig/releases/latest"

local_dir="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"

echo "Dependencies will be installed if missing"
echo "Checking dependencies..."
echo

# Install Brew
if [ "$(uname)" == "Darwin" ]
then
    found_brew=$(which brew)
    if [ -z "${found_brew}" ]
    then
         ruby -e "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install)"
    fi
fi

# Install Curl
found_curl=$(which curl)
if [ -z "${found_curl}" ]
then
    if [ "$(uname)" == "Darwin" ]
    then
        brew install curl
    else
        sudo apt-get install curl
    fi
fi

# Install Jq
found_jq=$(which jq)
if [ -z "${found_jq}" ]
then
    if [ "$(uname)" == "Darwin" ]
    then
        brew install jq
    else
        sudo apt-get install jq
    fi
fi

echo "Downloading and installing XMRig..."
echo

rm -f "/tmp/${xmrig_archive}"

if [ "$(uname)" == "Darwin" ]
then
    curl -L $(curl ${xmrig_repo} | jq -r '.assets[] | select(.name | endswith("-macos-x64.tar.gz")) | .browser_download_url') -o /tmp/${xmrig_archive}
else
    curl -L $(curl ${xmrig_repo} | jq -r '.assets[] | select(.name | endswith("-linux-x64.tar.gz")) | .browser_download_url') -o /tmp/${xmrig_archive}
fi

if [ -d "${xmrig_folder}" ]
then
    rm -f -r "${xmrig_folder:?}"
fi

mkdir "${xmrig_folder}"
tar -xvf "/tmp/${xmrig_archive}" -C ${xmrig_folder} --strip-components 1 > /dev/null

echo "Verifying XMRig installation..."
echo
# Test installation
if [ ! -f "${xmrig_folder}/${xmrig_runtime}" ]
then
    echo
    echo Problem with XMrig installation, "${xmrig_runtime}" not found!
    echo Please try installing this dependency using the manual procedure described in the README file.
    echo
else
    echo
    echo New XMRig installation found at "${xmrig_folder}"
    echo
fi
