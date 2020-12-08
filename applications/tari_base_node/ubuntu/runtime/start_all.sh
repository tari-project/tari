#!/bin/bash
#
# Initialize
export base_path="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
export config_path="${base_path}/config"
export exe_path="${base_path}/runtime"
echo
echo "base_path:   $base_path"
echo "config_path: $config_path"
echo "exe_path:    $exe_path"
echo
export use_parent_paths=true
sha3_mining=1

call_base_node() {
    if [[ $sha3_mining -eq 1 ]]; then
        export enable_mining="--enable_mining"
    fi
    "${exe_path}/start_tari_base_node.sh"
}

call_console_wallet() {
    "${exe_path}/start_tari_console_wallet.sh"
}

call_merge_mining_proxy() {
    "${exe_path}/start_tari_merge_mining_proxy.sh"
}

call_xmrig() {
    "${exe_path}/start_xmrig.sh"
}

merged_mining() {
    call_base_node
    call_console_wallet
    call_merge_mining_proxy
    call_xmrig
}

mining() {
    echo "Merged mining?"
    while true; do
        read yn
        case $yn in
            [Yy]* ) sha3_mining=0; merged_mining; break;;
            [Nn]* ) call_base_node; call_console_wallet; exit;;
            * ) echo "Please answer yes or no.";;
        esac
   done
}

echo
echo "Do you want to enable mining?"
    while true; do
        read yn
        case $yn in
            [Yy]* )  mining; break;;
            [Nn]* )  sha3_mining=0; call_base_node; call_console_wallet; exit;;
            * ) echo "Please answer yes or no.";;
        esac
 done
