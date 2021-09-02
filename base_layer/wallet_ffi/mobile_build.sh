#!/bin/bash
#
# Script to build libraries for Tari Wallet
#

#Terminal colors
RED='\033[0;31m'
GREEN='\033[0;32m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

source build.config
TARI_REPO_PATH=${TARI_REPO_PATH:-$(git rev-parse --show-toplevel)}
CURRENT_DIR=${TARI_REPO_PATH}/base_layer/wallet_ffi
cd "${CURRENT_DIR}" || exit
mkdir -p logs
cd logs || exit
mkdir -p ios
mkdir -p android
cd ../..
IOS_LOG_PATH=${CURRENT_DIR}/logs/ios
ANDROID_LOG_PATH=${CURRENT_DIR}/logs/android
SQLITE_FOLDER=sqlite
SSL_FOLDER=ssl
cd ../../..

unameOut="$(uname -s)"
case "${unameOut}" in
    Linux*)     MACHINE=Linux;;
    Darwin*)    MACHINE=Mac;;
    CYGWIN*)    MACHINE=Cygwin;;
    MINGW*)     MACHINE=MinGw;;
    *)          MACHINE="UNKNOWN:${unameOut}"
esac
export PKG_CONFIG_ALLOW_CROSS=1

# Fix for macOS Catalina failing to include correct headers for cross compilation
if [ "${MACHINE}" == "Mac" ]; then
  MAC_VERSION=$(sw_vers -productVersion)
  MAC_MAIN_VERSION=$(cut -d '.' -f1 <<<"$(sw_vers -productVersion)")
  MAC_SUB_VERSION=$(cut -d '.' -f2 <<<"$(sw_vers -productVersion)")
  echo "${PURPLE}Mac version as reported by OS: ${MAC_VERSION}"
  if [ "${MAC_MAIN_VERSION}" -le 10 ]; then
    if [ "${MAC_SUB_VERSION}" -ge 15 ]; then
      unset CPATH
      echo "${PURPLE}macOS 10.15 Detected${NC}"
    else
      echo "${PURPLE}macOS 10.14- Detected${NC}"
    fi
  else
    unset CPATH
    echo "${PURPLE}macOS 11+ Detected${NC}"
  fi
fi

DEPENDENCIES=${IOS_WALLET_PATH}
# PKG_PATH, BUILD_IOS is defined in build.config
if [ -n "${DEPENDENCIES}" ] && [ -n "${PKG_PATH}" ] && [ "${BUILD_IOS}" -eq 1 ] && [ "${MACHINE}" == "Mac" ]; then
  echo "${GREEN}Commencing iOS build${NC}"
  echo "${YELLOW}Build logs can be found at ${IOS_LOG_PATH}${NC}"
  # shellcheck disable=SC2028
  echo "\t${CYAN}Configuring Rust${NC}"
  rustup target add aarch64-apple-ios x86_64-apple-ios >> "${IOS_LOG_PATH}/rust.txt" 2>&1
  cargo install cargo-lipo >> "${IOS_LOG_PATH}/rust.txt" 2>&1
  # shellcheck disable=SC2028
  echo "\t${CYAN}Configuring complete${NC}"
  cd "${DEPENDENCIES}" || exit
  mkdir -p build
  cd build || exit
  BUILD_ROOT=$PWD
  cd ..
  cd "${CURRENT_DIR}" || exit
  if [ "${CARGO_CLEAN}" -eq "1" ]; then
      cargo clean >> "${IOS_LOG_PATH}/cargo.txt" 2>&1
  fi
  cp wallet.h "${DEPENDENCIES}/MobileWallet/TariLib/"
  export PKG_CONFIG_PATH=${PKG_PATH}
  # shellcheck disable=SC2028
  echo "\t${CYAN}Building Wallet FFI${NC}"
  cargo-lipo lipo --release > "${IOS_LOG_PATH}/cargo.txt" 2>&1
  cd ../..
  cd target || exit
  # Copy the fat library (which contains symbols for all built iOS architectures) created by the lipo tool
  # XCode will select the relevant set of symbols to be included in the mobile application depending on which arch is built
  cd universal || exit
  cd release || exit
  cp libtari_wallet_ffi.a "${DEPENDENCIES}/MobileWallet/TariLib/"
  cd ../../.. || exit
  rm -rf target
  cd "${DEPENDENCIES}" || exit
  echo "${GREEN}iOS build completed${NC}"
