#!/bin/bash
PLATFORMS=${1}
VERSION=${2}
SRC_DIR=${3}
DATE=$(date +%Y-%m-%d)

function get_arch() {
    p=$(cut -d'-' -f1 <<<"${1}")
    if [ "${p}" == "i686" ]; then
        ARCH="x86"
    elif [ "${p}" == "x86_64" ]; then
        ARCH="x86_64"
    elif [ "${p}" == "armv7" ]; then
        ARCH="armeabi-v7a"
    elif [ "${p}" == "aarch64" ]; then
        ARCH="arm64-v8a"
    else
        ARCH=${p}
    fi
}

export OUTDIR=/github/workspace/libwallet
mkdir -p $OUTDIR
mkdir -p /tmp/output
IFS=';' read -ra arch_arr <<<"$PLATFORMS"

# Create the hash file
hashfile=${OUTDIR}/libwallet-hashes-${VERSION}.txt
echo "# Mobile libraries for Tari libwallet version ${VERSION}. ${DATE}" >"${hashfile}"
# Copy wallet.h to staging area
cp "${SRC_DIR}/base_layer/wallet_ffi/wallet.h" /tmp/output
cd /tmp/output || exit
sha256sum wallet.h >>"${hashfile}"
for i in "${arch_arr[@]}"; do
    PLATFORM_ABI=${i}
    get_arch "${i}"
    # Tarball the required libraries
    filename=libwallet-${ARCH}-${VERSION}.tar.gz
    echo "Packaging ${i} to ${filename}"
    # Copy newly compiled libraries to staging area
    mkdir -p "/tmp/output/${ARCH}/"
    cp "/platforms/sqlite/${i}/lib/libsqlite3.a" "/tmp/output/${ARCH}/"
    cp "/platforms/ssl/${i}/usr/local/lib/libcrypto.a" "/tmp/output/${ARCH}/"
    cp "/platforms/ssl/${i}/usr/local/lib/libssl.a" "/tmp/output/${ARCH}/"
    cp "/build/${PLATFORM_ABI}/release/libtari_wallet_ffi.a" "/tmp/output/${ARCH}/"
    tar -czf "${OUTDIR}/${filename}" -C "/tmp/output/" wallet.h "$ARCH"
    echo sha256sum "./${ARCH}/* -> ${hashfile}"
    sha256sum ./"${ARCH}"/* >>"${hashfile}"
done

echo "Done"
