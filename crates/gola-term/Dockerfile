FROM alpine:latest

RUN apk add --no-cache ca-certificates

RUN mkdir -p /usr/share/doc/gola-term/copyright/
COPY LICENSE THIRDPARTY.html /usr/share/doc/gola-term/copyright/

ENV GOLA_EDITOR=none
COPY gola-term /usr/bin/

ENTRYPOINT ["/usr/bin/gola-term"]
