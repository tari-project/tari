#!/usr/bin/env bash
#
# Script to setup tor as a service with control port
#

# Update to latest tor
if [ ! -f "/etc/apt/sources.list.d/tor.list" ]; then
  curl https://deb.torproject.org/torproject.org/A3C4F0F979CAA22CDBA8F512EE8CBC9E886DDD89.asc | sudo gpg --import
  gpg --export A3C4F0F979CAA22CDBA8F512EE8CBC9E886DDD89 | sudo apt-key add -
  sudo tee -a /etc/apt/sources.list.d/tor.list >> /dev/null << EOD
deb https://deb.torproject.org/torproject.org/ $(lsb_release -cs) main
EOD
fi

sudo apt-get update
sudo apt-get -y install tor tor-geoipdb torsocks deb.torproject.org-keyring sqlite

if [ ! -f "/etc/tor/torrc.custom" ]; then
  sudo tee -a /etc/tor/torrc.custom >> /dev/null << EOD
# basenode only supports single port
SocksPort 127.0.0.1:9050
# Control Port Enable
ControlPort 127.0.0.1:9051
CookieAuthentication 0
#HashedControlPassword ""
ClientOnly 1
ClientUseIPv6 1
SafeLogging 0
# Below might need to be enabled in some Tor challanged regions 
#ExitPolicy reject *:*
#ExitRelay 0
EOD
fi

if ! grep -Fxq "%include /etc/tor/torrc.custom" /etc/tor/torrc ;then
    sudo tee -a /etc/tor/torrc >> /dev/null << EOD
# Include torrc.custom
%include /etc/tor/torrc.custom
#
EOD
fi

sudo service tor restart

# curl --socks5 localhost:9050 --socks5-hostname localhost:9050 -s https://check.torproject.org/ | cat | grep -m 1 Congratulations | xargs
# torsocks curl icanhazip.com
# curl icanhazip.com

wget -qO - https://api.ipify.org; echo
torsocks wget -qO - https://api.ipify.org; echo

mkdir ~/.tari
mkdir ~/.tari/logs
