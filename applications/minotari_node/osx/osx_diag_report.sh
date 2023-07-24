#!/bin/bash
#
# Diagnostic script for OSX to report information
#

# Kernel installed
uname -a

# Mac Model
sysctl hw.model | awk '{ print $2 }'

# CPU details
sysctl machdep.cpu

# Check 64bit CPU
sysctl hw.cpu64bit_capable

# pkg info
pkgutil --pkgs=com.tarilabs.pkg.basenode
pkgutil --files com.tarilabs.pkg.basenode

# XCode info
pkgutil --pkg-info=com.apple.pkg.CLTools_Executables
pkgutil --pkg-info=com.apple.pkg.DeveloperToolsCLI
xcode-select --version
xcode-select --print-path
xcodebuild -version
xcodebuild -showsdks
#softwareupdate --install --all

# brew info
brew config
brew missing
#brew doctor
