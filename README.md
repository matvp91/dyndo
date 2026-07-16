# dyndo

**Dynamic media packaging for adaptive streaming, in Rust.**

![Rust](https://img.shields.io/badge/rust-2021-orange?logo=rust)
![Packaging](https://img.shields.io/badge/packaging-DASH%20%7C%20HLS%20%7C%20CMAF-blue)

`dyndo` turns your existing CMAF files into an adaptive-streaming service
**without repackaging or duplicating a single byte of media**. You index your
sources once into a tiny JSON descriptor; the server then generates DASH and HLS
manifests and serves CMAF segments _on the fly_, straight from the original
files via HTTP byte-range reads.

> [!NOTE] `dyndo` is in early development (`0.3.0`). Both DASH and HLS are
> implemented, served from the same CMAF sources.

## 📖 Documentation

Full documentation lives at **<https://matvp91.github.io/dyndo/>**:

- **[Getting started](https://matvp91.github.io/dyndo/tutorial/getting-started.html)**
  — build, index, and serve your first stream end to end.
- **[How-to guides](https://matvp91.github.io/dyndo/how-to/index-sources.html)**
  — index sources, add subtitles, run the server, serve from S3.
- **[Reference](https://matvp91.github.io/dyndo/reference/cli.html)** — the CLI,
  the server's routes and configuration, and the `asset.json` descriptor.
- **[Explanation](https://matvp91.github.io/dyndo/explanation/thin-pointer.html)**
  — the thin-pointer design, bounded-memory parsing, and one source / two
  protocols.

## Quickstart

```bash
# Build both binaries; install the dyndo CLI into ~/.cargo/bin
cargo build
make install

# Index your CMAF sources into a descriptor under ./assets
dyndo index -i video.mp4 -i audio.mp4 -o assets/asset.json

# Serve it as DASH + HLS from ./assets on :8080
make run
```

Then point a player at either protocol:

```
http://localhost:8080/asset.json/dash/index.mpd    # DASH
http://localhost:8080/asset.json/hls/index.m3u8     # HLS
```

New here? The
**[Getting started guide](https://matvp91.github.io/dyndo/tutorial/getting-started.html)**
walks through the whole flow, including producing CMAF sources with ffmpeg.

## Project layout

`dyndo` is a Cargo workspace of three crates — one library and two binaries —
with a clean dependency direction: the core library carries no CLI or HTTP
concerns and is reused by both binaries.

| Crate                                 | Kind                    | Responsibility                                                                                                                                                                                                                                |
| ------------------------------------- | ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`dyndo-core`](crates/dyndo-core)     | library                 | CMAF header parsing (bounded memory via `mp4-atom`), the `Asset`/`Track` domain model, the `asset.json` serde contract, RFC 6381 codec strings, and DASH/HLS manifest generation. Reads bytes through [OpenDAL](https://opendal.apache.org/). |
| [`dyndo-cli`](crates/dyndo-cli)       | binary (`dyndo`)        | The indexing, subtitle-packing, and offline-manifest CLI.                                                                                                                                                                                     |
| [`dyndo-server`](crates/dyndo-server) | binary (`dyndo-server`) | The dynamic packaging HTTP server, built on [Axum](https://github.com/tokio-rs/axum).                                                                                                                                                         |

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
| `make doc`         | Build the crates' rustdoc.                            |
| `make book`        | Build the mdBook user guide into `docs/book`.         |
| `make book-serve`  | Serve the mdBook user guide locally with live reload. |
| `make clean`       | Remove build artifacts.                               |

Building the book needs [mdBook](https://rust-lang.github.io/mdBook/)
(`cargo install mdbook`). The guide's sources live in [`docs/`](docs/) and are
published to GitHub Pages by
[`.github/workflows/docs.yml`](.github/workflows/docs.yml).

Tests run against small, committed header-only CMAF fixtures under
[`tests/fixtures`](tests/fixtures) — just enough of each file (`ftyp` + `moov` +
`sidx` + first `moof`) to exercise parsing end to end without shipping gigabytes
of media.

## Releasing

Releases are cut locally and published by CI:

```bash
./scripts/release.sh          # prompts for the next version, e.g. 0.2.0
```

The script bumps all three crate versions in lockstep, commits
`release: <version>`, tags `v<version>`, and pushes. Pushing the tag triggers
[`.github/workflows/release.yml`](.github/workflows/release.yml), which verifies
the tag matches `Cargo.toml`, re-runs the CI gate, builds `dyndo` and
`dyndo-server` for Linux and macOS, and publishes a GitHub Release with the
binaries and a `SHA256SUMS` file. The same tag push also builds and publishes a
multi-arch (`linux/amd64` + `linux/arm64`) `dyndo-server` image to Docker Hub,
tagged `:<version>`, `:<major>.<minor>`, and `:latest`.
