#!/bin/bash
#
echo
echo "Starting XMRig"
echo

if [ -z "${use_parent_paths}" ]
then
    base_path="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
    config_path="${base_path}/config"
    exe_path="${base_path}/runtime"
fi
xmrig_folder="${HOME}/xmrig"
xmrig_runtime="xmrig"

if [ ! -f "${xmrig_folder}/${xmrig_runtime}" ]
then
    "${exe_path}/install_xmrig.sh"
fi

if [ ! -f "${xmrig_folder}/${xmrig_runtime}" ]
then
    echo
    echo Problem with XMrig installation, "${xmrig_runtime}" not found!
    echo Please try installing this dependency using the manual procedure described in the README file.
    echo
else
    # Copy the config file to the XMRig folder
    cp -f "${config_path}/config_xmrig.json" "${xmrig_folder}/config.json"
    # Run
    echo Spawning XMRig into new terminal..
    open -a terminal "${xmrig_folder}/${xmrig_runtime}"
    echo
fi
