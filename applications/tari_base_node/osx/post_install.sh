#!/usr/bin/env bash
#
# Setup init Tari Base Node - Default
#

# Installer script for Tari base node. This script is bundled with OSX 
# versions of the Tari base node binary distributions.

# Debugging enabled
#set -x
#set -e

logo="
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣤⣾⣿⣿⣶⣤⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⣠⣶⣿⣿⣿⣿⠛⠿⣿⣿⣿⣿⣿⣦⣤⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⣤⣾⣿⣿⣿⡿⠋⠀⠀⠀⠀⠀⠀⠉⠛⠿⣿⣿⣿⣿⣷⣦⣄⠀⠀⠀⠀
⣴⣿⣿⣿⣿⣿⣉⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠛⢿⣿⣿⣿⣿⣶⣤
⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣶⣦⣤⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⣿⣿⣿
⣿⣿⣿⠀⠀⠀⠀⠉⠉⠛⠿⣿⣿⣿⣿⣿⣿⣿⣿⣶⣶⣤⣄⣀⠀⠀⠀⣿⣿⣿
⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⣿⠀⠈⠉⠛⠛⠿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
⢿⣿⣿⣷⣄⠀⠀⠀⠀⠀⠀⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣉⣿⣿⣿⣿⠟
⠀⠈⢿⣿⣿⣷⣄⠀⠀⠀⠀⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⢀⣴⣿⣿⣿⡿⠋⠀⠀
⠀⠀⠀⠈⢿⣿⣿⣷⡄⠀⠀⣿⣿⣿⠀⠀⠀⠀⢀⣴⣿⣿⣿⡿⠋⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠈⢿⣿⣿⣷⡀⣿⣿⣿⠀⠀⣤⣾⣿⣿⣿⠛⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠈⢿⣿⣿⣿⣿⣿⣾⣿⣿⣿⠟⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⢿⣿⣿⣿⣿⠟⠉⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠿⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
"

