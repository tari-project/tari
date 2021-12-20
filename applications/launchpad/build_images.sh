#!/bin/bash
source versions.txt

build_image() {
  echo "Building $1 image v$VERSION.."
  docker build -f docker_rig/$1 --build-arg ARCH=native --build-arg FEATURES=avx2 --build-arg VERSION=$VERSION $3 $4 -t quay.io/tarilabs/$2:latest ./../..
  docker tag quay.io/tarilabs/$2:latest quay.io/tarilabs/$2:$VERSION
  docker push quay.io/tarilabs/$2:latest
  docker push quay.io/tarilabs/$2:$VERSION
}

build_image base_node.Dockerfile tari_base_node
build_image console_wallet.Dockerfile tari_console_wallet
build_image mm_proxy.Dockerfile tari_mm_proxy
build_image sha3_miner.Dockerfile tari_sha3_miner
build_image tor.Dockerfile tor
build_image monerod.Dockerfile monerod

echo "Building XMRig image v$VERSION (XMRig v$XMRIG_VERSION)"
docker build -f docker_rig/xmrig.Dockerfile --build-arg VERSION=$VERSION --build-arg XMRIG_VERSION=$XMRIG_VERSION -t quay.io/tarilabs/xmrig:latest ./../..
docker tag quay.io/tarilabs/xmrig:latest quay.io/tarilabs/xmrig:$VERSION
docker push quay.io/tarilabs/xmrig:latest
docker push quay.io/tarilabs/xmrig:$VERSION