# dyndo-server

The dynamic packaging HTTP server for [`dyndo`](../../README.md), built on
[Axum](https://github.com/tokio-rs/axum). It serves DASH and HLS streams straight
from your CMAF sources: at request time it reads each source's header through
[`dyndo-core`](../dyndo-core/README.md), renders the manifest with the same
crate's `dash`/`hls` modules, and streams init/media segments via byte-range
reads — nothing is pre-packaged to disk.

## Running

```bash
cargo run        # or, from the repo root: make run
# dyndo-server listening on http://0.0.0.0:8080
```

Descriptors are read from `./assets`. Point a player at either manifest:

```
http://localhost:8080/asset.json/dash/index.mpd    # DASH
http://localhost:8080/asset.json/hls/index.m3u8     # HLS
```

## Routes

Every stream lives under `/<asset>/<protocol>/<resource>`, where `<asset>` is the
descriptor path relative to the assets root and `<repr>` is a representation `id`
from the descriptor (e.g. `video_avc1_1080_4807228`).

| Method | Route | Description |
|---|---|---|
| `GET` | `/<asset>/dash/index.mpd` | The asset's DASH manifest (MPD). |
| `GET` | `/<asset>/hls/index.m3u8` | The asset's HLS multivariant playlist. |
| `GET` | `/<asset>/hls/<repr>.m3u8` | An HLS rendition's media playlist. |
| `GET` | `/<asset>/<protocol>/<repr>/init.mp4` | A representation's initialization segment. |
| `GET` | `/<asset>/<protocol>/<repr>/<time>.m4s` | The media segment starting at presentation `time`. |

Media segments are the same CMAF bytes under either `<protocol>` prefix, so only
the manifest resource is protocol-specific — the `init.mp4` and `<time>.m4s`
segment routes are shared.

## Configuration

| Setting | Value | Where |
|---|---|---|
| Listen port | `8080` | compile-time constant in [`main.rs`](src/main.rs) |
| Assets root | `./assets` | compile-time constant in [`main.rs`](src/main.rs) |
| CORS | any origin, any method | applied in [`routes`](src/routes/mod.rs) |

## Errors

A missing object (descriptor or source file) is a `404`. Every other core
failure — malformed descriptor JSON, an unreadable, unsupported, or
descriptor-mismatched CMAF source, other I/O — maps to a `500`, because the
asset files are server-owned and a bad one is our problem, not the client's. An
unknown representation or a malformed segment path is a `404` / `400`.
