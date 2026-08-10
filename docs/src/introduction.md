# dyndo

**Dynamic adaptive-streaming delivery from media you already store.**

`dyndo` serves existing CMAF files without repackaging or duplicating their media bytes. A small JSON descriptor supplies the presentation metadata; manifests, segment views, subtitles, and thumbnails are produced when requested while the original media stays untouched.

> `dyndo` is in early development. Both DASH and HLS are implemented, served
> from the same CMAF sources.

## What this enables

- **No second media library.** The original CMAF objects are the media dyndo serves. Segment requests become byte-range reads rather than reads from separately packaged copies.
- **Metadata can change independently of media.** Languages, roles, rendition order, and subtitle membership live in `asset.json`, so correcting them does not rewrite a large source object.
- **Delivery variants do not require new files.** A request can filter tracks, regroup fragments, add boundaries, split a DASH presentation into periods, select the HLS subtitle form, or enable thumbnails.
- **Track URLs survive metadata corrections.** IDs derive from source paths, not mutable labels such as language or role.
- **Storage may remain read-only.** dyndo reads source headers and requested ranges but never writes generated media back beside them.
- **Local files and object storage use the same model.** OpenDAL supplies the storage boundary, while the descriptor and HTTP routes stay unchanged.

## The idea in one picture

```text
CMAF sources                asset.json                dyndo-server
(fragmented MP4      dyndo    (thin                       │
 + global sidx)  ──index──▶   descriptor)  ────────▶  ┌───┴─────────────────────────┐
      │                                               │  GET /out/(asset:…)/…       │
      │                                               │        index.mpd            │
      └───────────── byte-range reads ───────────────▶│        master.m3u8          │
                                                      │        <track>/init.mp4     │
                                                      │        <track>/<time>.m4s   │
                                                      └─────────────────────────────┘
```

1. **Index** your CMAF files once with the `dyndo` CLI to produce `asset.json`.
2. **Serve** that descriptor with `dyndo-server`. At request time it parses each
   source's header, derives the segment index, and answers manifest and segment
   requests with ranged reads from the original media.

Because the descriptor stores only per-track metadata and a source path — no
segment list, no byte offsets — a single `asset.json` is a few hundred bytes,
and serving an 800 MB source reads the same ~10 KB header region as an 8 MB one.
See [The thin-pointer approach](./explanation/thin-pointer.md) for why.

## The two tools

dyndo is two programs, and you get each the easy way — no Rust toolchain
required:

| Tool                                    | What it does                                                                  | How to get it                                                                                    |
| --------------------------------------- | ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| [`dyndo`](./reference/cli.md) (CLI)     | Index CMAF and WebVTT sources into `asset.json`, and extract video frames.    | The [one-line installer](./tutorial/install-cli.md).                                             |
| [`dyndo-server`](./reference/server.md) | Generate DASH/HLS manifests and serve CMAF segments on the fly.               | The [`matvp91/dyndo-server`](https://hub.docker.com/r/matvp91/dyndo-server) image on Docker Hub. |

This book covers both programs and the `asset.json` descriptor that connects
them: you produce a descriptor with the CLI, and the server reads it. If neither
the installer nor Docker fits your platform, you can also
[build from source](./how-to/build-from-source.md).

## Quick install

Install the `dyndo` CLI (macOS and Linux):

```bash
curl -fsSL https://matvp91.github.io/dyndo/install.sh | bash
```

See [Install the CLI](./tutorial/install-cli.md) for details. For the server,
pull the Docker image — see
[Deploy with Docker](./how-to/deploy-with-docker.md).

## How to read this book

This documentation follows the [Diátaxis](https://diataxis.fr/) framework. Pick
the section that matches what you need right now:

- **[Tutorial](./tutorial/getting-started.md)** — _I'm new here._ A single
  guided lesson that takes you from nothing to a playing stream.
- **[How-to guides](./how-to/index-sources.md)** — _I have a task to do._
  Focused recipes: index sources, add subtitles, label tracks with roles, run
  the server, serve from S3, deploy with Docker.
- **[Reference](./reference/cli.md)** — _I need to look something up._ Exact,
  exhaustive descriptions of every command, route, config key, descriptor field,
  and track role.
- **[Explanation](./explanation/thin-pointer.md)** — _I want to understand why._
  The design ideas behind dyndo: dynamic packaging without media copies, the
  thin pointer, and bounded-memory parsing.

## Supported codecs

Codec parameters are read from the source and emitted as
[RFC 6381](https://datatracker.ietf.org/doc/html/rfc6381) strings.

| Media | Codec                       | Sample entry   |
| ----- | --------------------------- | -------------- |
| Video | AVC / H.264                 | `avc1`         |
| Video | HEVC / H.265                | `hvc1`, `hev1` |
| Video | AV1                         | `av01`         |
| Audio | AAC                         | `mp4a`         |
| Audio | Dolby Digital (AC-3)        | `ac-3`         |
| Audio | Dolby Digital Plus (E-AC-3) | `ec-3`         |
| Text  | WebVTT in ISO-BMFF          | `wvtt`         |

Raw WebVTT (`.vtt`) files are also accepted as text-track sources: dyndo parses
one and packages a `wvtt` track from it as it reads, so the `.vtt` stays the
source of truth. DASH is served that packaged track, while HLS is served plain
WebVTT documents — see [Add a subtitle track](./how-to/add-subtitles.md).
