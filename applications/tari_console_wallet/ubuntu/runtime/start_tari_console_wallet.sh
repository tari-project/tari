#!/bin/bash
#
echo
echo "Starting Console Wallet"
echo

# Initialize
if [ -z "${use_parent_paths}" ]
then
    base_path="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
    config_path="${base_path}/config"
    exe_path="${base_path}/runtime"
fi
"${exe_path}/start_tor.sh"

if [ ! -f "${config_path}/console_wallet_id.json" ]
then
    echo Creating new "${config_path}/console_wallet_id.json";
    "${exe_path}/tari_console_wallet" --create_id --init --config "${config_path}/config.toml" --log_config "${config_path}/log4rs_console_wallet.yml" --base-path ${base_path}
else
    echo Using existing "${config_path}/console_wallet_id.json";
fi
if [ ! -f "${config_path}/log4rs_console_wallet.yml" ]
then
    echo Creating new "${config_path}/log4rs_console_wallet.yml";
    "${exe_path}/tari_console_wallet" --init --config "${config_path}/config.toml" --log_config "${config_path}/log4rs_console_wallet.yml" --base-path ${base_path}
else
    echo Using existing "${config_path}/log4rs_console_wallet.yml";
fi
echo

# Run
echo Spawning Console Wallet into new terminal..
gnome-terminal --working-directory="$PWD" -- "${exe_path}/tari_console_wallet" --config "${config_path}/config.toml" --log_config "${config_path}/log4rs_console_wallet.yml" --base-path ${base_path}
echo
