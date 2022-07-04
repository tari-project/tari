# syntax = docker/dockerfile:1.3

# Usage:
#  docker run --restart=always -v /var/data/blockchain-xmr:/root/.bitmonero \
#    -p 18080:18080 -p 18081:18081 -p 18089:18089 \
#    --name=monerod -td kannix/monero-full-node

# Binary build

# https://github.com/monero-project/monero/releases
ARG MONERO_VERSION=0.17.3.2

# Declare stage using linux/amd64 base image
FROM --platform=linux/amd64 bitnami/minideb:bullseye AS build-stage-amd64

# Platform Args
ARG MONERO_ARCH=x64
ARG MONERO_TAR=x86_64

# https://github.com/monero-project/monero/releases
ARG MONERO_AMD64_SHA256=1e54acd749265d9439d3733441c645d9b058316283c8f21cca2a333c1238cd16
ARG MONERO_VERSION

# Declare stage using linux/arm64 base image
FROM --platform=linux/arm64 bitnami/minideb:bullseye AS build-stage-arm64

# Platform Args
ARG MONERO_ARCH=armv8
ARG MONERO_TAR=aarch64

# https://github.com/monero-project/monero/releases
ARG MONERO_ARM64_SHA256=8e311714e97f2ac87bfd818abd5c4c605ca19ebda84a1edea93ec00a89d07e2e
ARG MONERO_VERSION

# Declare TARGETARCH to make it available
ARG TARGETARCH
# Select final stage based on TARGETARCH ARG
FROM build-stage-${TARGETARCH} as build-stage-download

ARG BUILDPLATFORM

# Bring over the Args from platform selection
ARG MONERO_ARCH
ARG MONERO_TAR

ARG MONERO_VERSION
ARG MONERO_SHA256

ENV MONERO_SHA256=${MONERO_SHA256:-$MONERO_AMD64_SHA256}
ENV MONERO_SHA256=${MONERO_SHA256:-$MONERO_ARM64_SHA256}

RUN apt-get update && apt-get install -y \
    curl \
    bzip2

WORKDIR /root

RUN curl https://dlsrc.getmonero.org/cli/monero-linux-$MONERO_ARCH-v$MONERO_VERSION.tar.bz2 -O && \
  echo "$MONERO_SHA256  monero-linux-$MONERO_ARCH-v$MONERO_VERSION.tar.bz2" | sha256sum -c - && \
  tar -xvf monero-linux-$MONERO_ARCH-v$MONERO_VERSION.tar.bz2 && \
  rm monero-linux-$MONERO_ARCH-v$MONERO_VERSION.tar.bz2 && \
  cp ./monero-$MONERO_TAR-linux-gnu-v$MONERO_VERSION/monerod . && \
  rm -r monero-*

FROM bitnami/minideb:bullseye AS runtime-stage

# Bring over the Args from platform selection
ARG BUILDPLATFORM

ARG MONERO_VERSION
ARG VERSION=1.0.1

RUN groupadd -g 1000 tari && \
    useradd -ms /bin/bash -u 1000 -g 1000 tari && \
    mkdir -p /home/tari/.bitmonero  && \
    chown -R tari:tari /home/tari/.bitmonero

USER tari
WORKDIR /home/tari

COPY --chown=tari:tari --from=build-stage-download /root/monerod /usr/local/bin/monerod

# blockchain location
VOLUME /home/tari/.bitmonero

ENV dockerfile_version=$VERSION
ENV dockerfile_build_arch=$BUILDPLATFORM
ENV monero_version=$MONERO_VERSION

# Expose p2p port
EXPOSE 18080

# Expose RPC port
EXPOSE 18081

# Expose restricted RPC port
EXPOSE 18089

# Add HEALTHCHECK against get_info endpoint
HEALTHCHECK --interval=30s --timeout=5s --start-period=45s --retries=2 \
            CMD curl --fail http://localhost:18089/get_info || exit 1

ENTRYPOINT ["monerod"]
CMD ["--non-interactive", "--restricted-rpc", "--rpc-bind-ip=0.0.0.0", "--rpc-restricted-bind-port=18089", "--confirm-external-bind", "--enable-dns-blocklist", "--out-peers=16"]
