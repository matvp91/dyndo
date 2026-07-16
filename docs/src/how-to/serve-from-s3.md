# Serve media from S3

All of dyndo's I/O goes through [OpenDAL](https://opendal.apache.org/), so the
server can read descriptors and media from Amazon S3 (or an S3-compatible store)
instead of the local filesystem. Nothing about your assets changes — the same
`asset.json` and CMAF files work unmodified, because paths inside a descriptor
are relative and backend-agnostic.

Backed by S3 the server is **stateless**: there's no volume to mount, and all
configuration comes through environment variables.

## Run against S3

Select the `s3` store and give it a bucket, region, endpoint, and credentials:

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

- `DYNDO_S3__BUCKET` is **required** — the server fails to start without it.
- `DYNDO_S3__REGION` and `DYNDO_S3__ENDPOINT` identify the S3 service.
- Every S3 setting maps to a `DYNDO_S3__*` variable (double underscore between
  segments, single underscores preserved within a name), so
  `DYNDO_S3__ACCESS_KEY_ID` sets `s3.access_key_id`.

With `store: s3`, an asset at key `asset.json` is served exactly as it is from
local disk:

```text
http://localhost:8080/asset.json/dash/index.mpd
```

Set `DYNDO_S3__ROOT` to prepend a key prefix if your assets live under a
subdirectory of the bucket.

## The same, from a config file

If you'd rather keep settings in a file, the equivalent `config.yaml` is:

```yaml
store: s3

server:
  host: 0.0.0.0
  port: 8080

s3:
  bucket: my-assets
  region: eu-west-1
  endpoint: https://s3.eu-west-1.amazonaws.com
  root: /
```

Mount it and point the server at it with `DYNDO_CONFIG` — see
[Deploy with Docker](./deploy-with-docker.md#use-a-config-file-instead-of-environment-variables).

## Credentials

The S3 backend reads the standard AWS environment variables
(`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`), so credentials never have to live
in `config.yaml`. You can also supply them through dyndo's own configuration
(`DYNDO_S3__ACCESS_KEY_ID`, `DYNDO_S3__SECRET_ACCESS_KEY`).

> Prefer the AWS environment variables or a secrets manager for credentials in
> production; keep any checked-in `config.yaml` free of secrets.

## Verify the backend

If the selected store is misconfigured — an `s3` store with no `bucket`, or an
`fs` store with no `root` — the server fails at startup while building the
storage operator, rather than returning errors per request. A clean
`listening on …` line means the backend is wired up correctly.

## Next steps

- All S3 keys and how they map to environment variables:
  [Configuration reference](../reference/server/configuration.md).
- General server operation:
  [Run and configure the server](./run-the-server.md).
- Container recipes: [Deploy with Docker](./deploy-with-docker.md).
