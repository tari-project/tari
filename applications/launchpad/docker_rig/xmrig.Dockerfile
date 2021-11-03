FROM alpine:latest as build

ARG VERSION="v6.15.3"

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
    linux-headers

RUN git clone --branch ${VERSION} https://github.com/xmrig/xmrig.git
RUN mkdir xmrig/build
WORKDIR /xmrig/scripts
RUN ./build_deps.sh
WORKDIR /xmrig/build
RUN cmake .. -DXMRIG_DEPS=scripts/deps -DBUILD_STATIC=ON
RUN make -j$(nproc)

FROM alpine:latest as base
COPY --from=build /xmrig/build/xmrig /usr/bin/

# Create a user & group
RUN groupadd -g 1000 tari && useradd -ms /bin/bash -u 1000 -g 1000 tari

# Chown all the files to the app user.
USER tari

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
" > /home/xmrig/.xmrig.json

ENTRYPOINT [ "/usr/bin/xmrig" ]
