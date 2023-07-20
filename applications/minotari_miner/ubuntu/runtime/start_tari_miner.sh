#!/bin/bash
#
echo
echo "Starting Miner"
echo

# Initialize
if [ -z "${use_parent_paths}" ]
then
    base_path="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
    config_path="${base_path}/config"
    exe_path="${base_path}/runtime"
fi
"${exe_path}/start_tor.sh"

if [ ! -f "${config_path}/log4rs_miner.yml" ]
then
    echo Creating new "${config_path}/log4rs_miner.yml";
    init_flag="--init"
else
    echo Using existing "${config_path}/log4rs_miner.yml";
    init_flag=""
fi
echo

# Run
echo Spawning Miner into new terminal..
echo
gnome-terminal --working-directory="$PWD" -- "${exe_path}/minotari_miner" ${init_flag} --config "${config_path}/config.toml" --log_config "${config_path}/log4rs_miner.yml" --base-path ${base_path}
