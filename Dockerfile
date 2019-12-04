FROM rust:1.39.0 AS build
WORKDIR /usr/src

# Download the target for static linking.
RUN rustup target add x86_64-unknown-linux-musl

# Build an empty project with our dependencies so that we can cache the compiled dependencies
RUN USER=root cargo new sidecar-http-proxy
WORKDIR /usr/src/sidecar-http-proxy
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release

# Add our source code.
COPY src/ ./src/

# Build for real
RUN cargo install --target x86_64-unknown-linux-musl --path .

# Now for the runtime image
FROM scratch

COPY --from=build /usr/local/cargo/bin/sidecar-http-proxy /sidecar-http-proxy

USER 1000

ENTRYPOINT ["/sidecar-http-proxy"]
CMD ["--help"]