function display_center() {
#  columns="$(tput cols)"
  echo "$1" | while IFS= read -r line; do
    printf "%*s\n" $(( (${#line} + columns) / 2)) "$line"
  done
}

function banner() {
#  columns="$(tput cols)"
  for (( c=1; c<=columns; c++ )); do
    echo -n "—"
  done

  display_center " ✨  $1 ✨ "
  for (( c=1; c<=columns; c++ )); do
    echo -n "—"
  done

  echo
}

columns="$(tput cols)"
# shellcheck disable=SC2181
if [ $? -eq 0 ]; then
  echo "."
else
  # not in terminal - force colums
  echo ".."
  columns=80
fi

for line in $logo; do
  printf "%*s\n" $(( (31 + columns) / 2)) "$line"
done

if [ ! "$(uname)" == "Darwin" ]; then
  echo "Installer script meant for OSX"
  echo "Please visit https://tari.com/downloads/"
  echo " and download the binary distro for your platform"
  exit 1
fi

banner "Installing Brew and Tor for OSX ..."

#if ! xcode-select -p 1>&2 2>/dev/null; then
#  echo "XCode not installed. Installing..."
#  xcode-select --install 1>&2
#  echo "XCode successfully installed"
#else
#  echo "XCode already installed."
#fi

if [[ $(command -v brew) == "" ]]; then 
  echo "Homebrew not installed. Installing now ... "
#  ruby -e "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install)"
  bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install.sh)"
  echo "Homebrew successfully installed"
else
  echo "Updating Homebrew... "
  brew update
fi

echo "Brew packages ..."
brew services

# sqlite3
for pkg in sqlite tor torsocks wget; do
  if brew list -1 | grep -q "^${pkg}\$"; then
    echo "Package '$pkg' is installed"
  else
    echo "Package '$pkg' is not installed, installing now ..."
    brew install $pkg
    fi
done

echo "brew services list ..."
result=$(brew services list | grep -e "^tor")
echo "${result}"

if [[ $result =~ start ]];then
  echo "Tor is running, stopping before making changes"
  brew services stop tor
else
  echo "Tor is Stopped"
fi

echo "Setup Tor as a running service ..."
#/usr/local/etc/tor/torrc
if [ ! -f "/usr/local/etc/tor/torrc.custom" ]; then
#sudo tee -a /etc/tor/torrc.custom >> /dev/null << EOD
tee -a /usr/local/etc/tor/torrc.custom >> /dev/null << EOD

# basenode only supports single port
SocksPort 127.0.0.1:9050

# Control Port Enable
ControlPort 127.0.0.1:9051
CookieAuthentication 0
#HashedControlPassword ""

ClientOnly 1
ClientUseIPv6 1

SafeLogging 0

EOD
fi

if [ -f /usr/local/etc/tor/torrc ] ;then
  if grep -Fxq "%include /usr/local/etc/tor/torrc.custom" /usr/local/etc/tor/torrc ;then
    echo " torrc.custom already included for torrc ..."
  else
    echo "Adding torrc.custom include to torrc ..."
    #sudo tee -a /etc/tor/torrc >> /dev/null << EOD
    tee -a /usr/local/etc/tor/torrc >> /dev/null << EOD

# Include torrc.custom
%include /usr/local/etc/tor/torrc.custom
#

EOD
  fi
else
  echo "No /usr/local/etc/tor/torrc for Tor!"
  echo "Adding torrc.custom include to torrc ..."
  #sudo tee -a /etc/tor/torrc >> /dev/null << EOD
  tee -a /usr/local/etc/tor/torrc >> /dev/null << EOD

# Include torrc.custom
%include /usr/local/etc/tor/torrc.custom
#

EOD

fi

brew services start tor
brew services list

# Should rather add a check?
echo "Sleeping for 30sec while Tor warms up ..."
sleep 10
echo " ... 10sec ..."
sleep 10
echo " ... 10sec ..."
sleep 10
echo " ... 10sec ..."

# Check Tor service
#curl --socks5 localhost:9050 --socks5-hostname localhost:9050 -s https://check.torproject.org/ | cat | grep -m 1 Congratulations | xargs
#torsocks curl icanhazip.com
#curl icanhazip.com

wget -qO - https://api.ipify.org; echo
torsocks wget -qO - https://api.ipify.org; echo

NETWORK="stibbons"

# Fix permissions for everyone, users do not have these by default in /usr/local/*
INST_PATH="/usr/local/tari/"
sudo chmod -R 777 "${INST_PATH}"

DATA_DIR=${1:-"${INST_PATH}${NETWORK}"}

banner Installing your Tari Base Node
if [ ! -d "${DATA_DIR}/${NETWORK}" ]; then
  echo "Creating Tari data folder in $DATA_DIR"
  mkdir -p "${DATA_DIR}/${NETWORK}"
fi

# Shortcuts
# All
if [ -e ~/Desktop/tari_start_all ]; then
  echo "Desktop Link to Tari Ecosystem exists"
else
  ln -s ${INST_PATH}start_all ~/Desktop/tari_start_all
fi

# Base Node
if [ -e ~/Desktop/tari_base_node ]; then
  echo "Desktop Link to Tari Base Node exists"
else
  ln -s ${INST_PATH}start_tari_base_node ~/Desktop/tari_start_base_node
fi

# Console Wallet
if [ -e ~/Desktop/tari_start_console_wallet ]; then
  echo "Desktop Link to Tari Wallet exists"
else
  ln -s ${INST_PATH}start_tari_console_wallet ~/Desktop/tari_start_console_wallet
fi

# Mining Node
if [ -e ~/Desktop/tari_start_mining_node ]; then
  echo "Desktop Link to Tari Mining Node exists"
else
  ln -s ${INST_PATH}start_tari_mining_node ~/Desktop/tari_start_mining_node
fi

# Merge Mining Proxy
if [ -e ~/Desktop/tari_start_merge_mining_proxy ]; then
  echo "Desktop Link to Tari Merge Mining Proxy exists"
else
  ln -s ${INST_PATH}start_tari_merge_mining_proxy ~/Desktop/tari_start_merge_mining_proxy
fi

# XMRig
if [ -e ~/Desktop/tari_start_xmrig ]; then
   echo "Desktop Link to XMRig exists"
else
  ln -s ${INST_PATH}start_xmrig ~/Desktop/tari_start_xmrig
fi

banner Tari Base Node Install Done!
exit 0
