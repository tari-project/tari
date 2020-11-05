#!/bin/bash
#

export SHA3_MINING = 1

start_tor(){
  echo "Starting Tor"
  open -a Terminal.app "start_tor.sh"
}

start_base_node() {
  start_tor
  echo "Starting Base Node"
  if [[ $SHA3_MINING -eq 0 ]]; then
    open tari_base_node
  else
    open tari_base_node --args --enable_mining
  fi
}

start_console_wallet() {
 echo "Starting Console Wallet"
 open tari_console_wallet
}

start_merge_mining_proxy() {
 echo "Starting Merge Mining Proxy"
 open tari_merge_mining_proxy
}

download_xmrig() {
 echo "Downloading XMRig"
 mkdir xmrig
 curl -L -\# https://github.com/xmrig/xmrig/releases/download/v6.5.0/xmrig-6.5.0-macos-x64.tar.gz > xmrig.tar.gz && tar -xvf xmrig.tar.gz -C xmrig
 start_xmrig
}

start_xmrig() {
 echo 'Please input Monero Wallet Address for XMRig'
 cd xmrig
 read wallet_address
  echo 'Please input Merge Mining Proxy Address for XMRig (Default: 127.0.0.1:7878)'
 read proxy_address
 echo "Starting XMRig"
 cd *
 open xmrig --args --donate-level 5 --url $proxy_address --user $wallet_address --coin monero --daemon
 cd ../..
}

merged_mining() {
  start_base_node
  start_console_wallet
  start_merge_mining_proxy
  #check xmrig directory exists
  if [ -d xmrig ]; then
    start_xmrig
  else
    download_xmrig
    start_xmrig
  fi
}

mining() {
  echo "Merged mining?"
  select yn in "Yes" "No"; do
      case $yn in
          Yes ) export SHA3_MINING = 0; merged_mining; break;;
          No ) start_base_node; exit;;
      esac
  done
}

echo "Do you want to enable mining?"
select yn in "Yes" "No"; do
    case $yn in
        Yes ) mining; break;;
        No ) export SHA3_MINING = 0; start_base_node; exit;;
    esac
done

read -rsp $'Press enter to quit...\n'
