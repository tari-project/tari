#!/usr/bin/env bash

set -e

if hash tor 2>/dev/null; then
  echo "Tor is already installed. To reinstall remove tor and run this script."
  exit
fi

function tor_installed_success() {
   echo
   echo "Tor installed."
   echo "You may want to run the tor proxy with the following command:"
   echo 'tor --allow-missing-torrc --ignore-missing-torrc --clientonly 1 --socksport 9050 --controlport 127.0.0.1:9060 --log "notice stdout" --clientuseipv6 1'
}

function install_tor_linux_apt() {
  if [ "$EUID" -ne 0 ]; then
    echo "Please run as root"
    exit 1
  fi

  RELEASE=`lsb_release -c -s`
  echo "Installing tor for $(lsb_release -i -s) $RELEASE using apt..."

   if [ "$RELEASE" != "cosmic" ]; then
   cat > /etc/apt/sources.list.d/tor-stable.list <<- EOF
deb https://deb.torproject.org/torproject.org ${RELEASE} main
deb-src https://deb.torproject.org/torproject.org ${RELEASE} main
EOF
  fi

   curl https://deb.torproject.org/torproject.org/A3C4F0F979CAA22CDBA8F512EE8CBC9E886DDD89.asc | gpg --import 1>&2
   gpg --export A3C4F0F979CAA22CDBA8F512EE8CBC9E886DDD89 | apt-key add - 1>&2

   apt update 1>&2
   apt install -y tor deb.torproject.org-keyring 1>&2
   systemctl disable tor.service
   kill `ps -e | grep tor | cut -d " " -f1` 2>/dev/null || true

   tor_installed_success
}

function install_tor_mac() {
  echo "Installing tor for Mac using homebrew..."
  brew install tor 1>&2

  tor_installed_success
}

case "$(uname -s)" in
    Linux*)
     if ! hash apt 2> /dev/null; then
        echo "Your system is not supported by this script."
        echo "Visit https://2019.www.torproject.org/docs/debian for details on installing tor on your system"
        exit 1
      fi

      install_tor_linux_apt
      ;;
    Darwin*)
      if ! hash brew 2> /dev/null; then
        echo "Homebrew is not installed. To install homebrew use:"
        echo '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install.sh)'
        exit 1
      fi
      install_tor_mac
      ;;
    CYGWIN*)
      alias apt=apt-cyg
      install_tor_linux_apt
      ;;
    *)
      echo "Unsupported platform $(uname -s)"
      exit 1
      ;;
esac
