#!/bin/bash
NDK_VERSION=r21b
ANDROID_NDK_HOME=/opt/android-ndk

# Download and install NDK
mkdir -p /opt/android-ndk-tmp &&
    cd /opt/android-ndk-tmp &&
    wget https://dl.google.com/android/repository/android-ndk-${NDK_VERSION}-linux-x86_64.zip &&
    cd /opt/android-ndk-tmp &&
    unzip android-ndk-${NDK_VERSION}-linux-x86_64.zip &&
    mv /opt/android-ndk-tmp/android-ndk-${NDK_VERSION} /opt/android-ndk
# add to PATH
PATH=${PATH}:${ANDROID_NDK_HOME}
NDK_PATH=${ANDROID_NDK_HOME}

#Fix for missing header, c code should reference limits.h instead of syslimits.h, happens with code that has been around for a long time.
mkdir -p "${NDK_PATH}"/sources/cxx-stl/llvm-libc++/include/sys
cp "${NDK_PATH}/sources/cxx-stl/llvm-libc++/include/limits.h" "${NDK_PATH}/sources/cxx-stl/llvm-libc++/include/sys/syslimits.h"

# Add Android ABIs
TARGETS="x86_64-linux-android aarch64-linux-android armv7-linux-androideabi i686-linux-android arm-linux-androideabi"
LEVEL=24
NDK_HOME=${NDK_PATH}
NDK_TOOLCHAIN_VERSION=clang
rustup target add "${TARGETS}"

# Build Android libs
# ADD scripts /scripts
# We'll build a version of the library for every platform given in `PLATFORMS`. Separate multiple platforms by a semicolon
# ENV PLATFORMS "x86_64-linux-android;aarch64-linux-android;i686-linux-android;armv7-linux-androideabi"
# Cargo will build the source code found in SRC_DIR
# ENV SRC_DIR "/src"
# You can pass any flags you wish to cargo by overriding the CARGO_FLAGS envar
CARGO_FLAGS="--package tari_wallet_ffi --lib --release"
CARGO_HTTP_MULTIPLEXING="false"

# Converts a rust option from the config file format into the corresponding env variable name
# E.G.  "target.x86_64-unknown-linux-gnu.runner" => "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER"
function argify() {
    arg=$1
    arg=${arg^^}
    arg=${arg//-/_}
    arg=CARGO_${arg//./_}
}

# Build a native library for the given platform.
# Assume source code resides at SRC_DIR
PLATFORMABI=$1
LEVEL=$2
SRCDIR=$3

set -e

export PKG_CONFIG_ALLOW_CROSS=1

echo "Building ${SRCDIR} for ${PLATFORMABI} on level ${LEVEL}"
PLATFORM=$(cut -d'-' -f1 <<<"${PLATFORMABI}")

# Directory mappings for build abi vs expected directory structure for jniLibs.
PLATFORM_OUTDIR=""
if [ "${PLATFORM}" == "i686" ]; then
    PLATFORM_OUTDIR="x86"
elif [ "${PLATFORM}" == "x86_64" ]; then
    PLATFORM_OUTDIR="x86_64"
elif [ "${PLATFORM}" == "armv7" ]; then
    PLATFORM_OUTDIR="armeabi-v7a"
elif [ "${PLATFORM}" == "aarch64" ]; then
    PLATFORM_OUTDIR="arm64-v8a"
else
    PLATFORM_OUTDIR=${PLATFORM}
fi

# Configure C build environment to use the tools in the NDK
# When configuring dependencies these variables will be used by Make
# Additionally CC, AR and the library paths of the dependencies get passed to rust

PLATFORMABI_TOOLCHAIN=${PLATFORMABI}
PLATFORMABI_COMPILER=${PLATFORMABI}

# Handle the special case
if [ "${PLATFORMABI}" == "armv7-linux-androideabi" ]; then
    PLATFORMABI_TOOLCHAIN="arm-linux-androideabi"
    PLATFORMABI_COMPILER="armv7a-linux-androideabi"
fi

# set toolchain path
export TOOLCHAIN=${NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/${PLATFORMABI_TOOLCHAIN}
echo "Toolchain path: ${TOOLCHAIN}"

# set the archiver
export AR=${NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/${PLATFORMABI_TOOLCHAIN}$'-'ar
echo "Archiver: ${AR}"

# set the assembler
export AS=${NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/${PLATFORMABI_TOOLCHAIN}$'-'as
echo "Assembler: ${AS}"

# set the c and c++ compiler
CC=${NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/${PLATFORMABI_COMPILER}
export CC=${CC}${LEVEL}$'-'clang
export CXX=${CC}++
echo "C Compiler: ${CC}"
echo "CXX Compiler: ${CXX}"

# set the linker
export LD=${NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/${PLATFORMABI_TOOLCHAIN}$'-'ld
echo "Linker ${LD}"

# set the archive index generator tool
export RANLIB=${NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/${PLATFORMABI_TOOLCHAIN}$'-'ranlib
echo "Archive Indexer: ${RANLIB}"

# set the symbol stripping tool
export STRIP=${NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/${PLATFORMABI_TOOLCHAIN}$'-'strip
echo "Symbol Stripper: ${STRIP}"

echo ""
export CXXFLAGS="-stdlib=libstdc++ -isystem ${NDK_HOME}/sources/cxx-stl/llvm-libc++/include"
echo "CXX Flags: ${CXXFLAGS}"

export CFLAGS="${CFLAGS//PF/$PLATFORMABI} -I${NDK_HOME}/sources/cxx-stl/llvm-libc++/include -I${NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/include -I${NDK_HOME}/sysroot/usr/include/${PLATFORMABI}"
echo "CFLAGS: ${CFLAGS}"

export LDFLAGS="-L${NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/${PLATFORMABI_TOOLCHAIN}/${LEVEL} ${LDFLAGS//PF/$PLATFORMABI}"
echo "LDFLAGS: ${LDFLAGS}"

export CPPFLAGS="${CPPFLAGS//PF/$PLATFORMABI}"
echo "CPPFLAGS: ${CPPFLAGS}"

export RUSTFLAGS="${RUSTFLAGS//PF/$PLATFORMABI}"
echo "RUSTFLAGS: ${RUSTFLAGS}"

mkdir -p build
argify "build.target.dir"
export "$arg"="build/"
echo "$arg"="build/"

argify "build.target"
export "$arg"="${PLATFORMABI}"
echo "$arg"="${PLATFORMABI}"
argify "target.${PLATFORMABI}.ar"
export "$arg"="${AR}"
echo "$arg"="${AR}"
argify "target.${PLATFORMABI}.linker"
export "$arg"="${CC}"
echo "$arg"="${CC}"
argify "target.${PLATFORMABI}.rustflags"
export "${arg}"="${RUSTFLAGS}"
echo "${arg}"="${RUSTFLAGS}"

echo "Cargo Flags: ${CARGO_FLAGS}"
echo "Cargo HTTP multiplexing: ${CARGO_HTTP_MULTIPLEXING}"

# Ensure target is installed in the event rust updated and invalidated it
echo "rustup target add"
rustup target add x86_64-linux-android aarch64-linux-android armv7-linux-androideabi

# Build rust build!
echo "cargo build"
cargo build --package tari_wallet_ffi --lib --release
