#!/bin/bash
source build.config
CURRENT_DIR=$TARI_REPO_PATH/base_layer/wallet_ffi
ZMQ_REPO=https://github.com/zeromq/libzmq.git
ZMQ_FOLDER=libzmq
cd ..
cd ..
cd ..
DEPENDENCIES=$ANDROID_WALLET_PATH
export PKG_CONFIG_ALLOW_CROSS=1
if [ ! -z "$DEPENDENCIES" ] && [ ! -z "$NDK_PATH" ] && [ ! -z "$PKG_PATH" ] && [ "$BUILD_ANDROID" -eq 1 ]; then
  DEPENDENCIES=$DEPENDENCIES/jniLibs
  cd $DEPENDENCIES
  mkdir -p build
  cd build
  BUILD_ROOT=$PWD
  cd ..
  if [ ! -d $ZMQ_FOLDER ]; then
    git clone $ZMQ_REPO
    cd $ZMQ_FOLDER
  else
    cd $ZMQ_FOLDER
    git pull
  fi

  export NDK_HOME=$NDK_PATH
  export PKG_CONFIG_PATH=$PKG_PATH
  export NDK_TOOLCHAIN_VERSION=clang

  for PLATFORMABI in "armv7-linux-androideabi"
  #"i686-linux-android" "aarch64-linux-android" "x86_64-linux-android" not included at present
  do
    for LEVEL in 24
    #21 22 23 26 26 27 28 29 not included at present
    do
      PLATFORM=$(cut -d'-' -f1 <<<"$PLATFORMABI")

      PLATFORM_OUTDIR=""
      if [ "$PLATFORM" == "i686" ]; then
        PLATFORM_OUTDIR="x86"
        elif [ "$PLATFORM" == "x86_64" ]; then
          PLATFORM_OUTDIR="x86_64"
        elif [ "$PLATFORM" == "armv7" ]; then
          PLATFORM_OUTDIR="armeabi-v7a"
        elif [ "$PLATFORM" == "aarch64" ]; then
          PLATFORM_OUTDIR="arm64-v8"
        else
          PLATFORM_OUTDIR=$PLATFORM
      fi
      cd $BUILD_ROOT
      mkdir -p $PLATFORM_OUTDIR
      OUTPUT_DIR=$BUILD_ROOT/$PLATFORM_OUTDIR
      echo $OUTPUT_DIR
      cd $DEPENDENCIES
      cd $ZMQ_FOLDER

      PLATFORMABI_TOOLCHAIN=$PLATFORMABI
      PLATFORMABI_COMPILER=$PLATFORMABI
      if [ "$PLATFORMABI" == "armv7-linux-androideabi" ]; then
        PLATFORMABI_TOOLCHAIN="arm-linux-androideabi"
        PLATFORMABI_COMPILER="armv7a-linux-androideabi"
      fi

      # set toolchain path
      export TOOLCHAIN=$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/$PLATFORMABI_TOOLCHAIN

      # set the archiver
      export AR=$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/${PLATFORMABI_TOOLCHAIN}\-ar

      # set the assembler
      export AS=$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/${PLATFORMABI_TOOLCHAIN}\-as

      # set the c and c++ compiler
      CC=$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/$PLATFORMABI_COMPILER
      export CC=${CC}${LEVEL}\-clang
      export CXX=${CC}++

      # set c++ (pre)compilation flags
      export CXXFLAGS="-stdlib=libstdc++ -isystem $NDK_HOME/sources/cxx-stl/llvm-libc++/include"

      # set the linker
      export LD=$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/${PLATFORMABI_TOOLCHAIN}\-ld

      # set linker flags
      export LDFLAGS="-L$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/sysroot/usr/lib/$PLATFORMABI_TOOLCHAIN/$LEVEL -L$OUTPUT_DIR/lib -lc++"

      # set the archive index generator tool
      export RANLIB=$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/${PLATFORMABI_TOOLCHAIN}\-ranlib

      # set the symbol stripping tool
      export STRIP=$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/${PLATFORMABI_TOOLCHAIN}\-strip

      # set c flags
      export CFLAGS=-DMDB_USE_ROBUST=0

      # set cpp flags
      export CPPFLAGS="-fPIC -I$OUTPUT_DIR/include"

      make clean
      ./autogen.sh
      ./configure --enable-static --disable-shared --host=$PLATFORMABI --prefix=$OUTPUT_DIR
      make install

      cd $CURRENT_DIR
      mkdir -p .cargo
      cd .cargo
      cat > config <<EOF
[target.${PLATFORMABI}]
ar = "${AR}"
linker = "${CC}"

[target.${PLATFORMABI}.zmq]
rustc-flags = "-L${OUTPUT_DIR}"
EOF
      cd ..
      cargo ndk --target $PLATFORMABI --android-platform $LEVEL -- build --release
      HEADERFILE=$DEPENDENCIES/wallet.h
      if [ ! -f "$HEADERFILE" ]; then
        cp wallet.h $DEPENDENCIES/
      fi
      rm -rf .cargo
      cd ..
      cd ..
      cd target
      cd $PLATFORMABI
      cd release
      cp libwallet_ffi.a $OUTPUT_DIR
      cd ..
      cd ..
      rm -rf target
      cd $DEPENDENCIES
      mkdir -p $PLATFORM_OUTDIR
      cd $PLATFORM_OUTDIR
      cp ${OUTPUT_DIR}/libwallet_ffi.a $PWD
      cp ${OUTPUT_DIR}/lib/libzmq.a $PWD
    done
  done
  cd $DEPENDENCIES
  rm -rf build
  rm -rf $ZMQ_FOLDER
else
  echo "Cannot configure Android Wallet Library build"
fi

unameOut="$(uname -s)"
case "${unameOut}" in
    Linux*)     MACHINE=Linux;;
    Darwin*)    MACHINE=Mac;;
    CYGWIN*)    MACHINE=Cygwin;;
    MINGW*)     MACHINE=MinGw;;
    *)          MACHINE="UNKNOWN:${unameOut}"
esac

DEPENDENCIES=$IOS_WALLET_PATH
if [ ! -z "$DEPENDENCIES" ] && [ ! -z "$PKG_PATH" ] && [ "$BUILD_IOS" -eq 1 ] && [ "$MACHINE" == "Mac" ]; then
  #below line is temporary
  ZMQ_REPO="https://github.com/azawawi/libzmq-ios"
  cd $DEPENDENCIES
  mkdir -p build
  cd build
  BUILD_ROOT=$PWD
  cd ..
  if [ ! -d "${ZMQ_FOLDER}-ios" ]; then
    git clone $ZMQ_REPO
    cd ${ZMQ_FOLDER}-ios
  else
    cd ${ZMQ_FOLDER}-ios
    git pull
  fi
  ruby libzmq.rb
  cp "${PWD}/dist/ios/lib/libzmq.a" "${DEPENDENCIES}/MobileWallet/TariLib/"
  cd ${CURRENT_DIR}
  cp wallet.h "${DEPENDENCIES}/MobileWallet/TariLib/"
  export PKG_CONFIG_PATH=${PKG_PATH}
  cargo-lipo lipo --release
  cd ..
  cd ..
  cd target
  cd universal
  cd release
  cp libwallet_ffi.a "${DEPENDENCIES}/MobileWallet/TariLib/"
  cd ..
  cd ..
  cd ..
  rm -rf target
  cd ${DEPENDENCIES}
  rm -rf ${ZMQ_FOLDER}-ios
else
  echo "Cannot configure iOS Wallet Library build"
fi