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
    echo "${exe_path}/tari_merge_mining_proxy" --init --config "${config_path}/config.toml" --log_config "${config_path}/log4rs_merge_mining_proxy.yml" --base-path ${base_path} > $exe_path/mm_init.sh
    chmod +x $exe_path/mm_init.sh
    open -a terminal $exe_path/mm_init.sh
else
    echo Using existing "${config_path}/log4rs_merge_mining_proxy.yml";
fi
echo

# Run
echo "${exe_path}/tari_merge_mining_proxy" --config="${config_path}/config.toml" --log_config="${config_path}/log4rs_merge_mining_proxy.yml" --base-path=${base_path} > $exe_path/tari_merge_mining_proxy_command.sh
chmod +x $exe_path/tari_merge_mining_proxy_command.sh

ping -c 3 localhost > /dev/null
mm_running=$(lsof -nP -iTCP:7878)
if [ -z "${mm_running}" ]
then
    echo Spawning Merge Mining Proxy into new terminal..
    open -a terminal $exe_path/tari_merge_mining_proxy_command.sh
fi
echo

