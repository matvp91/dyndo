# syntax=docker/dockerfile:1

# ---- build stage ----
# Pin the Debian codename (not plain `rust:1-slim`, which tracks Debian's latest)
# so the build glibc matches the runtime stage below and can't silently drift.
FROM rust:1-slim-trixie AS build
WORKDIR /src
COPY . .
# Cache the cargo registry and target dir across local builds (BuildKit). The
# binary is copied OUT of the target/ cache mount within the same RUN, because
# cache-mount contents do not persist into the image layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p dyndo-server && \
    cp target/release/dyndo-server /usr/local/bin/dyndo-server

# ---- runtime stage ----
# Same Debian release as the build image, so the runtime glibc satisfies the
# binary's GLIBC_* symbols (a bookworm runtime vs a trixie build fails at
# startup with `GLIBC_2.38 not found`).
FROM debian:trixie-slim
# rustls verifies S3's TLS certs against the system trust store; fs-only runs
# never touch this. No libssl is needed (TLS is pure-Rust rustls).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# Run unprivileged.
RUN useradd --system --uid 10001 dyndo
USER dyndo
COPY --from=build /usr/local/bin/dyndo-server /usr/local/bin/dyndo-server
EXPOSE 8080
ENTRYPOINT ["dyndo-server"]
