# Deploy with Docker

`dyndo-server` ships as a container image, so you can run it anywhere Docker
runs — no Rust toolchain required. One image serves from either storage
backend; you pick the backend at runtime with configuration.

## Run the published image

Released versions are published to Docker Hub as `<namespace>/dyndo-server`.
Pull and run it against a local directory of assets:

```bash
docker run --rm -p 8080:8080 \
  -e DYNDO_FS__ROOT=/assets \
  -v "$PWD/assets:/assets:ro" \
  <namespace>/dyndo-server
```

- `-p 8080:8080` publishes the port. The server binds `0.0.0.0` inside the
  container, so it is reachable from the host with no extra configuration.
- `-v "$PWD/assets:/assets:ro"` mounts your assets read-only — the server only
  reads them.
- `-e DYNDO_FS__ROOT=/assets` points the filesystem backend at the mount. `fs`
  is the default store, so its root is the only setting you must supply.

Then request a stream just as you would from a local server:

```text
http://localhost:8080/asset.json/dash/index.mpd    # DASH
http://localhost:8080/asset.json/hls/index.m3u8     # HLS
```

Pin a version with a tag — `:0.3.0`, `:0.3`, or `:latest`:

```bash
docker run --rm -p 8080:8080 -e DYNDO_FS__ROOT=/assets \
  -v "$PWD/assets:/assets:ro" <namespace>/dyndo-server:0.3.0
```

## Serve from S3 instead

Backed by S3 the container is stateless — no volume, all configuration through
environment variables:

```bash
docker run --rm -p 8080:8080 \
  -e DYNDO_STORE=s3 \
  -e DYNDO_S3__BUCKET=my-assets \
  -e DYNDO_S3__REGION=eu-west-1 \
  -e DYNDO_S3__ENDPOINT=https://s3.eu-west-1.amazonaws.com \
  -e AWS_ACCESS_KEY_ID=AKIA... \
  -e AWS_SECRET_ACCESS_KEY=... \
  <namespace>/dyndo-server
```

The values above are placeholders — substitute your own bucket, region,
endpoint, and credentials. See [Serve media from S3](./serve-from-s3.md) for how
each setting maps, and prefer a secrets manager over inline credentials in
production.

## Build the image yourself

The repository ships a `Dockerfile`, so you can build from source instead of
pulling:

```bash
docker build -t dyndo-server .
docker run --rm -p 8080:8080 -e DYNDO_FS__ROOT=/assets \
  -v "$PWD/assets:/assets:ro" dyndo-server
```

It is a multi-stage build: a Rust stage compiles `dyndo-server`, and a slim
Debian runtime carries just the binary and CA certificates (needed for S3 over
TLS). The container runs as an unprivileged user.

## Use a config file instead of environment variables

To keep settings in a file, mount a `config.yaml` and point the server at it
with `DYNDO_CONFIG`:

```bash
docker run --rm -p 8080:8080 \
  -v "$PWD/config.yaml:/etc/dyndo/config.yaml:ro" \
  -v "$PWD/assets:/assets:ro" \
  -e DYNDO_CONFIG=/etc/dyndo/config.yaml \
  <namespace>/dyndo-server
```

Environment variables still override the file, so you can bake defaults into
`config.yaml` and override per environment.

## Next steps

- Configuration schema and precedence:
  [Configuration reference](../reference/server/configuration.md).
- Running without a container:
  [Run and configure the server](./run-the-server.md).
- Object-storage details: [Serve media from S3](./serve-from-s3.md).
