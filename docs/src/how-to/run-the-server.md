# Run and configure the server

This guide covers starting `dyndo-server` and pointing it at your media. For the
complete configuration schema and precedence rules, see the
[Configuration reference](../reference/server/configuration.md).

## Start the server

From the repository:

```bash
make run        # equivalent to: cargo run -p dyndo-server
```

```text
dyndo-server listening on http://0.0.0.0:8080
```

The server binds `0.0.0.0:8080` by default; the repository's `config.yaml`
points it at a local `./assets` directory. (There is no built-in storage root —
whichever backend `store` selects must be given one, or the server exits at
startup.) Request a stream at
`/<asset>/<protocol>/<resource>`, where `<asset>` is the descriptor's path
relative to the storage root:

```text
http://localhost:8080/asset.json/dash/index.mpd    # DASH
http://localhost:8080/asset.json/hls/index.m3u8     # HLS
```

Nested descriptors work too: a descriptor at `assets/movies/big/asset.json` is
served at `/movies/big/asset.json/dash/index.mpd`. See the
[HTTP routes reference](../reference/server/routes.md) for the full route table.

## Configure with `config.yaml`

The server reads a `config.yaml` from its working directory. The repository
ships one:

```yaml
store: fs

server:
  host: 0.0.0.0
  port: 8080

fs:
  root: ./assets
```

- `store` selects the storage backend: `fs` (local disk) or `s3`.
- `server.host` / `server.port` set the listen address.
- The `fs:` section configures the local-filesystem backend; its `root` is the
  directory descriptors and media are read from.

To serve a different directory on a different port, edit those values:

```yaml
server:
  port: 9000
fs:
  root: /srv/media
```

## Override with environment variables

Every setting can also be supplied through a `DYNDO_`-prefixed environment
variable. Nested keys use a **double underscore** (`__`) as the separator, so
single underscores inside a field name survive:

```bash
DYNDO_SERVER__PORT=9000 DYNDO_FS__ROOT=/srv/media make run
```

Environment variables take precedence over `config.yaml`, which in turn
overrides the built-in defaults. This lets you keep a checked-in `config.yaml`
and override just what differs per environment.

## Use a config file elsewhere

Point the server at a specific config file with `DYNDO_CONFIG`:

```bash
DYNDO_CONFIG=/etc/dyndo/prod.yaml make run
```

When `DYNDO_CONFIG` is set, the named file **must exist** or the server exits
with an error. (Without it, a missing `config.yaml` is fine — the server falls
back to defaults and environment variables.)

## Serving to browser players

The server sends permissive CORS headers (any origin, any method), so a
browser-based player can load a manifest during development without a proxy.

## Next steps

- Run the server as a container: [Deploy with Docker](./deploy-with-docker.md).
- Serve from object storage: [Serve media from S3](./serve-from-s3.md).
- Full configuration schema and precedence:
  [Configuration reference](../reference/server/configuration.md).
- Every route and status code:
  [HTTP routes reference](../reference/server/routes.md).
