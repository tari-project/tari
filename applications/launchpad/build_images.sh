#!/bin/bash
#
# Used to build TariLabs docker images
# Defaults for building
#   Arch CPU: x86-64
#   Features: safe
#   Image ID: quay.io/tarilabs
#   Push Image Tag remote: enabled (1)
#   TL_VERSION: TL - Read from versions.txt
#   XMRIG_VERSION: XMRig version - read from versions.txt
#
# These can be overridden before running build_image.sh with shell exports
# ie:
#   export TBN_ARCH=native
#   export TBN_FEATURES=avx2
#   export TL_TAG_URL=local-tarilabs
#   export TL_TAG_PUSH=0
#

set -e

# If TL Version is not set, use versions.txt to build with?
if [ -z "${TL_VERSION}" ]; then
  source versions.txt
fi

# Version refers to the base_node, wallet, etc. version - set if they not set
# applications/tari_app_utilities/Cargo.toml
TL_VERSION=${TL_VERSION:-0.31.1}
# https://github.com/xmrig/xmrig/releases
XMRIG_VERSION=${XMRIG_VERSION:-v6.15.3}
# https://github.com/monero-project/monero/releases
MONERO_VERSION=${MONERO_VERSION:-0.17.2.3}
MONERO_SHA256=${MONERO_SHA256:-8069012ad5e7b35f79e35e6ca71c2424efc54b61f6f93238b182981ba83f2311}

#TBN_ARCH=native
TBN_ARCH=${TBN_ARCH:-x86-64}
#TBN_FEATURES=avx2
TBN_FEATURES=${TBN_FEATURES:-safe}

# Docker tag URL
TL_TAG_URL=${TL_TAG_URL:-quay.io/tarilabs}
# Disable docker push if 0
TL_TAG_PUSH=${TL_TAG_PUSH:-0}

build_image() {
  echo "Building $1 image v$TL_VERSION ..."
  docker build -f docker_rig/$1 --build-arg ARCH=${TBN_ARCH} \
    --build-arg FEATURES=${TBN_FEATURES} --build-arg VERSION=$TL_VERSION $3 $4 \
    -t ${TL_TAG_URL}/$2:"${TL_VERSION}-${TBN_ARCH}-${TBN_FEATURES}" ./../..
  if [[ "${TL_TAG_PUSH}" == "1" ]]; then
    docker push ${TL_TAG_URL}/$2:"${TL_VERSION}-${TBN_ARCH}-${TBN_FEATURES}"
  fi
}

build_image base_node.Dockerfile tari_base_node
build_image console_wallet.Dockerfile tari_console_wallet
build_image mm_proxy.Dockerfile tari_mm_proxy
build_image sha3_miner.Dockerfile tari_sha3_miner
build_image tor.Dockerfile tor

echo "Building Monerod image v$TL_VERSION (Monerod v$MONERO_VERSION) - $TL_VERSION-v$MONERO_VERSION"
docker build -f docker_rig/monerod.Dockerfile --build-arg VERSION="$TL_VERSION-v$MONERO_VERSION" \
  --build-arg MONERO_VERSION=$MONERO_VERSION --build-arg MONERO_SHA256=$MONERO_SHA256  \
  -t ${TL_TAG_URL}/monerod:"${TL_VERSION}-v$MONERO_VERSION-${TBN_ARCH}-${TBN_FEATURES}" ./../..
if [[ "${TL_TAG_PUSH}" == "1" ]]; then
  docker push ${TL_TAG_URL}/monerod:"${TL_VERSION}-v${MONERO_VERSION}-${TBN_ARCH}-${TBN_FEATURES}"
fi

echo "Building XMRig image v$TL_VERSION (XMRig v$XMRIG_VERSION) - $TL_VERSION-$XMRIG_VERSION"
docker build -f docker_rig/xmrig.Dockerfile --build-arg VERSION="$TL_VERSION-$XMRIG_VERSION" \
  --build-arg XMRIG_VERSION=$XMRIG_VERSION \
  -t ${TL_TAG_URL}/xmrig:"${TL_VERSION}-${XMRIG_VERSION}-${TBN_ARCH}-${TBN_FEATURES}" ./../..
if [[ "${TL_TAG_PUSH}" == "1" ]]; then
  docker push ${TL_TAG_URL}/xmrig:"${TL_VERSION}-${XMRIG_VERSION}-${TBN_ARCH}-${TBN_FEATURES}"
fi

docker build -f docker_rig/frontail.Dockerfile -t ${TL_TAG_URL}/frontail ./docker_rig
docker tag ${TL_TAG_URL}/frontail:"$TL_VERSION" ${TL_TAG_URL}/frontail:$TL_VERSION
if [[ "${TL_TAG_PUSH}" == "1" ]]; then
  docker push ${TL_TAG_URL}/frontail:"$TL_VERSION"
fi
