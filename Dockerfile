# Use a Debian-compatible Rust base for a reproducible server build.
FROM rust:1.88-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates procps \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/apkez /usr/local/bin/apkez
WORKDIR /workspace
ENTRYPOINT ["/usr/local/bin/apkez"]
CMD ["--root", "/workspace"]
