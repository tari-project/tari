#!/usr/bin/env bash
#
# Setup init Minotari Base Node - Default
#

# Installer script for Minotari base node. This script is bundled with OSX 
# versions of the Minotari base node binary distributions.

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
  for (( c=1; c<=$columns; c++ )); do
    echo -n "—"
  done

  display_center " ✨  $1 ✨ "
  for (( c=1; c<=$columns; c++ )); do
    echo -n "—"
  done

  echo
}

columns="$(tput cols)"
if [ $? -eq 0 ]; then
  echo "."
else
  # not in terminal - force colums
  echo ".."
  colums=80
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

banner "Installing XCode/Brew and Tor for OSX ..."

if !xcode-select -p 1>&2 2>/dev/null; then
  echo "XCode not installed. Installing..."
#  xcode-select --install 1>&2
  echo "XCode successfully installed"
else
  echo "XCode already installed."
fi

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

echo "brew serivces list ..."
result=$(brew services list | grep -e "^tor")
echo $result

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

DATA_DIR=${1:-"$HOME/.tari"}
NETWORK=stibbons

banner Installing and setting up your Tari Base Node
if [ ! -d "$DATA_DIR/$NETWORK" ]; then
  echo "Creating Tari data folder in $DATA_DIR"
  mkdir -p $DATA_DIR/$NETWORK
fi

if [ ! -f "$DATA_DIR/config.toml" ]; then
  echo "Copying configuraton files"
#  cp tari_config_example.toml $DATA_DIR/config.toml
#  cp log4rs_sample_base_node.yml $DATA_DIR/log4rs_base_node.yml

  # Configure Base Node
  minotari_node --init

  echo "Configuration complete."
fi

banner Running Tari Base Node
# Run Base Node
if [ -e ~/Desktop/minotari_node ]; then
  echo "Desktop Link to Minotari Base Node exits"
else
  ln -s /usr/local/bin/minotari_node ~/Desktop/minotari_node
fi
cd "$DATA_DIR"
open /usr/local/bin/minotari_node

# Start Minotari Base Node in another Terminal
#osascript -e "tell application \"Terminal\" to do script \"sh ${PWD}/start_tor.sh\""

banner Minotari Base Node Install Done!
exit 0
