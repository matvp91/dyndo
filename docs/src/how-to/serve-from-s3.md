# Serve media from S3

All of dyndo's I/O goes through [OpenDAL](https://opendal.apache.org/), so the
server can read descriptors and media from Amazon S3 (or an S3-compatible store)
instead of the local filesystem. Nothing about your assets changes — the same
`asset.json` and CMAF files work unmodified, because paths inside a descriptor
are relative and backend-agnostic.

## Switch the backend to S3

Set `store: s3` and add an `s3:` section to `config.yaml`:

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

- `bucket` is **required** — the operator fails to build without it.
- `region` and `endpoint` identify the S3 service.
- `root` is a prefix prepended to every object key; use it to serve assets from
  a subdirectory of the bucket.

With `store: s3`, an asset at key `asset.json` is served exactly as before:

```text
http://localhost:8080/asset.json/dash/index.mpd
```

## Provide credentials

Credentials do not have to live in `config.yaml`. The S3 backend picks up the
standard AWS environment variables, so you can keep secrets out of the file:

```bash
export AWS_ACCESS_KEY_ID=AKIA…
export AWS_SECRET_ACCESS_KEY=…
make run
```

You can also set them through dyndo's own configuration if you prefer. Because
every S3 setting maps to a `DYNDO_S3__*` environment variable (double underscore
between segments, single underscores preserved within a name), credentials can
be injected per environment:

```bash
DYNDO_S3__ACCESS_KEY_ID=AKIA… DYNDO_S3__SECRET_ACCESS_KEY=… make run
```

> Prefer the AWS environment variables or a secrets manager for credentials in
> production; keep `config.yaml` free of secrets so it can be checked in.

## Verify the backend

If the selected store is misconfigured — an `s3` store with no `bucket`, or an
`fs` store with no `root` — the server fails at startup while building the
storage operator, rather than returning errors per request. A clean start means
the backend is wired up correctly.

## Next steps

- All S3 keys and how they map to environment variables:
  [Configuration reference](../reference/server/configuration.md).
- General server operation:
  [Run and configure the server](./run-the-server.md).
