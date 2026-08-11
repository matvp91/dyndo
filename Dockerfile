# syntax=docker/dockerfile:1

# This is the single Linux FFmpeg build definition. All Linux CI verification,
# release artifacts, and runtime images build from these stages.
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
    && rm -rf /var/lib/apt/lists/*
WORKDIR /tmp
RUN curl --fail --location --retry 5 --retry-all-errors --retry-delay 2 \
        "https://github.com/FFmpeg/FFmpeg/archive/refs/tags/n${FFMPEG_VERSION}.tar.gz" \
        --output ffmpeg.tar.gz \
    && mkdir ffmpeg \
    && tar --extract --file ffmpeg.tar.gz --strip-components=1 --directory ffmpeg \
    && cd ffmpeg \
    && ./configure \
        --prefix=/opt/ffmpeg \
        --disable-static \
        --enable-shared \
        --disable-programs \
        --disable-doc \
        --disable-debug \
        --disable-autodetect \
        --disable-everything \
        --enable-avcodec \
        --enable-avdevice \
        --enable-avfilter \
        --enable-avformat \
        --enable-avutil \
        --enable-swresample \
        --enable-swscale \
        --enable-demuxer=mov \
        --enable-decoder=h264,hevc,av1 \
        --enable-encoder=mjpeg \
        --enable-parser=h264,hevc,av1 \
    && make --jobs "$(nproc)" \
    && make install

# ---- build stage ----
# Pin the exact rustc to match rust-toolchain.toml (FROM can't read that file,
# so this is the one deliberate duplicate — bump both together). The Debian
# codename is pinned too (not plain `rust:1-slim`, which tracks Debian's
# latest) so the build glibc matches the runtime stage below and can't
# silently drift.
FROM rust:1.97.0-slim-trixie AS build
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libclang-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add clippy
WORKDIR /src
COPY --from=ffmpeg /opt/ffmpeg /opt/ffmpeg
COPY . .
ENV PKG_CONFIG_PATH=/opt/ffmpeg/lib/pkgconfig \
    LD_LIBRARY_PATH=/opt/ffmpeg/lib

# ---- verification stage ----
# Linux CI builds this target, so every FFmpeg-dependent check runs against the
# same compiler, libraries, and headers as the shipped server image.
FROM build AS verify
RUN --mount=type=cache,id=dyndo-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=dyndo-target,target=/src/target \
    cargo clippy --all-targets -- -D warnings && \
    cargo test && \
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace

# ---- release artifacts stage ----
FROM build AS artifacts
RUN --mount=type=cache,id=dyndo-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=dyndo-target,target=/src/target \
    cargo build --release -p dyndo-cli -p dyndo-server && \
    install --directory /out && \
    install --mode=755 target/release/dyndo /out/dyndo && \
    install --mode=755 target/release/dyndo-server /out/dyndo-server

# This target exports Linux release binaries with Buildx's type=local output.
FROM scratch AS artifact-output
COPY --from=artifacts /out/ /

# ---- server stage ----
# The runtime image does not need the CLI, so avoid compiling it for every
# published Linux architecture.
FROM build AS server
RUN --mount=type=cache,id=dyndo-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=dyndo-target,target=/src/target \
    cargo build --release -p dyndo-server && \
    install --directory /out && \
    install --mode=755 target/release/dyndo-server /out/dyndo-server

# ---- JSON Schema stage ----
FROM build AS schema
RUN --mount=type=cache,id=dyndo-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=dyndo-target,target=/src/target \
    install --directory /out && \
    cargo xtask asset-schema --output /out/schema.json

# This target exports only the generated schema to the Pages workflow.
FROM scratch AS schema-output
COPY --from=schema /out/schema.json /schema.json

# ---- runtime stage ----
# Same Debian release as the build image, so the runtime glibc satisfies the
# binary's GLIBC_* symbols (a bookworm runtime vs a trixie build fails at
# startup with `GLIBC_2.38 not found`).
FROM debian:trixie-slim AS runtime
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
COPY --from=server /out/dyndo-server /usr/local/bin/dyndo-server
EXPOSE 8080
ENTRYPOINT ["dyndo-server"]
