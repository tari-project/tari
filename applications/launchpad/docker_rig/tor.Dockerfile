FROM alpine:latest
ARG VERSION=1.0.1

RUN apk update \
 && apk upgrade \
 && apk add tor \
            bash \
 && rm /var/cache/apk/*

EXPOSE 9050
EXPOSE 9051
ENV dockerfile_version=$VERSION

USER tor
CMD /usr/bin/tor -f /etc/tor/torrc
