FROM alpine:latest

RUN apk update \
 && apk upgrade \
 && apk add tor \
            bash \
 && rm /var/cache/apk/*

EXPOSE 9050
EXPOSE 9051

ADD ./torrc /etc/tor/torrc

USER tor
CMD /usr/bin/tor -f /etc/tor/torrc