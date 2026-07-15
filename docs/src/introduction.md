# dyndo

**Dynamic media packaging for adaptive streaming, in Rust.**

`dyndo` turns your existing CMAF files into an adaptive-streaming service
**without repackaging or duplicating a single byte of media**. You index your
sources once into a small JSON descriptor; the server then generates DASH and
HLS manifests and serves CMAF segments *on the fly*, straight from the original
files via HTTP byte-range reads.

> `dyndo` is in early development (`0.2.0`). Both DASH and HLS are implemented,
> served from the same CMAF sources.

## The idea in one picture

```text
  CMAF sources                asset.json                dyndo-server
  (fragmented MP4      dyndo    (thin                       │
   + global sidx)  ──index──▶   descriptor)  ────────▶  ┌───┴────────────────────┐
        │                                               │  GET …/dash/index.mpd  │
        └───────────── byte-range reads ───────────────▶│  GET …/hls/index.m3u8  │
                                                        │  GET …/<repr>/init.mp4 │
                                                        │  GET …/<repr>/<t>.m4s  │
                                                        └────────────────────────┘
```

1. **Index** your CMAF files once with the `dyndo` CLI to produce `asset.json`.
2. **Serve** that descriptor with `dyndo-server`. At request time it parses each
   source's header, derives the segment index, and answers manifest and segment
   requests with ranged reads from the original media.

Because the descriptor stores only per-track metadata and a source path — no
segment list, no byte offsets — a single `asset.json` is a few hundred bytes,
and serving an 800 MB source reads the same ~10 KB header region as an 8 MB one.
See [The thin-pointer approach](./explanation/thin-pointer.md) for why.

## The two binaries

`dyndo` is a Cargo workspace. This documentation covers its two executables:

| Binary | Role |
|---|---|
| [`dyndo`](./reference/cli.md) | The CLI: index CMAF sources into `asset.json`, pack subtitles, and render offline manifests. |
| [`dyndo-server`](./reference/server.md) | The HTTP server: generate DASH/HLS manifests and serve CMAF segments on the fly. |

Both are thin front-ends over the `dyndo-core` library, which does the CMAF
parsing and manifest generation. This book documents the two binaries and the
`asset.json` contract between them; the core library's Rust API is documented in
its [rustdoc](https://github.com/matvp91/dyndo).

## How to read this book

This documentation follows the [Diátaxis](https://diataxis.fr/) framework. Pick
the section that matches what you need right now:

- **[Tutorial](./tutorial/getting-started.md)** — *I'm new here.* A single
  guided lesson that takes you from nothing to a playing stream.
- **[How-to guides](./how-to/index-sources.md)** — *I have a task to do.*
  Focused recipes: index sources, add subtitles, configure the server, serve
  from S3.
- **[Reference](./reference/cli.md)** — *I need to look something up.* Exact,
  exhaustive descriptions of every command, route, config key, and descriptor
  field.
- **[Explanation](./explanation/thin-pointer.md)** — *I want to understand why.*
  The design ideas behind dyndo: the thin pointer, bounded-memory parsing, and
  one source serving two protocols.

## Supported codecs

Codec parameters are read from the source and emitted as
[RFC 6381](https://datatracker.ietf.org/doc/html/rfc6381) strings.

| Media | Codec | Sample entry |
|---|---|---|
| Video | AVC / H.264 | `avc1` |
| Video | AV1 | `av01` |
| Audio | AAC | `mp4a` |
| Audio | Dolby Digital (AC-3) | `ac-3` |
| Audio | Dolby Digital Plus (E-AC-3) | `ec-3` |
| Text | WebVTT in ISO-BMFF | `wvtt` |
