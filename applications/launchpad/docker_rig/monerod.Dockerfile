# Usage: docker run --restart=always -v /var/data/blockchain-xmr:/root/.bitmonero -p 18080:18080 -p 18081:18081 --name=monerod -td kannix/monero-full-node
FROM quay.io/bitnami/minideb:bullseye AS build

ENV MONERO_VERSION=0.17.2.3 MONERO_SHA256=8069012ad5e7b35f79e35e6ca71c2424efc54b61f6f93238b182981ba83f2311


RUN apt-get update && apt-get install -y curl bzip2

WORKDIR /root

RUN curl https://dlsrc.getmonero.org/cli/monero-linux-x64-v$MONERO_VERSION.tar.bz2 -O &&\
  echo "$MONERO_SHA256  monero-linux-x64-v$MONERO_VERSION.tar.bz2" | sha256sum -c - &&\
  tar -xvf monero-linux-x64-v$MONERO_VERSION.tar.bz2 &&\
  rm monero-linux-x64-v$MONERO_VERSION.tar.bz2 &&\
  cp ./monero-x86_64-linux-gnu-v$MONERO_VERSION/monerod . &&\
  rm -r monero-*

FROM quay.io/bitnami/minideb:bullseye

RUN groupadd -g 1000 tari && useradd -ms /bin/bash -u 1000 -g 1000 tari \
    && mkdir -p /home/tari/.bitmonero  \
    && chown -R tari:tari /home/tari/.bitmonero
USER tari
WORKDIR /home/tari

COPY --chown=tari:tari --from=build /root/monerod /home/tari/monerod

# blockchain location
VOLUME /home/tari/.bitmonero

EXPOSE 18080 18081


ENTRYPOINT ["./monerod"]
CMD ["--non-interactive", "--restricted-rpc", "--rpc-bind-ip=0.0.0.0", "--confirm-external-bind", "--enable-dns-blocklist", "--out-peers=16"]
