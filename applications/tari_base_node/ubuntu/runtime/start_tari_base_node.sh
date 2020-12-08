#!/bin/bash
#
echo
echo "Starting Base Node"
echo

# Initialize
if [ -z "${use_parent_paths}" ]
then
    base_path="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
    config_path="${base_path}/config"
    exe_path="${base_path}/runtime"
fi
"${exe_path}/start_tor.sh"

if [ ! -f "${config_path}/base_node_id.json" ] || [ ! -f "${config_path}/wallet_id.json" ]
then
    if [ ! -f "${config_path}/base_node_id.json" ]
    then
        echo Creating new "${config_path}/base_node_id.json";
    fi
    if [ ! -f "${config_path}/wallet_id.json" ]
    then
        echo Creating new "${config_path}/wallet_id.json";
    fi
    "${exe_path}/tari_base_node" --create_id --init --config "${config_path}/config.toml" --log_config "${config_path}/log4rs_base_node.yml" --base-path ${base_path}
else
    echo Using existing "${config_path}/base_node_id.json";
    echo Using existing "${config_path}/wallet_id.json";
fi
if [ ! -f "${config_path}/log4rs_base_node.yml" ]
then
    echo Creating new "${config_path}/log4rs_base_node.yml";
    "${exe_path}/tari_base_node" --init --config "${config_path}/config.toml" --log_config "${config_path}/log4rs_base_node.yml" --base-path ${base_path}
else
    echo Using existing "${config_path}/log4rs_base_node.yml";
fi
echo

# Run
echo Spawning Base Node into new terminal..
gnome-terminal --working-directory="$PWD" -- "${exe_path}/tari_base_node" --config "${config_path}/config.toml" --log_config "${config_path}/log4rs_base_node.yml" --base-path ${base_path} ${enable_mining}
echo
