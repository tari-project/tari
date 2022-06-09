# Source build
FROM alpine:latest as builder

# Declare to make available
ARG BUILDPLATFORM

# https://github.com/xmrig/xmrig/releases
ARG XMRIG_VERSION="v6.17.0"

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
    bash

RUN git clone --branch ${XMRIG_VERSION} https://github.com/xmrig/xmrig.git
RUN mkdir /xmrig/build
WORKDIR /xmrig/scripts
RUN bash ./build_deps.sh
WORKDIR /xmrig/build
RUN cmake .. -DXMRIG_DEPS=scripts/deps -DBUILD_STATIC=ON
RUN make -j$(nproc)

# Create runtime base minimal image for the target platform executables
FROM --platform=$TARGETPLATFORM alpine:latest as runtime

ARG BUILDPLATFORM

ARG XMRIG_VERSION
ARG VERSION=1.0.1
COPY --from=builder /xmrig/build/xmrig /usr/bin/

# Create a user & group
RUN addgroup -g 1000 tari && adduser -u 1000 -g 1000 -S tari -G tari
RUN mkdir -p /home/tari && chown tari.tari /home/tari
# Chown all the files to the app user.
USER tari
ENV dockerfile_version=$VERSION
ENV dockerfile_build_arch=$BUILDPLATFORM
ENV xmrig_version=$XMRIG_VERSION

RUN echo -e "\
{\
    \"autosave\": true,\
    \"cpu\": true,\
    \"opencl\": false,\
    \"cuda\": false,\
    \"pools\": [ \
    { \"coin\": \"monero\", \"url\": \"127.0.0.1:18081\", \"user\": \"44\", \"daemon\": true }\
    ]\
}\
" > /home/tari/.xmrig.json

ENTRYPOINT [ "/usr/bin/xmrig" ]
