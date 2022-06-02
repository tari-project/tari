# Package build
FROM alpine:latest

ARG BUILDPLATFORM
ARG VERSION=1.0.1

# https://pkgs.alpinelinux.org/packages?name=tor&branch=v3.16&repo=community
ARG TOR_VERSION=0.4.7.7-r1

RUN apk update \
 && apk upgrade \
 && apk add tor=$TOR_VERSION \
 && rm /var/cache/apk/*

EXPOSE 9050
EXPOSE 9051
ENV dockerfile_version=$VERSION
ENV dockerfile_build_arch=$BUILDPLATFORM
ENV tor_version=$TOR_VERSION

USER tor
ENTRYPOINT ["/usr/bin/tor"]
CMD ["-f", "/etc/tor/torrc"]
