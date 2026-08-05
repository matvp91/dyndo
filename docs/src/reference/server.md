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

Every descriptor in the storage backend is addressable under the server's
`/out/` tree, as both a DASH and an HLS stream over one shared set of segment
routes:

```text
GET /out/(asset:demo)/index.mpd      # DASH manifest
GET /out/(asset:demo)/master.m3u8    # HLS multivariant playlist
```

The bracketed part is a Rison object naming the descriptor and, optionally,
overriding how it is segmented for that request. Manifests are generated per
request by parsing each source's CMAF header; media segments are returned as
byte-range reads from the original files. Nothing is written back to storage.

There is no explicit listing route: an asset is served if a descriptor exists at
the path a request names.

## In this section

- [HTTP routes](./server/routes.md) — the complete route table, the Rison
  options object, content types, and status codes.
- [Configuration](./server/configuration.md) — the config schema, layering, and
  environment-variable mapping.
