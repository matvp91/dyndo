# syntax=docker/dockerfile:1

# Keep this aligned with `.github/actions/setup-ffmpeg/action.yml`. rsmpeg's
# generated bindings and the shared libraries it links against must use the
# same FFmpeg major version in CI and in the container image.
ARG FFMPEG_VERSION=8.0.3

# ---- FFmpeg stage ----
FROM debian:trixie-slim AS ffmpeg
ARG FFMPEG_VERSION
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        nasm \
        pkg-config \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /tmp
RUN curl --fail --location --retry 3 \
        "https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz" \
        --output ffmpeg.tar.xz \
    && tar --extract --file ffmpeg.tar.xz \
    && cd "ffmpeg-${FFMPEG_VERSION}" \
    && ./configure \
        --prefix=/opt/ffmpeg \
        --disable-static \
        --enable-shared \
        --disable-programs \
        --disable-doc \
        --disable-debug \
        --disable-autodetect \
    && make --jobs "$(nproc)" \
    && make install

# ---- build stage ----
# Pin the exact rustc to match rust-toolchain.toml (FROM can't read that file,
# so this is the one deliberate duplicate — bump both together). The Debian
# codename is pinned too (not plain `rust:1-slim`, which tracks Debian's
# latest) so the build glibc matches the runtime stage below and can't
# silently drift.
FROM rust:1.97.0-slim-trixie AS build
WORKDIR /src
COPY --from=ffmpeg /opt/ffmpeg /opt/ffmpeg
COPY . .
# Cache the cargo registry and target dir across local builds (BuildKit). The
# binary is copied OUT of the target/ cache mount within the same RUN, because
# cache-mount contents do not persist into the image layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    PKG_CONFIG_PATH=/opt/ffmpeg/lib/pkgconfig \
    LD_LIBRARY_PATH=/opt/ffmpeg/lib \
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
COPY --from=ffmpeg /opt/ffmpeg/lib/ /usr/local/lib/
RUN ldconfig
# Run unprivileged.
RUN useradd --system --uid 10001 dyndo
USER dyndo
COPY --from=build /usr/local/bin/dyndo-server /usr/local/bin/dyndo-server
EXPOSE 8080
ENTRYPOINT ["dyndo-server"]
