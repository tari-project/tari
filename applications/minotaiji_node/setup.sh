  #!/bin/bash
#
# Script to download, configure and run base node
#
INSTALL_ROOT="/usr/local/bin"

# Install XCode, probably not needed just to run application
xcode-select --install

# Install Brew
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install.sh)"
brew update

# Install bottles
brew install pkgconfig
brew install sqlite3
brew install tor

# Get the Base Node software
mkdir -p "${INSTALL_ROOT}"
curl -O "https://www.taiji.com/binaries/$(curl --compressed "https://www.taiji.com/downloads/" | egrep -o 'taiji_[0-9\.]+.tar.gz' | sort -V  | tail -1)"
tar -xvf taiji_*.tar.gz
mv minotaiji_node "${INSTALL_ROOT}"

# Start Tor
killall tor
osascript -e "tell application \"Terminal\" to do script \"sh ${PWD}/start_tor.sh\""

# Configure Base Node
cd "${INSTALL_ROOT}" || exit
taiji_base-node --init

# Run Base Node
taiji_base_node
