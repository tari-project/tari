# Tari Wallet FFI

Foreign Function interface for the Tari Android and Tari iOS Wallets.

This crate is part of the [Tari Cryptocurrency](https://tari.com) project.

# Build setup (Mac)

## Homebrew

Install Brew
```Shell Script
/usr/bin/ruby -e "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install)"
```

Run the following to install the needed bottles
```Shell Script
brew install pkgconfig
brew install git
brew install make
brew install cmake
brew install autoconf
brew install automake
brew install libtool
brew install zmq
```

## iOS Dependencies

Install [XCode](https://apps.apple.com/za/app/xcode/id497799835?mt=12) and then the XCode Command Line Tools with the following command
```Shell Script
xcode-select --install
```

For macOS Mojave additional headers need to be installed, run
```Shell Script
open /Library/Developer/CommandLineTools/Packages/macOS_SDK_headers_for_macOS_10.14.pkg
```
and follow the prompts

## Android Dependencies

Download the [Android NDK Bundle](https://developer.android.com/ndk/downloads)

## Enable Hidden Files

Run the following to show hidden files and folders
```Shell Script
defaults write com.apple.finder AppleShowAllFiles -bool YES
killall Finder
```
## The Code

Clone the following git repositories
1. [Tari](https://github.com/tari-project/tari.git)
2. [Wallet-Android](https://github.com/tari-project/wallet-android.git)
3. [Wallet-iOS](https://github.com/tari-project/wallet-ios.git)

Afterwards ```cd``` into the Tari repository and run the following
```Shell Script
git submodule init
git config submodule.recurse true
git submodule update --recursive --remote
```

## Rust
Install [Rust](https://www.rust-lang.org/tools/install)

Install the following tools and system images
```Shell Script
rustup toolchain add nightly-2019-10-04
rustup default nightly-2019-10-04
rustup component add rustfmt --toolchain nightly
rustup component add clippy
rustup target add x86_64-linux-android 
rustup target add aarch64-linux-android 
rustup target add armv7-linux-androideabi 
rustup target add i686-linux-android arm-linux-androideabi
rustup target add aarch64-apple-ios
rustup target add x86_64-apple-ios
cargo install cargo-ndk
cargo install cargo-lipo
```

## Build Configuration

To configure the build, ```cd``` to the Tari repository and then 
```Shell Script
cd base_layer/wallet_ffi
open build.sample.config
```

Which will present you with the file contents as follows
```text
BUILD_ANDROID=1
BUILD_IOS=1
SQLITE_SOURCE=https://www.sqlite.org/snapshot/sqlite-snapshot-201911192122.tar.gz
NDK_PATH=/Users/user/Desktop/android-ndk-r20
PKG_PATH=/usr/local/Cellar/zeromq/4.3.2/lib/pkgconfig
ZMQ_REPO=https://github.com/zeromq/libzmq.git
ANDROID_WALLET_PATH=/Users/user/Desktop/wallet-android
IOS_WALLET_PATH=/Users/user/Desktop/wallet-ios
TARI_REPO_PATH=/Users/user/Desktop/tari-main
```
The following changes need to be made to the file
1. ```NDK_PATH``` needs to be changed to the directory of the Android NDK Bundle.
2. ```ANDROID_WALLET``` needs to be changed to the path of the Android-Wallet repository
3. ```IOS_WALLET_PATH``` needs to be changed to the path of the Wallet-iOS repository
4. ```TARI_REPO_PATH``` needs to be changed to the path of the Tari repository

Save the file and rename it to ```build.config```

## Building the Libraries

To build the libraries, ```cd``` to the Tari repository and then 
```Shell Script
cd base_layer/wallet_ffi
sh mobile_build.sh
```

The relevant libraries will then be built and placed in the appropriate directories of the Wallet-iOS and Wallet-Android repositories. 

