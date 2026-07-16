# dyndo-server

The dynamic packaging HTTP server for [`dyndo`](../../README.md), built on
[Axum](https://github.com/tokio-rs/axum). It serves DASH and HLS streams straight
from your CMAF sources: at request time it reads each source's header through
[`dyndo-core`](../dyndo-core/README.md), renders the manifest with the same
crate's `dash`/`hls` modules, and streams init/media segments via byte-range
reads — nothing is pre-packaged to disk.

Full documentation lives in the book: the
**[dyndo-server reference](https://matvp91.github.io/dyndo/reference/server.html)**
covers the HTTP routes and the configuration schema, and there are how-to guides
for [running the server](https://matvp91.github.io/dyndo/how-to/run-the-server.html),
[serving from S3](https://matvp91.github.io/dyndo/how-to/serve-from-s3.html), and
[deploying with Docker](https://matvp91.github.io/dyndo/how-to/deploy-with-docker.html).

## Running

```bash
cargo run        # or, from the repo root: make run
# dyndo-server listening on http://0.0.0.0:8080
```

Releases are also published as a container image,
[`matvp91/dyndo-server`](https://hub.docker.com/r/matvp91/dyndo-server) on
Docker Hub.
