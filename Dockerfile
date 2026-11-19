# Blip server (see SERVER.md): the Rust build, about 2 MB, nothing else inside.
#   docker build -t blip-server https://github.com/safder2000/blip.git
#   docker run -d --name blip-server --restart unless-stopped -p 47620:47620 blip-server
FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY rust/ .
RUN cargo build --release -p blip-server

FROM alpine:3.20
COPY --from=build /src/target/release/blip-server /usr/local/bin/blip-server
ENV PORT=47620
EXPOSE 47620
HEALTHCHECK --interval=30s --timeout=5s CMD wget -qO- "http://127.0.0.1:$PORT/health" >/dev/null || exit 1
CMD ["blip-server"]
