apt-get install --no-install-recommends --assume-yes \
  apt-transport-https \
  ca-certificates \
  curl \
  gpg \
  bash \
  less \
  openssl \
  libssl-dev \
  pkg-config \
  libsqlite3-dev \
  libsqlite3-0 \
  libreadline-dev \
  git \
  make \
  cmake \
  dh-autoreconf \
  clang \
  g++ \
  libc++-dev \
  libc++abi-dev \
  libprotobuf-dev \
  protobuf-compiler \
  libncurses5-dev \
  libncursesw5-dev \
  libudev-dev \
  libhidapi-dev \
  libdbus-1-dev \
  zip \
  unzip

# Ubuntu's apt `protobuf-compiler` is too old (3.12.4 on 22.04) to compile the
# proto3 `optional` fields used in our .proto files, so install a modern protoc
# from the upstream release into /usr/local (which precedes /usr/bin in PATH).
PROTOC_VERSION="29.3"
case "$(uname -m)" in
  x86_64) PROTOC_ARCH="x86_64" ;;
  aarch64 | arm64) PROTOC_ARCH="aarch_64" ;;
  *) PROTOC_ARCH="x86_64" ;;
esac
curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused --location --silent --show-error --fail \
  --output /tmp/protoc.zip \
  "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-linux-${PROTOC_ARCH}.zip"
unzip -o /tmp/protoc.zip -d /usr/local bin/protoc 'include/*'
rm -f /tmp/protoc.zip
chmod +x /usr/local/bin/protoc
protoc --version
