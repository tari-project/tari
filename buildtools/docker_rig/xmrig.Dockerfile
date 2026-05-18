# Source build

#ARG ALPINE_VERSION=3.21
ARG ALPINE_VERSION=3.19

FROM alpine:$ALPINE_VERSION as builder

ARG ALPINE_VERSION
ARG BUILDPLATFORM

# https://github.com/xmrig/xmrig/releases
#ARG XMRIG_VERSION="v6.22.2"
ARG XMRIG_VERSION="v6.21.3"

RUN apk add \
        git \
        make \
        cmake \
        libstdc++ \
        gcc \
        g++  \
        automake \
        libtool \
        autoconf \
        linux-headers \
        libuv-dev \
        openssl-dev \
        hwloc-dev \
        bash

RUN git clone --branch ${XMRIG_VERSION} https://github.com/xmrig/xmrig.git
RUN mkdir /xmrig/build
WORKDIR /xmrig/scripts
RUN bash ./build_deps.sh
WORKDIR /xmrig/build
RUN cmake .. -DXMRIG_DEPS=scripts/deps -DBUILD_STATIC=ON
RUN make -j$(nproc)

# Create runtime base minimal image for the target platform executables
FROM --platform=$TARGETPLATFORM alpine:$ALPINE_VERSION as runtime

ARG ALPINE_VERSION
ARG BUILDPLATFORM

ARG XMRIG_VERSION
ARG VERSION=1.0.1
COPY --from=builder /xmrig/build/xmrig /usr/local/bin/

# Create a user & group
RUN addgroup -g 1000 tari && \
    adduser -u 1000 -g 1000 -S tari -G tari

# Chown all the files to the app user.
RUN mkdir -p /home/tari && \
    chown tari:tari /home/tari

USER tari

ENV dockerfile_version=$VERSION
ENV dockerfile_build_arch=$BUILDPLATFORM
ENV xmrig_version=$XMRIG_VERSION

ENTRYPOINT [ "/usr/local/bin/xmrig" ]
