FROM node:16-alpine

RUN npm install -g frontail

ADD run_frontail.sh /usr/bin/
WORKDIR /var/tari
ENTRYPOINT ["/usr/bin/run_frontail.sh"]
EXPOSE 9001
CMD ["--help"]