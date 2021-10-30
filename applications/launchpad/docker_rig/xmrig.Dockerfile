FROM alpine:latest as build

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
# TODO - pull specific tag
RUN git clone https://github.com/xmrig/xmrig.git
RUN mkdir xmrig/build
WORKDIR /xmrig/scripts
RUN ./build_deps.sh
WORKDIR /xmrig/build
RUN cmake .. -DXMRIG_DEPS=scripts/deps -DBUILD_STATIC=ON
RUN make -j$(nproc)

FROM alpine:latest as base
COPY --from=build /xmrig/build/xmrig /usr/bin/

# Create a user & group
RUN addgroup -S xmrig
RUN adduser -S -D -h /home/xmrig xmrig xmrig
# Chown all the files to the app user.
RUN chown xmrig:xmrig /usr/bin/xmrig
USER xmrig

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
