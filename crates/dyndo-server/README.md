# dyndo-server

The dynamic packaging HTTP server for [`dyndo`](../../README.md), built on
[Axum](https://github.com/tokio-rs/axum). It serves DASH streams straight from
your CMAF sources: at request time it reads each source's header through
[`dyndo-core`](../dyndo-core/README.md), renders the manifest with
[`dyndo-dash`](../dyndo-dash/README.md), and streams init/media segments via
byte-range reads — nothing is pre-packaged to disk.

## Running

```bash
cargo run        # or, from the repo root: make run
# dyndo-server listening on http://0.0.0.0:8080
```

Descriptors are read from `./assets`. Point a DASH player at the manifest:

```
http://localhost:8080/asset.json/dash/index.mpd
```

## Routes

Every stream lives under `/<asset>/<protocol>/<resource>`, where `<asset>` is the
descriptor path relative to the assets root and `<repr>` is a representation `id`
from the descriptor (e.g. `video_avc1_1080_4807228`).

| Method | Route | Description |
|---|---|---|
| `GET` | `/<asset>/dash/index.mpd` | The asset's DASH manifest (MPD). |
| `GET` | `/<asset>/dash/<repr>/init.mp4` | A representation's initialization segment. |
| `GET` | `/<asset>/dash/<repr>/<time>.m4s` | The media segment starting at presentation `time`. |

Media segments are the same CMAF bytes for any protocol, so only the manifest
route is DASH-specific. Adding HLS means registering the protocol and a sibling
manifest handler — the segment routes are reused as-is.

## Configuration

| Setting | Value | Where |
|---|---|---|
| Listen port | `8080` | compile-time constant in [`main.rs`](src/main.rs) |
| Assets root | `./assets` | compile-time constant in [`main.rs`](src/main.rs) |
| CORS | any origin, any method | applied in [`routes`](src/routes/mod.rs) |

## Errors

A missing descriptor is a `404`; malformed JSON or other I/O is a `500`. CMAF
parse/read failures panic in the core by design and never surface as a response.
