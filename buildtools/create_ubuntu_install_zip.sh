#!/bin/bash
#

if [ $# -eq 0 ]; then
    echo
    echo Please provide archive file name, \'.tar.gz\' will be appended
    echo
    exit
fi
rm -f "./$1.tar.gz" >/dev/null

tarball_parent=${tarball_parent:-/tmp}
tarball_source=${tarball_source:-tari_testnet}
tarball_folder=${tarball_parent}/${tarball_source}
if [ -d "${tarball_folder}" ]; then
    rm -f -r "${tarball_folder:?}"
fi

mkdir "${tarball_folder}"
mkdir "${tarball_folder}/config"
mkdir "${tarball_folder}/runtime"

local_dir="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
project_dir="$(dirname "$(readlink -e "$local_dir")")"
app_dir="$(dirname "$(readlink -e "$project_dir/applications/tari_base_node")")"

if [ ! "${app_dir}" == "${project_dir}/applications" ]; then
    echo
    echo Please run this script from '/buildtools'
    echo
    exit
else
    echo
    echo Found project folders:
    echo "  ${project_dir}"
    echo "  ${local_dir}"
    echo "  ${app_dir}"
    echo
fi

# One click miner
cp -f -P "${app_dir}/tari_base_node/linux/start_all" "${tarball_folder}/start_all"
cp -f "${app_dir}/tari_base_node/linux/runtime/start_all.sh" "${tarball_folder}/runtime/start_all.sh"

# Base Node
cp -f -P "${app_dir}/tari_base_node/linux/setup_tor_service" "${tarball_folder}/setup_tor_service"
cp -f -P "${app_dir}/tari_base_node/linux/start_tari_base_node" "${tarball_folder}/start_tari_base_node"
cp -f -P "${app_dir}/tari_base_node/linux/start_tor" "${tarball_folder}/start_tor"
cp -f "${app_dir}/tari_base_node/linux/runtime/setup_tor_service.sh" "${tarball_folder}/runtime/setup_tor_service.sh"
cp -f "${app_dir}/tari_base_node/linux/runtime/start_tari_base_node.sh" "${tarball_folder}/runtime/start_tari_base_node.sh"
cp -f "${app_dir}/tari_base_node/linux/runtime/start_tor.sh" "${tarball_folder}/runtime/start_tor.sh"
cp -f "${project_dir}/target/release/tari_base_node" "${tarball_folder}/runtime/tari_base_node"

# Console Wallet
cp -f -P "${app_dir}/tari_console_wallet/linux/start_tari_console_wallet" "${tarball_folder}/start_tari_console_wallet"
cp -f "${app_dir}/tari_console_wallet/linux/runtime/start_tari_console_wallet.sh" "${tarball_folder}/runtime/start_tari_console_wallet.sh"
cp -f "${project_dir}/target/release/tari_console_wallet" "${tarball_folder}/runtime/tari_console_wallet"

# Mining Node
cp -f -P "${app_dir}/tari_mining_node/linux/start_tari_mining_node" "${tarball_folder}/start_tari_mining_node"
cp -f "${app_dir}/tari_mining_node/linux/runtime/start_tari_mining_nodet.sh" "${tarball_folder}/runtime/start_tari_mining_node.sh"
cp -f "${project_dir}/target/release/tari_mining_node" "${tarball_folder}/runtime/tari_mining_node"

# Merge Mining Proxy
cp -f -P "${app_dir}/tari_merge_mining_proxy/linux/start_tari_merge_mining_proxy" "${tarball_folder}/start_tari_merge_mining_proxy"
cp -f -P "${app_dir}/tari_merge_mining_proxy/linux/start_xmrig" "${tarball_folder}/start_xmrig"
cp -f "${app_dir}/tari_merge_mining_proxy/linux/runtime/start_tari_merge_mining_proxy.sh" "${tarball_folder}/runtime/start_tari_merge_mining_proxy.sh"
cp -f "${app_dir}/tari_merge_mining_proxy/linux/runtime/start_xmrig.sh" "${tarball_folder}/runtime/start_xmrig.sh"
cp -f "${project_dir}/target/release/tari_merge_mining_proxy" "${tarball_folder}/runtime/tari_merge_mining_proxy"

# Collectibles
cp -f "${project_dir}/target/release/tari_collectibles" "${tarball_folder}/runtime/tari_collectibles"

# Validator node
cp -f "${project_dir}/target/release/tari_validator_node" "${tarball_folder}/runtime/tari_validator_node"

# Launchpad
cp -f "${project_dir}/target/release/tari_launchpad" "${tarball_folder}/runtime/tari_launchpad"

# 3rd party install
cp -f "${local_dir}/install_xmrig.sh" "${tarball_folder}/runtime/install_xmrig.sh"
cp -f "${local_dir}/get_xmrig_ubuntu.ps1" "${tarball_folder}/runtime/get_xmrig_ubuntu.ps1"
cp -f "${local_dir}/install_powershell_ubuntu.sh" "${tarball_folder}/runtime/install_powershell_ubuntu.sh"

# Config
cat "${project_dir}/common/config/presets/*.toml" >"${tarball_folder}/config/config.toml"
cp -f "${project_dir}/common/xmrig_config/config_example_stagenet.json" "${tarball_folder}/config/xmrig_config_example_stagenet.json"
cp -f "${project_dir}/common/xmrig_config/config_example_mainnet.json" "${tarball_folder}/config/xxmrig_config_example_mainnet.json"
cp -f "${project_dir}/common/xmrig_config/config_example_mainnet_self_select.json" "${tarball_folder}/config/xmrig_config_example_mainnet_self_select.json"

echo Files copied to "${tarball_folder}"
echo Creating archive...
echo

cd "${tarball_parent}"
tar -cvf "${local_dir}/$1.tar.gz" ${tarball_source}
cd "${local_dir}"
echo
echo Created "./$1.tar.gz" in "${local_dir}"
echo
