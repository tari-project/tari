# syntax=docker/dockerfile:1
#FROM rust:1.42.0 as builder
FROM quay.io/tarilabs/rust_tari-build-with-deps:nightly-2024-02-04 as builder

# Copy the dependency lists
#ADD Cargo.toml ./
ADD . /minotari_node
WORKDIR /minotari_node

# RUN rustup component add rustfmt --toolchain nightly-2024-02-04-x86_64-unknown-linux-gnu
#ARG TBN_ARCH=native
ARG TBN_ARCH=x86-64
#ARG TBN_FEATURES=avx2
ARG TBN_FEATURES=safe
ENV RUSTFLAGS="-C target_cpu=$TBN_ARCH"
ENV ROARING_ARCH=$TBN_ARCH
# Work around for odd issue with broken Cargo.lock and builds
RUN cargo fetch && \
  cd applications/minotari_node && \
  cargo build --bin minotari_node --release --features $TBN_FEATURES --locked

# Create a base minimal image for adding our executables to
FROM quay.io/bitnami/minideb:buster as base

#RUN mv /usr/sbin/policy-rc.d /usr/sbin/policy-rc.d.org && echo "#!/bin/sh\nexit 0" > /usr/sbin/policy-rc.d

# Disable Prompt During Packages Installation
ARG DEBIAN_FRONTEND=noninteractive
RUN apt update && apt -y install \
    curl \
    bash \
    gpg \
    apt-transport-https \
    ca-certificates && \
    printf "\
# Add Sources for the latest tor - https://support.torproject.org/apt/tor-deb-repo/ \n\
deb https://deb.torproject.org/torproject.org buster main\n\
deb-src https://deb.torproject.org/torproject.org buster main\n"\
     > /etc/apt/sources.list.d/tor-apt-sources.list && \
    curl https://deb.torproject.org/torproject.org/A3C4F0F979CAA22CDBA8F512EE8CBC9E886DDD89.asc | gpg --import && \
    gpg --export A3C4F0F979CAA22CDBA8F512EE8CBC9E886DDD89 | apt-key add - && \
    apt update && \
    apt-get install --no-install-recommends --no-install-suggests -y \
        pwgen \
        iputils-ping \
        tor \
        tor-geoipdb \
        deb.torproject.org-keyring

# Can't use tor as a service in docker
#    update-rc.d -f tor defaults 10 10 && \
#    update-rc.d -f tor enable 3 && \
#    /etc/init.d/tor start

# Setup minotari_node group & user
RUN groupadd --system minotari_node && \
  useradd --no-log-init --system --gid minotari_node --comment "Tari base node" --create-home minotari_node

# Now create a new image with only the essentials and throw everything else away
FROM base

COPY --from=builder /minotari_node/buildtools/docker/torrc /etc/tor/torrc
COPY --from=builder /minotari_node/buildtools/docker/start.sh /usr/local/bin/start_minotari_node.sh

COPY --from=builder /minotari_node/target/release/minotari_node /usr/local/bin/

USER minotari_node
#RUN echo ${HOME} && ls -la /home
RUN mkdir -p ~/.tari
COPY --from=builder /minotari_node/common/config/presets/*.toml /home/minotari_node/.tari
COPY --from=builder /minotari_node/common/logging/log4rs_sample_base_node.yml /home/minotari_node/.tari/log4rs_base_node.yml

# Keep the .tari directory in a volume by default
VOLUME ["/home/minotari_node/.tari"]
# Use start.sh to run tor then the base node or minotari_node for the executable
CMD ["/usr/local/bin/start_minotari_node.sh"]
