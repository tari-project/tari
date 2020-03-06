#!/usr/bin/env bash

set -e

SOCKSPORT=${SOCKSPORT:-9050}
CONTROLPORT=${CONTROLPORT:-127.0.0.1:9051}

function display_center() {
    columns="$(tput cols)"
    echo "$1" | while IFS= read -r line; do
        printf "%*s\n" $(( (${#line} + columns) / 2)) "$line"
    done
}

function banner() {
  columns="$(tput cols)"
  for (( c=1; c<=$columns; c++ )); do
      echo -n "—"
  done

  display_center " ✨  $1 ✨ "
  for (( c=1; c<=$columns; c++ )); do
      echo -n "—"
  done

  echo
}

function run_tor() {
  echo
  banner "Running Tor"

  tor --allow-missing-torrc --ignore-missing-torrc \
     --clientonly 1 \
     --socksport $SOCKSPORT \
     --controlport $CONTROLPORT\
     --log "notice stdout" \
     --clientuseipv6 1
}

if hash tor 2>/dev/null; then
  run_tor
  exit
fi

function install_tor_linux_apt() {
  if [ "$EUID" -ne 0 ]; then
    echo "Please run as root"
    exit 1
  fi

  RELEASE=`lsb_release -c -s`
  banner "Installing tor for $(lsb_release -i -s) $RELEASE..."

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

   run_tor
}

function install_tor_mac() {
  banner "Installing Tor for Mac"

  if !xcode-select -p 1>&2 2>/dev/null; then
    echo "XCode not installed. Installing..."
    xcode-select --install 1>&2
    echo "XCode successfully installed"
  fi

  if !hash brew 2> /dev/null; then
    echo "Homebrew not installed. Installing..."
    bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install.sh)"
    echo "Homebrew successfully installed"
  fi

  brew install tor 1>&2
  echo "Tor successfully installed"

  run_tor
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
