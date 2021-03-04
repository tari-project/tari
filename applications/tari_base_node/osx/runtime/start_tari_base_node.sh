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

tor_running=$(lsof -nP -iTCP:9050)
if [ -z "${tor_running}" ]
then
    echo "Starting Tor"
    open -a Terminal.app "${exe_path}/start_tor.sh"
    ping -c 15 localhost > /dev/null
fi

if [ ! -f "${config_path}/base_node_id.json" ]
then
    if [ ! -f "${config_path}/base_node_id.json" ]
    then
        echo Creating new "${config_path}/base_node_id.json";
    fi
    "${exe_path}/tari_base_node" --create_id --init --config "${config_path}/config.toml" --log_config "${config_path}/log4rs_base_node.yml" --base-path ${base_path}
else
    echo Using existing "${config_path}/base_node_id.json";
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
# `open` command won't pass arguments via `--args`...
# https://gist.github.com/delta1/8ffc61200b650ab471e83f008645b01c
# so hack around it. 🔪

echo "${exe_path}/tari_base_node" --config="${config_path}/config.toml" --log_config="${config_path}/log4rs_base_node.yml" --base-path="${base_path}" > $exe_path/tari_base_node_command.sh
chmod +x $exe_path/tari_base_node_command.sh

open -a terminal $exe_path/tari_base_node_command.sh
echo
