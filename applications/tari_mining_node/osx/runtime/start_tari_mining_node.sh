#!/bin/bash
#
echo
echo "Starting Mining Node"
echo

# Initialize
if [ -z "${use_parent_paths}" ]
then
    base_path="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
    config_path="${base_path}/config"
    exe_path="${base_path}/runtime"
fi

tor_running=$(lsof -nP -iTCP:9050)
if [ -z "${tor_running}" ]
then
    echo "Starting Tor"
    open -a Terminal.app "${exe_path}/start_tor.sh"
    ping -c 15 localhost > /dev/null
fi

if [ ! -f "${config_path}/log4rs_mining_node.yml" ]
then
    echo Creating new "${config_path}/log4rs_mining_node.yml";
    init_flag="--init"
else
    echo Using existing "${config_path}/log4rs_mining_node.yml";
    init_flag=""
fi
echo

# Run
echo Spawning Console Wallet into new terminal..
echo "${exe_path}/tari_mining_node" ${init_flag} --config="${config_path}/config.toml" --log_config="${config_path}/log4rs_mining_node.yml" --base-path=${base_path} > $exe_path/tari_mining_node_command.sh
chmod +x $exe_path/tari_mining_node_command.sh

open -a terminal $exe_path/tari_mining_node_command.sh
echo
