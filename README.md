# dyndo

**Dynamic adaptive-streaming delivery from media you already store.**

![Rust](https://img.shields.io/badge/rust-2024-orange?logo=rust)
![Packaging](https://img.shields.io/badge/packaging-DASH%20%7C%20HLS%20%7C%20CMAF-blue)

`dyndo` serves existing CMAF files without repackaging or duplicating their
media bytes. A small JSON descriptor supplies the presentation metadata;
manifests, segment views, subtitles, and thumbnails are produced when requested
while the original media stays untouched.

## Why dyndo?

- **Store media once.** Segment requests are byte-range reads from the original
  CMAF objects, not files copied into a second packaging layout.
- **Change presentation metadata without rewriting media.** Correct a language,
  assign a role, change rendition order, or add a subtitle by editing or
  rebuilding a small descriptor.
- **Keep URLs stable while metadata evolves.** Track IDs derive from source
  paths rather than mutable labels such as language and role.
- **Create delivery variants at request time.** Filters, segment lengths, DASH
  periods, and HLS subtitle form can vary without producing another media
  library.
- **Keep sources immutable.** The server only reads headers and requested byte
  ranges, which suits read-only filesystems and object storage.
- **Keep indexing and delivery aligned.** The CLI writes the same compact
  descriptor that the server reads, so presentation metadata has one source of
  truth.
- **Scale parsing with metadata, not file size.** dyndo derives its index from
  bounded header reads instead of scanning or loading complete media objects.

## 📖 Documentation

Full documentation lives at **<https://matvp91.github.io/dyndo/>**:

- **[Getting started](https://matvp91.github.io/dyndo/tutorial/getting-started.html)**
  — build, index, and serve your first stream end to end.
- **[How-to guides](https://matvp91.github.io/dyndo/how-to/index-sources.html)**
  — index sources, add subtitles, run the server, serve from S3.
- **[Reference](https://matvp91.github.io/dyndo/reference/cli.html)** — the CLI,
  the server's routes and configuration, and the `asset.json` descriptor.
- **[Explanation](https://matvp91.github.io/dyndo/explanation/thin-pointer.html)**
  — storage-efficient dynamic packaging, the thin-pointer design, and
  bounded-memory parsing.

## Install

```bash
curl -fsSL https://matvp91.github.io/dyndo/install.sh | bash
```

Installs the prebuilt `dyndo` CLI for macOS or Linux into `~/.dyndo/bin` and
puts it on your `PATH`. Pin a version with `bash -s <version>`. To build from
source instead, follow the [Quickstart](#quickstart) below.

## Quickstart

```bash
# Build both binaries; install the dyndo CLI into ~/.cargo/bin
cargo build
make install

# Index your CMAF sources into a descriptor under ./assets
dyndo index video.mp4 audio.mp4 -o assets/asset.json

# Serve it as DASH + HLS from ./assets on :8080
make run
```

Then point a player at either protocol — the bracketed part names the descriptor
relative to the storage root, without its `.json` extension:

```
http://localhost:8080/out/(asset:asset)/index.mpd      # DASH
http://localhost:8080/out/(asset:asset)/master.m3u8    # HLS
```

New here? The
**[Getting started guide](https://matvp91.github.io/dyndo/tutorial/getting-started.html)**
walks through the whole flow, including producing CMAF sources with ffmpeg.

Prefer not to build at all? `dyndo-server` is published to Docker Hub as
[`matvp91/dyndo-server`](https://hub.docker.com/r/matvp91/dyndo-server) — see
**[Deploy with Docker](https://matvp91.github.io/dyndo/how-to/deploy-with-docker.html)**:

```bash
docker run --rm -p 8080:8080 -e DYNDO_FS__ROOT=/assets \
  -v "$PWD/assets:/assets:ro" matvp91/dyndo-server
```

## Project layout

`dyndo` is a Cargo workspace of five crates — three libraries and two binaries —
with a strictly one-way dependency direction. `dyndo-core` knows nothing about
manifests; the two manifest crates know nothing about each other; neither
library layer knows anything about CLI or HTTP concerns.

```text
binaries     dyndo-cli                    dyndo-server
                 │                         ┌───┴───┐
manifests        │                    dyndo-dash  dyndo-hls
                 │                         └───┬───┘
core             └───────────────────── dyndo-core
```

| Crate                                 | Kind                    | Responsibility                                                                                                                                                                         |
| ------------------------------------- | ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`dyndo-core`](crates/dyndo-core)     | library                 | CMAF header parsing, track and segment models, byte-range reading, WebVTT packaging, and on-demand thumbnail generation. Reads storage through [OpenDAL](https://opendal.apache.org/). |
| [`dyndo-dash`](crates/dyndo-dash)     | library                 | DASH MPD generation: adaptation-set grouping, segment templates and timelines, DASH role signalling.                                                                                   |
| [`dyndo-hls`](crates/dyndo-hls)       | library                 | HLS playlist generation: the multivariant playlist, per-track media playlists, rendition groups and HLS role signalling.                                                               |
| [`dyndo-cli`](crates/dyndo-cli)       | binary (`dyndo`)        | Indexes sources into descriptors and extracts video frames as JPEG images.                                                                                                             |
| [`dyndo-server`](crates/dyndo-server) | binary (`dyndo-server`) | The dynamic packaging HTTP server, built on [Axum](https://github.com/tokio-rs/axum).                                                                                                  |

`dyndo-server` combines the protocol builders with the storage and segment model
in `dyndo-core`. The CLI works directly with `dyndo-core`; it does not generate
manifests.

## Development

Common tasks are wrapped in the [`Makefile`](Makefile):

| Target             | Description                                           |
| ------------------ | ----------------------------------------------------- |
| `make build`       | Release build of the CLI.                             |
| `make build-debug` | Debug build of the CLI.                               |
| `make run`         | Run `dyndo-server`.                                   |
| `make test`        | Run the whole workspace test suite.                   |
| `make lint`        | Clippy across all targets, warnings as errors.        |
| `make fmt`         | Format all crates (nightly `rustfmt`).                |
| `make fmt-check`   | Verify formatting without modifying.                  |
| `make check`       | Fast type-check of the workspace.                     |
| `make install`     | Install the `dyndo` CLI into `~/.cargo/bin`.          |
| `make doc`         | Build the crates' rustdoc.                            |
| `make book`        | Build the mdBook user guide into `docs/book`.         |
| `make book-serve`  | Serve the mdBook user guide locally with live reload. |
| `make clean`       | Remove build artifacts.                               |

Building the book needs [mdBook](https://rust-lang.github.io/mdBook/) — install
the version pinned as `MDBOOK_VERSION` in
[`.github/workflows/docs.yml`](.github/workflows/docs.yml)
(`cargo install mdbook --version <that version>`) so local output matches what
CI publishes. The guide's sources live in [`docs/`](docs/) and are published to
GitHub Pages by the same workflow: [`/main/`](https://matvp91.github.io/dyndo/main/)
tracks `main`, while each release remains available at
`https://matvp91.github.io/dyndo/<version>/`.

Tests run against small, committed header-only CMAF fixtures in the
[`dyndo-core`](crates/dyndo-core/tests/fixtures),
[`dyndo-dash`](crates/dyndo-dash/tests/fixtures), and
[`dyndo-hls`](crates/dyndo-hls/tests/fixtures) crates — just enough of each file
(`ftyp` + `moov` + `sidx` + first `moof`) to exercise parsing end to end without
shipping gigabytes of media.
