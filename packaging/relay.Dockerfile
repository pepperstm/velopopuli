# The group-ride relay as a small container image: ghcr.io/pepperstm/velopopuli-relay
#
#   docker run -d --restart unless-stopped -p 47800:47800 ghcr.io/pepperstm/velopopuli-relay
#
# Built from the repository root: docker build -f packaging/relay.Dockerfile .

# Alpine's Rust builds a static (musl) binary, so the image needs nothing else to run it.
FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p velopopuli-relay \
 && strip target/release/velopopuli-relay

# No shell or package manager: just the relay, run as an unprivileged user.
FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /src/target/release/velopopuli-relay /velopopuli-relay
EXPOSE 47800
ENTRYPOINT ["/velopopuli-relay"]
# Give another address to listen on another port: docker run … velopopuli-relay 0.0.0.0:9000
CMD ["0.0.0.0:47800"]
