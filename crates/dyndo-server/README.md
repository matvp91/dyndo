# dyndo-server

The dynamic packaging HTTP server for [`dyndo`](../../README.md), built on
[Axum](https://github.com/tokio-rs/axum). It serves DASH and HLS streams straight
from your CMAF sources: at request time it reads each source's header through
[`dyndo-core`](../dyndo-core/README.md), renders manifests with
[`dyndo-dash`](../dyndo-dash/README.md) and
[`dyndo-hls`](../dyndo-hls/README.md), and streams init/media segments via
byte-range reads — nothing is pre-packaged to disk.

## Routes

```text
GET /health
GET /out/<rison-options>/<resource>
```

The options object names the descriptor and may override its segmentation for
that request; `<resource>` is `index.mpd`, `master.m3u8`, `<track-id>.m3u8`,
`<track-id>/init.mp4`, or `<track-id>/<time>.m4s`. Segment resources carry no
protocol component — both manifests reference the same segment URLs.

```bash
curl "http://localhost:8080/out/(asset:demo)/index.mpd"
curl "http://localhost:8080/out/(asset:demo,min_segment_length:6000)/master.m3u8"
```

## Configuration

Layered: built-in defaults, then `config.yaml` (or `DYNDO_CONFIG`), then
`DYNDO_*` environment variables. Storage is an OpenDAL backend selected by
`store` (`fs` or `s3`), configured with OpenDAL's own field names — nested keys
use a double underscore, so `DYNDO_S3__ACCESS_KEY_ID` sets `s3.access_key_id`.
There are no command-line flags.

## Running

```bash
cargo run        # or, from the repo root: make run
# dyndo-server listening on http://0.0.0.0:8080
```

Releases are also published as a container image,
[`matvp91/dyndo-server`](https://hub.docker.com/r/matvp91/dyndo-server) on
Docker Hub.

Full documentation lives in the book: the
**[dyndo-server reference](https://matvp91.github.io/dyndo/reference/server.html)**
covers the HTTP routes and the configuration schema, and there are how-to guides
for [running the server](https://matvp91.github.io/dyndo/how-to/run-the-server.html),
[serving from S3](https://matvp91.github.io/dyndo/how-to/serve-from-s3.html), and
[deploying with Docker](https://matvp91.github.io/dyndo/how-to/deploy-with-docker.html).
