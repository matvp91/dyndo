# Run and configure the server

This guide covers how `dyndo-server` is configured and how it behaves once
running. For the container recipes themselves — pinning versions, mounting a
config file, building the image — see
[Deploy with Docker](./deploy-with-docker.md). For the complete schema and
precedence rules, see the
[Configuration reference](../reference/server/configuration.md).

## Start the server

The server ships as the
[`matvp91/dyndo-server`](https://hub.docker.com/r/matvp91/dyndo-server) image.
The one setting you must supply is the **storage root** — the directory (or
bucket) your descriptors and media live in. There is no built-in default; if the
selected backend has no root, the server exits at startup.

```bash
docker run --rm -p 8080:8080 \
  -e DYNDO_FS__ROOT=/assets \
  -v "$PWD/assets:/assets:ro" \
  matvp91/dyndo-server
```

```text
dyndo-server listening on http://0.0.0.0:8080
```

Here the `fs` backend (the default) reads from `/assets`, which is your local
`./assets` mounted into the container. Running from a source build instead? The
`dyndo-server` binary reads the same configuration — see
[Build from source](./build-from-source.md).

## Request a stream

Request a stream at `/<asset>/<protocol>/<resource>`, where `<asset>` is the
descriptor's path relative to the storage root:

```text
http://localhost:8080/asset.json/dash/index.mpd    # DASH
http://localhost:8080/asset.json/hls/index.m3u8     # HLS
```

Nested descriptors work too: a descriptor at `assets/movies/big/asset.json`
(with storage root `assets/`) is served at
`/movies/big/asset.json/dash/index.mpd`. See the
[HTTP routes reference](../reference/server/routes.md) for the full route table.

## Configure with a file or environment variables

The server layers three sources, each overriding the one before it: built-in
defaults, then a `config.yaml`, then `DYNDO_`-prefixed environment variables.

A `config.yaml` looks like this:

```yaml
store: fs

server:
  host: 0.0.0.0
  port: 8080

fs:
  root: /assets
```

- `store` selects the storage backend: `fs` (local disk) or `s3`.
- `server.host` / `server.port` set the listen address.
- The `fs:` section configures the local-filesystem backend; its `root` is the
  directory descriptors and media are read from.

To use a file, mount it and point the server at it with `DYNDO_CONFIG` (see
[Deploy with Docker](./deploy-with-docker.md#use-a-config-file-instead-of-environment-variables)).

Every setting also maps to a `DYNDO_`-prefixed environment variable. Nested keys
use a **double underscore** (`__`) as the separator, so single underscores
inside a field name survive:

```bash
docker run --rm -p 8080:8080 \
  -e DYNDO_SERVER__PORT=9000 \
  -e DYNDO_FS__ROOT=/assets \
  -v "$PWD/assets:/assets:ro" \
  matvp91/dyndo-server
```

Environment variables take precedence over `config.yaml`, which in turn
overrides the built-in defaults — so you can bake a `config.yaml` into an image
and override just what differs per environment.

## Point at a specific config file

By default the server looks for `config.yaml` in its working directory. Set
`DYNDO_CONFIG` to load a different path:

```bash
-e DYNDO_CONFIG=/etc/dyndo/prod.yaml
```

When `DYNDO_CONFIG` is set, the named file **must exist** or the server exits
with an error. Without it, a missing `config.yaml` is fine — the server falls
back to defaults and environment variables.

## Health checks

The server answers `GET /health` with `200 OK`. Use it as a container or
load-balancer liveness probe; it never collides with a stream route. See
[Deploy with Docker](./deploy-with-docker.md#health-checks) for wiring it into an
orchestrator.

## Serving to browser players

The server sends permissive CORS headers (any origin, any method), so a
browser-based player can load a manifest during development without a proxy.

## Next steps

- Container recipes and production tips:
  [Deploy with Docker](./deploy-with-docker.md).
- Serve from object storage: [Serve media from S3](./serve-from-s3.md).
- Full configuration schema and precedence:
  [Configuration reference](../reference/server/configuration.md).
- Every route and status code:
  [HTTP routes reference](../reference/server/routes.md).
