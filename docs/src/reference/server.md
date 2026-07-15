# dyndo-server

`dyndo-server` is the dynamic packaging HTTP server, produced by the
`dyndo-server` crate and built on [Axum](https://github.com/tokio-rs/axum). It
serves DASH and HLS manifests and CMAF segments generated on the fly from
`asset.json` descriptors.

## Running

```text
dyndo-server
```

The server takes no command-line arguments; all settings come from configuration
(see [Configuration](./server/configuration.md)). On startup it:

1. loads configuration (defaults, then `config.yaml`, then `DYNDO_*`
   environment variables);
2. builds the storage operator for the selected backend; and
3. binds the configured address and begins serving.

```text
dyndo-server listening on http://0.0.0.0:8080
```

If configuration cannot be loaded, or the selected storage backend is
misconfigured, the server exits during startup rather than serving errors per
request.

## What it serves

For every descriptor in the storage backend, the server exposes both a DASH and
an HLS stream over a shared set of segment routes. Manifests are generated per
request by parsing each source's CMAF header; media segments are returned as
byte-range reads from the original files. Nothing is written back to storage.

## In this section

- [HTTP routes](./server/routes.md) — the complete route table, path grammar,
  content types, and status codes.
- [Configuration](./server/configuration.md) — the config schema, layering, and
  environment-variable mapping.
