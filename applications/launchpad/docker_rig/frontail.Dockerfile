# Package build
FROM node:16-alpine

ARG BUILDPLATFORM
ARG FRONTAIL_VERSION=4.9.2
ARG VERSION=1.0.1

RUN npm install -g frontail

ADD run_frontail.sh /usr/bin/
WORKDIR /var/tari

EXPOSE 9001
ENV dockerfile_version=$VERSION
ENV dockerfile_build_arch=$BUILDPLATFORM
ENV frontail_version=$FRONTAIL_VERSION

ENTRYPOINT ["/usr/bin/run_frontail.sh"]
CMD ["--help"]
