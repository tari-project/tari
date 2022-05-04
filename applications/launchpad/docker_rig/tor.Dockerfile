# Package build
FROM alpine:latest

# https://pkgs.alpinelinux.org/packages?name=tor&branch=v3.15&repo=community
ARG TOR_VERSION=0.4.6.9-r0
ARG VERSION=1.0.1

RUN apk update \
 && apk upgrade \
 && apk add tor=$TOR_VERSION \
 && rm /var/cache/apk/*

EXPOSE 9050
EXPOSE 9051
ENV dockerfile_version=$VERSION
ENV tor_version=$TOR_VERSION

USER tor
ENTRYPOINT ["/usr/bin/tor"]
CMD ["-f", "/etc/tor/torrc"]
