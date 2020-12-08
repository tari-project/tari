#!/bin/bash
#
echo
echo "Starting Merge Mining Proxy"
echo

# Initialize
if [ -z "${use_parent_paths}" ]
then
    base_path="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
    config_path="${base_path}/config"
    exe_path="${base_path}/runtime"
fi
if [ ! -f "${config_path}/log4rs_merge_mining_proxy.yml" ]
then
    echo Creating new "${config_path}/log4rs_merge_mining_proxy.yml";
    gnome-terminal --working-directory="$PWD" -- "${exe_path}/tari_merge_mining_proxy" --init --config "${config_path}/config.toml" --log_config "${config_path}/log4rs_merge_mining_proxy.yml" --base-path ${base_path}
else
    echo Using existing "${config_path}/log4rs_merge_mining_proxy.yml";
fi
echo

# Run
echo Spawning Merge Mining Proxy into new terminal..
gnome-terminal --working-directory="$PWD" -- "${exe_path}/tari_merge_mining_proxy" --config "${config_path}/config.toml" --log_config "${config_path}/log4rs_merge_mining_proxy.yml" --base-path ${base_path}
echo

