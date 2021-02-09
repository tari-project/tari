#!/bin/bash
#
# - XMRig latest
#   - Download `XMRig` at `https://github.com/xmrig/xmrig/releases/`

export xmrig_zip="xmrig-linux64.tar.gz"
export xmrig_folder="${HOME}/xmrig"
export xmrig_runtime="xmrig"
# TODO: Standardize on version 6.6.2 for now as later version(s) has a breaking interface change with the merge mining proxy
#export xmrig_repo="https://api.github.com/repos/xmrig/xmrig/releases/latest"
export xmrig_repo="https://api.github.com/repos/xmrig/xmrig/releases"

echo "Downloading and installing XMRig..."
echo

local_dir="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"

found=$(which pwsh)
if [ -z "${found}" ]
then
    if [ "$(uname)" == "Darwin" ]
    then
        brew install --cask powershell
    else
        sudo "${local_dir}/install_powershell_ubuntu.sh"
    fi
fi

rm -f "/tmp/${xmrig_zip}"

if [ "$(uname)" == "Darwin" ]
then
    pwsh -command "${local_dir}/get_xmrig_osx.ps1"
else
    pwsh -command "${local_dir}/get_xmrig_ubuntu.ps1"
fi

if [ -d "${xmrig_folder}" ]
then
    rm -f -r "${xmrig_folder:?}"
fi

mkdir "${xmrig_folder}"
tar -xvf "/tmp/${xmrig_zip}" -C ${xmrig_folder} > /dev/null
pwsh -command "Get-Childitem -File -Recurse '${xmrig_folder}/' | Move-Item  -Force -Destination '${xmrig_folder}'"
pwsh -command "Get-Childitem -Directory '${xmrig_folder}' | Remove-item -Force"

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
