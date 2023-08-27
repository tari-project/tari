#!/bin/bash
# todo: update arg handling

if [ $# -eq 0 ]; then
    echo
    echo Please provide archive file name, \'.tar.gz\' will be appended
    echo
    exit
fi

rm -f "./$1.tar.gz" >/dev/null

target_release=${target_release:-target/release}

tarball_parent=${tarball_parent:-/tmp}
tarball_source=${tarball_source:-taiji_testnet}
tarball_folder=${tarball_parent}/${tarball_source}
if [ -d "${tarball_folder}" ]; then
    rm -f -r "${tarball_folder:?}"
fi

mkdir "${tarball_folder}"
mkdir "${tarball_folder}/config"
mkdir "${tarball_folder}/runtime"
mkdir "${tarball_folder}/scripts"

local_dir="$(
    cd "$(dirname "$0")" >/dev/null 2>&1 || exit 1
    pwd -P
)"
project_dir="$(dirname "$(greadlink -e "$local_dir")")"
app_dir="$(dirname "$(greadlink -e "$project_dir/applications/taiji_base_node")")"

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
# cp -f -P "${app_dir}/taiji_base_node/osx/start_all" "${tarball_folder}/start_all"
# cp -f "${app_dir}/taiji_base_node/osx/runtime/start_all.sh" "${tarball_folder}/runtime/start_all.sh"

# Base Node
cp -f -P "${app_dir}/taiji_base_node/osx/start_minotaiji_node" "${tarball_folder}/start_minotaiji_node"
cp -f -P "${app_dir}/taiji_base_node/osx/start_tor" "${tarball_folder}/start_tor"
cp -f "${app_dir}/taiji_base_node/osx/runtime/start_minotaiji_node.sh" "${tarball_folder}/runtime/start_minotaiji_node.sh"
cp -f "${app_dir}/taiji_base_node/osx/runtime/start_tor.sh" "${tarball_folder}/runtime/start_tor.sh"
cp -f "${project_dir}/${target_release}/taiji_base_node" "${tarball_folder}/runtime/taiji_base_node"

# Console Wallet
cp -f -P "${app_dir}/taiji_console_wallet/osx/start_taiji_console_wallet" "${tarball_folder}/start_taiji_console_wallet"
cp -f "${app_dir}/taiji_console_wallet/osx/runtime/start_taiji_console_wallet.sh" "${tarball_folder}/runtime/start_taiji_console_wallet.sh"
cp -f "${project_dir}/${target_release}/taiji_console_wallet" "${tarball_folder}/runtime/taiji_console_wallet"

# Miner
cp -f -P "${app_dir}/taiji_miner/osx/start_taiji_miner" "${tarball_folder}/start_taiji_miner"
cp -f "${app_dir}/taiji_miner/osx/runtime/start_taiji_miner.sh" "${tarball_folder}/runtime/start_taiji_miner.sh"
cp -f "${project_dir}/${target_release}/taiji_miner" "${tarball_folder}/runtime/taiji_miner"

# Merge Mining Proxy
cp -f -P "${app_dir}/taiji_merge_mining_proxy/osx/start_taiji_merge_mining_proxy" "${tarball_folder}/start_taiji_merge_mining_proxy"
cp -f -P "${app_dir}/taiji_merge_mining_proxy/osx/start_xmrig" "${tarball_folder}/start_xmrig"
cp -f "${app_dir}/taiji_merge_mining_proxy/osx/runtime/start_taiji_merge_mining_proxy.sh" "${tarball_folder}/runtime/start_taiji_merge_mining_proxy.sh"
cp -f "${app_dir}/taiji_merge_mining_proxy/osx/runtime/start_xmrig.sh" "${tarball_folder}/runtime/start_xmrig.sh"
cp -f "${project_dir}/${target_release}/taiji_merge_mining_proxy" "${tarball_folder}/runtime/taiji_merge_mining_proxy"

# 3rd party install
cp -f "${local_dir}/install_xmrig.sh" "${tarball_folder}/runtime/install_xmrig.sh"
cp -f "${local_dir}/get_xmrig_osx.ps1" "${tarball_folder}/runtime/get_xmrig_osx.ps1"

# Config
cat "${project_dir}"/common/config/presets/*.toml >"${tarball_folder}/config/config.toml"
cp -f "${project_dir}/common/xmrig_config/config_example_stagenet.json" "${tarball_folder}/config/xmrig_config_example_stagenet.json"
cp -f "${project_dir}/common/xmrig_config/config_example_mainnet.json" "${tarball_folder}/config/xxmrig_config_example_mainnet.json"
cp -f "${project_dir}/common/xmrig_config/config_example_mainnet_self_select.json" "${tarball_folder}/config/xmrig_config_example_mainnet_self_select.json"

# Scripts
cp -f "${local_dir}/osx_postinstall.sh" "${tarball_folder}/scripts/postinstall"

echo Files copied to "${tarball_folder}"
echo Creating archive...
echo

cd "${tarball_parent}" || exit 1
if [ -z "$2" ]; then
    tar -cvf "${local_dir}/$1.tar.gz" ${tarball_source}
    cd "${local_dir}" || exit 1
    echo
    echo Created "./$1.tar.gz" in "${local_dir}".
    echo
else
    echo
    echo Files copied to "${tarball_folder}". No zip created.
    echo
fi
