# Deploy with Docker

Every release of `dyndo-server` is published to Docker Hub as
[`matvp91/dyndo-server`](https://hub.docker.com/r/matvp91/dyndo-server), so you
can run it anywhere Docker runs — no Rust toolchain required. The image is
multi-arch (amd64 and arm64), and one image serves from either storage backend;
you pick the backend at runtime with configuration.

## Quick start: run it locally

This is the fastest way to get dyndo running. You need a directory containing
an `asset.json` and the CMAF sources it points at — if you don't have one yet,
the [Getting started tutorial](../tutorial/getting-started.md) shows how to
create it. Then one command starts the server:

```bash
docker run --rm -p 8080:8080 \
  -e DYNDO_FS__ROOT=/assets \
  -v "$PWD/assets:/assets:ro" \
  matvp91/dyndo-server
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

## Pin a version

Releases are tagged three ways on Docker Hub: the full version
(`:<major>.<minor>.<patch>`), the minor line (`:<major>.<minor>`), and
`:latest`. Pick a version from the
[tags page](https://hub.docker.com/r/matvp91/dyndo-server/tags) and pin the
full version in production so a new release never changes what you run:

```bash
docker run --rm -p 8080:8080 -e DYNDO_FS__ROOT=/assets \
  -v "$PWD/assets:/assets:ro" matvp91/dyndo-server:<version>
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
  matvp91/dyndo-server
```

The values above are placeholders — substitute your own bucket, region,
endpoint, and credentials. See [Serve media from S3](./serve-from-s3.md) for how
each setting maps, and prefer a secrets manager over inline credentials in
production.

## Use a config file instead of environment variables

To keep settings in a file, mount a `config.yaml` and point the server at it
with `DYNDO_CONFIG`:

```bash
docker run --rm -p 8080:8080 \
  -v "$PWD/config.yaml:/etc/dyndo/config.yaml:ro" \
  -v "$PWD/assets:/assets:ro" \
  -e DYNDO_CONFIG=/etc/dyndo/config.yaml \
  matvp91/dyndo-server
```

Environment variables still override the file, so you can bake defaults into
`config.yaml` and override per environment.

## Health checks

The server answers `GET /health` with `200 OK`. Point a Kubernetes liveness or
readiness probe — or an external load balancer — at it:

```yaml
# Kubernetes pod spec
livenessProbe:
  httpGet:
    path: /health
    port: 8080
```

The runtime image is deliberately minimal — just the binary and CA
certificates, with no HTTP client — so prefer an out-of-container probe
like the above over a `HEALTHCHECK` that would need `curl` inside the
container.

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

## Next steps

- Configuration schema and precedence:
  [Configuration reference](../reference/server/configuration.md).
- Running without a container:
  [Run and configure the server](./run-the-server.md).
- Object-storage details: [Serve media from S3](./serve-from-s3.md).