elif [ "${BUILD_IOS}" -eq 1 ]; then
  echo "${RED}Cannot configure iOS Wallet Library build${NC}"
else
  echo "${GREEN}iOS Wallet is configured not to build${NC}"
fi

DEPENDENCIES=$ANDROID_WALLET_PATH
# PKG_PATH, BUILD_ANDROID, NDK_PATH is defined in build.config
if [ -n "${DEPENDENCIES}" ] && [ -n "${NDK_PATH}" ] && [ -n "${PKG_PATH}" ] && [ "${BUILD_ANDROID}" -eq 1 ]; then
  echo "${GREEN}Commencing Android build${NC}"
  echo "${YELLOW}Build logs can be found at ${ANDROID_LOG_PATH}${NC}"
  # shellcheck disable=SC2028
  echo "\t${CYAN}Configuring Rust${NC}"
  rustup target add x86_64-linux-android aarch64-linux-android armv7-linux-androideabi i686-linux-android arm-linux-androideabi > "${ANDROID_LOG_PATH}/rust.txt" 2>&1
  if [ "${MAC_MAIN_VERSION}" -le 10 ]; then
    if [ "${MAC_SUB_VERSION}" -lt 15 ]; then
      cargo install cargo-ndk > "${ANDROID_LOG_PATH}/rust.txt" 2>&1
    fi
  fi
  # shellcheck disable=SC2028
  echo "\t${CYAN}Configuring complete${NC}"
  export NDK_HOME=${NDK_PATH}
  export PKG_CONFIG_PATH=${PKG_PATH}
  export NDK_TOOLCHAIN_VERSION=clang
  DEPENDENCIES=${DEPENDENCIES}/jniLibs

  SQLITE_BUILD_FOUND=0
  if [ -f "${DEPENDENCIES}/x86_64/libsqlite3.a" ] && [ -f "${DEPENDENCIES}/armeabi-v7a/libsqlite3.a" ] && [ -f "${DEPENDENCIES}/arm64-v8a/libsqlite3.a" ]; then
    SQLITE_BUILD_FOUND=1
  fi

  SSL_BUILD_FOUND=0
  if [ -f "${DEPENDENCIES}/x86_64/libssl.a" ] && [ -f "${DEPENDENCIES}/x86_64/libcrypto.a" ] && \
     [ -f "${DEPENDENCIES}/armeabi-v7a/libssl.a" ] && [ -f "${DEPENDENCIES}/armeabi-v7a/libcrypto.a" ] && \
     [ -f "${DEPENDENCIES}/arm64-v8a/libssl.a" ] && [ -f "${DEPENDENCIES}/arm64-v8a/libcrypto.a" ]; then
    SSL_BUILD_FOUND=1
  fi

  cd "${DEPENDENCIES}" || exit
  mkdir -p build
  cd build || exit
  BUILD_ROOT=${PWD}
  if [ "${MACHINE}" == "Mac" ]; then
    if [ "${MAC_MAIN_VERSION}" -le 10 ]; then
      if [ "${MAC_SUB_VERSION}" -ge 15 ]; then
        cd "${NDK_HOME}/sources/cxx-stl/llvm-libc++/include" || exit
        mkdir -p sys
        #Fix for missing header, c code should reference limits.h instead of syslimits.h, happens with code that has been around for a long time.
        cp "${NDK_HOME}/sources/cxx-stl/llvm-libc++/include/limits.h" "${NDK_HOME}/sources/cxx-stl/llvm-libc++/include/sys/syslimits.h"
        cd "${BUILD_ROOT}" || exit
      fi
      else
        cd "${NDK_HOME}/sources/cxx-stl/llvm-libc++/include" || exit
        mkdir -p sys
        cp "${NDK_HOME}/sources/cxx-stl/llvm-libc++/include/limits.h" "${NDK_HOME}/sources/cxx-stl/llvm-libc++/include/sys/syslimits.h"
        cd "${BUILD_ROOT}" || exit
    fi
  fi
  cd ..

  for PLATFORMABI in "x86_64-linux-android" "aarch64-linux-android" "armv7-linux-androideabi"
  do
    # Lint warning for loop only running once is acceptable here
    # shellcheck disable=SC2043
    for LEVEL in 24
    #21 22 23 26 26 27 28 29 not included at present
    do
      if [ ${SSL_BUILD_FOUND} -eq 0 ]; then
        touch "${ANDROID_LOG_PATH}/ssl_${PLATFORMABI}_${LEVEL}.txt"
      fi

      if [ ${SQLITE_BUILD_FOUND} -eq 0 ]; then
        touch "${ANDROID_LOG_PATH}/sqlite_${PLATFORMABI}_${LEVEL}.txt"
      fi

      touch "${ANDROID_LOG_PATH}/cargo_${PLATFORMABI}_${LEVEL}.txt"

      PLATFORM=$(cut -d'-' -f1 <<<"${PLATFORMABI}")

      # Below "null" is to prevent it exiting with mismatched '"' once it reaches the end of the script
      PLATFORM_OUTDIR="null"
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
      cd "${BUILD_ROOT}" || exit
      mkdir -p "${PLATFORM_OUTDIR}"
      OUTPUT_DIR="${BUILD_ROOT}/${PLATFORM_OUTDIR}"

      if [ ${SQLITE_BUILD_FOUND} -eq 1 ]; then
        mkdir -p "${BUILD_ROOT}/${PLATFORM_OUTDIR}/lib"
      fi

      if [ ${SSL_BUILD_FOUND} -eq 1 ]; then
        mkdir -p "${BUILD_ROOT}/${PLATFORM_OUTDIR}/usr/local/lib"
      fi

      cd "${DEPENDENCIES}" || exit

      PLATFORMABI_TOOLCHAIN=${PLATFORMABI}
      PLATFORMABI_COMPILER=${PLATFORMABI}
      if [ "${PLATFORMABI}" == "armv7-linux-androideabi" ]; then
        PLATFORMABI_TOOLCHAIN="arm-linux-androideabi"
        PLATFORMABI_COMPILER="armv7a-linux-androideabi"
      fi
      # set toolchain path
      TOOLCHAIN_PATH="${NDK_HOME}/toolchains/llvm/prebuilt/darwin-x86_64/"
      export TOOLCHAIN="${TOOLCHAIN_PATH}${PLATFORMABI_TOOLCHAIN}"

      # undo compiler configuration (if set) of previous iteration for ssl scripts
      unset AR;
      unset AS;
      unset CC;
      unset CXX;
      unset CXXFLAGS;
      unset LD;
      unset LDFLAGS;
      unset RANLIB;
      unset STRIP;
      unset CFLAGS;
      unset CXXFLAGS;

      mkdir -p ${SSL_FOLDER}
      cd ${SSL_FOLDER} || exit
      if [ ${SSL_BUILD_FOUND} -eq 0 ]; then
        # shellcheck disable=SC2028
        echo "\t${CYAN}Fetching SSL source${NC}"
        OPENSSL_SOURCE="https://www.openssl.org/source/openssl-${OPENSSL_VERSION}.tar.gz"
        curl -s "${OPENSSL_SOURCE}" | tar -xvf - -C "${PWD}" >> "${ANDROID_LOG_PATH}/ssl_${PLATFORMABI}_${LEVEL}.txt" 2>&1 || exit
        # shellcheck disable=SC2028
        echo "\t${CYAN}Source fetched${NC}"
        cd "openssl-${OPENSSL_VERSION}" || exit
        # shellcheck disable=SC2028
        echo "\t${CYAN}Building SSL${NC}"
        # Required by openssl-build-script
        ANDROID_NDK=${NDK_PATH}
        export ANDROID_NDK
        case ${PLATFORM} in
        armv7)
          SSL_TARGET="android-arm"
          SSL_OPTIONS="--target=${PLATFORMABI_COMPILER} -Wl,--fix-cortex-a8 -fPIC -no-zlib -no-hw -no-engine -no-shared -D__ANDROID_API__=${LEVEL}"
          ;;
        x86_64)
          SSL_TARGET="android-x86_64"
          SSL_OPTIONS="-fPIC -no-zlib -no-hw -no-engine -no-shared -D__ANDROID_API__=${LEVEL}"
          ;;
        aarch64)
          SSL_TARGET="android-arm64"
          SSL_OPTIONS="-fPIC -no-zlib -no-hw -no-engine -no-shared -D__ANDROID_API__=${LEVEL}"
          ;;
        esac
        # Required by openssl-build-script
        export PATH="${TOOLCHAIN}/bin:${TOOLCHAIN_PATH}/bin:${PATH}"
        make clean > "${ANDROID_LOG_PATH}/ssl_${PLATFORMABI}_${LEVEL}.txt" 2>&1
        ./Configure "${SSL_TARGET}" "${SSL_OPTIONS}" > "${ANDROID_LOG_PATH}/ssl_${PLATFORMABI}_${LEVEL}.txt" 2>&1
        make >> "${ANDROID_LOG_PATH}/ssl_${PLATFORMABI}_${LEVEL}.txt" 2>&1
        make install DESTDIR="${OUTPUT_DIR}" >> "${ANDROID_LOG_PATH}/ssl_${PLATFORMABI}_${LEVEL}.txt" 2>&1
        # shellcheck disable=SC2028
        echo "\t${CYAN}SSL built${NC}"
      else
        # shellcheck disable=SC2028
        echo "\t${CYAN}SSL located${NC}"
      fi
      cd ../..

      # set the archiver
      export AR=${NDK_HOME}/toolchains/llvm/prebuilt/darwin-x86_64/bin/${PLATFORMABI_TOOLCHAIN}$'-'ar

      # set the assembler
      export AS=${NDK_HOME}/toolchains/llvm/prebuilt/darwin-x86_64/bin/${PLATFORMABI_TOOLCHAIN}$'-'as

      # set the c and c++ compiler
      CC=${NDK_HOME}/toolchains/llvm/prebuilt/darwin-x86_64/bin/${PLATFORMABI_COMPILER}
      export CC=${CC}${LEVEL}$'-'clang
      export CXX=${CC}++

      export CXXFLAGS="-stdlib=libstdc++ -isystem ${NDK_HOME}/sources/cxx-stl/llvm-libc++/include"
      # set the linker
      export LD=${NDK_HOME}/toolchains/llvm/prebuilt/darwin-x86_64/bin/${PLATFORMABI_TOOLCHAIN}$'-'ld

      # set linker flags
      export LDFLAGS="-L${NDK_HOME}/toolchains/llvm/prebuilt/darwin-x86_64/sysroot/usr/lib/${PLATFORMABI_TOOLCHAIN}/${LEVEL} -L${OUTPUT_DIR}/lib -lc++"

      # set the archive index generator tool
      export RANLIB=${NDK_HOME}/toolchains/llvm/prebuilt/darwin-x86_64/bin/${PLATFORMABI_TOOLCHAIN}$'-'ranlib

      # set the symbol stripping tool
      export STRIP=${NDK_HOME}/toolchains/llvm/prebuilt/darwin-x86_64/bin/${PLATFORMABI_TOOLCHAIN}$'-'strip

      # set c flags
      #note: Add -v to below to see compiler output, include paths, etc
      export CFLAGS="-DMDB_USE_ROBUST=0"

      # set cpp flags
      export CPPFLAGS="-fPIC -I${OUTPUT_DIR}/include"

      mkdir -p ${SQLITE_FOLDER}
      cd ${SQLITE_FOLDER} || exit
      if [ ${SQLITE_BUILD_FOUND} -eq 0 ]; then
        # shellcheck disable=SC2028
        echo "\t${CYAN}Fetching Sqlite3 source${NC}"
        curl -s "${SQLITE_SOURCE}" | tar -xvf - -C "${PWD}" >> "${ANDROID_LOG_PATH}/sqlite_${PLATFORMABI}_${LEVEL}.txt" 2>&1
        # shellcheck disable=SC2028
        echo "\t${CYAN}Source fetched${NC}"
        cd "$(find . -type d -maxdepth 1 -print | grep -m1 'sqlite')" || exit
        # shellcheck disable=SC2028
        echo "\t${CYAN}Building Sqlite3${NC}"
        make clean > "${ANDROID_LOG_PATH}/sqlite_${PLATFORMABI}_${LEVEL}.txt" 2>&1
        ./configure --host=${PLATFORMABI} --prefix="${OUTPUT_DIR}" > "${ANDROID_LOG_PATH}/sqlite_${PLATFORMABI}_${LEVEL}.txt" 2>&1
        make install > "${ANDROID_LOG_PATH}/sqlite_${PLATFORMABI}_${LEVEL}.txt" 2>&1
        # shellcheck disable=SC2028
        echo "\t${CYAN}Sqlite3 built${NC}"
      else
        # shellcheck disable=SC2028
        echo "\t${CYAN}Sqlite3 located${NC}"
      fi
      cd ../..

      if [ "${MACHINE}" == "Mac" ]; then
        if [ "${MAC_MAIN_VERSION}" -le 10 ]; then
          if [ "${MAC_SUB_VERSION}" -ge 15 ]; then
            # Not ideal, however necesary for cargo to pass additional flags
            export CFLAGS="${CFLAGS} -I${NDK_HOME}/sources/cxx-stl/llvm-libc++/include -I${NDK_HOME}/toolchains/llvm/prebuilt/darwin-x86_64/sysroot/usr/include -I${NDK_HOME}/sysroot/usr/include/${PLATFORMABI}"
          fi
        else
            export CFLAGS="${CFLAGS} -I${NDK_HOME}/sources/cxx-stl/llvm-libc++/include -I${NDK_HOME}/toolchains/llvm/prebuilt/darwin-x86_64/sysroot/usr/include -I${NDK_HOME}/sysroot/usr/include/${PLATFORMABI}"
        fi
      fi
      export LDFLAGS="-L${NDK_HOME}/toolchains/llvm/prebuilt/darwin-x86_64/sysroot/usr/lib/${PLATFORMABI_TOOLCHAIN}/${LEVEL} -L${OUTPUT_DIR}/lib -L${OUTPUT_DIR}/usr/local/lib -lc++ -lsqlite3 -lcrypto -lssl"
      cd "${OUTPUT_DIR}/lib" || exit

      if [ ${SQLITE_BUILD_FOUND} -eq 1 ]; then
       cp "${DEPENDENCIES}/${PLATFORM_OUTDIR}/libsqlite3.a" "${OUTPUT_DIR}/lib/libsqlite3.a"
      fi

      if [ ${SSL_BUILD_FOUND} -eq 1 ]; then
       cp "${DEPENDENCIES}/${PLATFORM_OUTDIR}/libcrypto.a" "${OUTPUT_DIR}/usr/local/lib/libcrypto.a"
       cp "${DEPENDENCIES}/${PLATFORM_OUTDIR}/libssl.a" "${OUTPUT_DIR}/usr/local/lib/libssl.a"
      fi

      # shellcheck disable=SC2028
      echo "\t${CYAN}Configuring Cargo${NC}"
      cd "${CURRENT_DIR}" || exit
      if [ "${CARGO_CLEAN}" -eq "1" ]; then
        cargo clean > "${ANDROID_LOG_PATH}/cargo_${PLATFORMABI}_${LEVEL}.txt" 2>&1
      fi
      mkdir -p .cargo
      cd .cargo || exit
      if [ "${MACHINE}" == "Mac" ]; then
        if [ "${MAC_MAIN_VERSION}" -le 10 ]; then
          if [ "${MAC_SUB_VERSION}" -ge 15 ]; then
