#!/bin/bash
#
sha3mining=1

start_tor(){
  echo "Starting Tor"
  gnome-terminal --working-directory=$PWD -- sh start_tor.sh
}

start_base_node() {
  start_tor
  echo "Starting Base Node"
  if [ $sha3mining -eq 0 ]; then
    gnome-terminal --working-directory=$PWD -- ./tari_base_node
  else
    gnome-terminal --working-directory=$PWD -- ./tari_base_node --enable_mining
  fi
}

start_console_wallet() {
 echo "Starting Console Wallet"
 gnome-terminal --working-directory=$PWD -- ./tari_console_wallet
}

start_merge_mining_proxy() {
 echo "Starting Merge Mining Proxy"
 gnome-terminal --working-directory=$PWD -- ./tari_merge_mining_proxy
}

download_xmrig() {
 echo "Downloading XMRig"
 mkdir xmrig
 curl -L -\# https://github.com/xmrig/xmrig/releases/download/v6.5.0/xmrig-6.5.0-focal-x64.tar.gz > xmrig.tar.gz && tar -xvf xmrig.tar.gz -C xmrig
 start_xmrig
}

start_xmrig() {
 echo 'Please input Monero Wallet Address for XMRig'
 read wallet_address
 echo 'Please input Merge Mining Proxy Address for XMRig (Default: 127.0.0.1:7878)'
 read proxy_address
 echo "Starting XMRig"
 cd xmrig/*
 gnome-terminal --working-directory=$PWD -- ./xmrig --donate-level 5 --url $proxy_address --user $wallet_address --coin monero --daemon
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
  while true; do
    read yn
    case $yn in
        [Yy]* ) sha3mining=0; merged_mining; break;;
        [Nn]* ) start_base_node; exit;;
        * ) echo "Please answer yes or no.";;
    esac
 done
}

echo "Do you want to enable mining?"
  while true; do
    read yn
    case $yn in
        [Yy]* )  mining; break;;
        [Nn]* )  sha3mining=0; start_base_node; exit;;
        * ) echo "Please answer yes or no.";;
    esac
 done

 read -rsp $'Press enter to quit...\n'
