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
brew install openssl@1.1
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
and follow the prompts.

For Catalina, if you get compilation errors such as these:

    xcrun: error: SDK "iphoneos" cannot be located
    xcrun: error: unable to lookup item 'Path' in SDK 'iphoneos'

Switch the XCode app defaults with:

    sudo xcode-select --switch /Applications/Xcode.app

**Note:** If this command fails, XCode was not found and needs to be installed/re-installed.

For Big Sur, currently it seems only thin libraries for openssl are being distributed via brew (instead of fat ones),
should you run into linker errors in the logs:
```Shell Script
git clone https://github.com/StriderDM/OpenSSL-for-iPhone.git
cd OpenSSL-for-iPhone
git checkout shared_lib_and_mobile_optimizations
./build-libssl.sh --version=1.1.1h
```

After the script finishes building the libraries, copy the following:
```
./bin/iPhoneOS14.3-arm64.sdk/lib/libcrypto.1.1.dylib
./bin/iPhoneOS14.3-arm64.sdk/lib/libcrypto.dylib
./bin/iPhoneOS14.3-arm64.sdk/lib/libssl.1.1.dylib
./bin/iPhoneOS14.3-arm64.sdk/lib/libssl.dylib

```

To:
```
~/.rustup/toolchains/nightly-x86_64-apple-darwin/lib/rustlib/aarch64-apple-ios/lib
```

And the following:
```
./bin/iPhoneSimulator14.3-x86_64.sdk/lib/libcrypto.1.1.dylib
./bin/iPhoneSimulator14.3-x86_64.sdk/lib/libcrypto.dylib
./bin/iPhoneSimulator14.3-x86_64.sdk/lib/libssl.1.1.dylib
./bin/iPhoneSimulator14.3-x86_64.sdk/lib/libssl.dylib
```

To:
```
~/.rustup/toolchains/nightly-x86_64-apple-darwin/lib/rustlib/x86_64-apple-ios/lib
```

Note: This is purely to resolve linker issues during the library build (prior to trying to include it in the iOS
wallet). This dependency is already included in the dependencies to be built for the iOS wallet.

## Android Dependencies

Install [Android Studio](https://developer.android.com/studio) and then use the SDK Manager to install the Android NDK
along with the SDK of your choice (Android Q is recommended). Not all of these tools are required, but will come in
handy during Rust / Android development:

* LLDB
* NDK (Side by side)
* Android SDK Command-line Tools (latest)
* Android SDK Platform Tools
* Android SDK Tools
* CMake

When setting up an AVD (Android Virtual Device) please note that a 64-bit image (x86_64) needs to be used and not a
32-bit image (x86). This is to run the application on the simulator with these libraries.

Alternatively, download the [Android NDK Bundle](https://developer.android.com/ndk/downloads) directly.

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
rustup toolchain add nightly-2024-02-04
rustup default nightly-2024-02-04
rustup component add rustfmt --toolchain nightly
rustup component add clippy
rustup target add x86_64-apple-ios aarch64-apple-ios # iPhone and emulator cross compiling
rustup target add x86_64-linux-android aarch64-linux-android armv7-linux-androideabi # Android device cross compiling
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
CARGO_CLEAN=1
SQLITE_SOURCE=https://www.sqlite.org/snapshot/sqlite-snapshot-201911192122.tar.gz
NDK_PATH=$HOME/android-ndk-r20
PKG_PATH=/usr/local/opt/openssl@1.1/lib/pkgconfig
ANDROID_WALLET_PATH=$HOME/wallet-android
IOS_WALLET_PATH=$HOME/wallet-ios
TARI_REPO_PATH=$HOME/tari-main
```
The following changes need to be made to the file
1. ```NDK_PATH``` needs to be changed to the directory of the Android NDK Bundle.
1. ```ANDROID_WALLET``` needs to be changed to the path of the Android-Wallet repository
1. ```IOS_WALLET_PATH``` needs to be changed to the path of the Wallet-iOS repository
1. ```CARGO_CLEAN``` if set to 1, the cargo clean command will be run before the build
1. ```TARI_REPO_PATH``` needs to be changed to the path of the Tari repository (Optional - defaults to current repo)
1. ```BUILD_ANDROID``` can be set to ```0``` to disable Android library build
1. ```BUILD_IOS``` can be set to ```0``` to disable iOS library build
1. ```PKG_PATH``` needs to be changed to OpenSSL 1.1 pkgconfig path (only necessary for iOS build)

Save the file and rename it to ```build.config```

## Building the Libraries

To build the libraries, ```cd``` to the Tari repository and then
```Shell Script
cd base_layer/wallet_ffi
sh mobile_build.sh
```

The relevant libraries will then be built and placed in the appropriate directories of the Wallet-iOS and Wallet-Android repositories.