cat > config <<EOF
[build]
target = "${PLATFORMABI}"

[target.${PLATFORMABI}]
ar = "${AR}"
linker = "${CC}"
rustflags = "-L${OUTPUT_DIR}/lib -L${OUTPUT_DIR}/usr/local/lib"

EOF
        else
cat > config <<EOF
[target.${PLATFORMABI}]
ar = "${AR}"
linker = "${CC}"
rustflags = "-L${OUTPUT_DIR}/lib -L${OUTPUT_DIR}/usr/local/lib"

EOF

        fi
        else
cat > config <<EOF
[build]
target = "${PLATFORMABI}"

[target.${PLATFORMABI}]
ar = "${AR}"
linker = "${CC}"
rustflags = "-L${OUTPUT_DIR}/lib -L${OUTPUT_DIR}/usr/local/lib"

EOF

          fi
      fi
      # shellcheck disable=SC2028
      echo "\t${CYAN}Configuring complete${NC}"
      cd .. || exit
      # shellcheck disable=SC2028
      echo "\t${CYAN}Building Wallet FFI${NC}"
      #note: add -vv to below to see verbose and build script output
      if [ "${MACHINE}" == "Mac" ]; then
        if [ "${MAC_MAIN_VERSION}" -le 10 ]; then
          if [ "${MAC_SUB_VERSION}" -ge 15 ]; then
            cargo build --lib --release > "${ANDROID_LOG_PATH}/cargo_${PLATFORMABI}_${LEVEL}.txt" 2>&1
          else
            cargo ndk --target ${PLATFORMABI} --android-platform ${LEVEL} -- build --release > "${ANDROID_LOG_PATH}/cargo_${PLATFORMABI}_${LEVEL}.txt" 2>&1
          fi
        else
          # Fix for lmdb-sys compilation for armv7 on Big Sur
          if [ "${PLATFORMABI}" == "armv7-linux-androideabi" ]; then
            # shellcheck disable=SC2028
            echo "\t${CYAN}Extracting supplementary header pack ${NC}"
            tar -xvf "${TARI_REPO_PATH}/base_layer/wallet_ffi/asm.tar.gz" -C "${NDK_PATH}/sources/cxx-stl/llvm-libc++/include"
            # shellcheck disable=SC2028
            echo "\t${CYAN}Extraction complete, continuing build ${NC}"
          fi
          cargo build --lib --release > "${ANDROID_LOG_PATH}/cargo_${PLATFORMABI}_${LEVEL}.txt" 2>&1
          if [ "${PLATFORMABI}" == "armv7-linux-androideabi" ]; then
            BACKTRACK=${PWD}
            # shellcheck disable=SC2028
            echo "\t${CYAN}Removing supplementary header pack ${NC}"
            cd "${NDK_PATH}/sources/cxx-stl/llvm-libc++/include" || exit
            rm -rf asm
            cd "${BACKTRACK}" || exit
          fi
        fi
      else
        cargo ndk --target ${PLATFORMABI} --android-platform ${LEVEL} -- build --release > "${ANDROID_LOG_PATH}/cargo_${PLATFORMABI}_${LEVEL}.txt" 2>&1
      fi
      cp wallet.h "${DEPENDENCIES}/"
      rm -rf .cargo
      cd ../..
      cd target || exit
      cd ${PLATFORMABI} || exit
      cd release || exit
      cp libtari_wallet_ffi.a "${OUTPUT_DIR}"
      cd ../..
      rm -rf target
      cd "${DEPENDENCIES}" || exit
      mkdir -p "${PLATFORM_OUTDIR}"
      cd "${PLATFORM_OUTDIR}" || exit
      if [ ${SQLITE_BUILD_FOUND} -eq 0 ]; then
        cp "${OUTPUT_DIR}/lib/libsqlite3.a" "${PWD}"
      fi
      if [ ${SSL_BUILD_FOUND} -eq 0 ]; then
        cp "${OUTPUT_DIR}/usr/local/lib/libcrypto.a" "${PWD}"
        cp "${OUTPUT_DIR}/usr/local/lib/libssl.a" "${PWD}"
      fi
      cp "${OUTPUT_DIR}/libtari_wallet_ffi.a" "${PWD}"
      # shellcheck disable=SC2028
      echo "\t${GREEN}Wallet library built for android architecture ${PLATFORM_OUTDIR} with minimum platform level support of ${LEVEL}${NC}"
    done
  done
  cd "${DEPENDENCIES}" || exit
  rm -rf build
  rm -rf ${SQLITE_FOLDER}
  rm -rf ${SSL_FOLDER}
  echo "${GREEN}Android build completed${NC}"
elif [ "${BUILD_ANDROID}" -eq 1 ]; then
  echo "${RED}Cannot configure Android Wallet Library build${NC}"
else
  echo "${GREEN}Android Wallet is configured not to build${NC}"
fi
