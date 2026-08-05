# HTTP routes

The server exposes two things: a health probe, and a single output tree under
`/out/`. All routes are `GET`.

```text
GET /health
GET /out/<options>/<resource>
```

`<options>` is a **Rison** object — a compact, URL-friendly object notation, so
`(asset:demo,msl:6000)` is the equivalent of the JSON
`{"asset":"demo","msl":6000}` — carrying the asset to serve and
how to segment it. `<resource>` names what to return from it.
Because the options travel in the path rather than a query string, a manifest
and every segment it references share one prefix, and the relative URLs inside a
manifest resolve correctly without rewriting.

## Health check

| Path | Description | Content-Type |
|---|---|---|
| `/health` | Liveness probe. Returns `200 OK` with an empty body. | *(none)* |

`/health` is a fixed route registered ahead of the `/out/` tree, so it can never
be shadowed by an asset. Use it for container and load-balancer health checks;
see [Deploy with Docker](../../how-to/deploy-with-docker.md).

## The options object

The first path segment after `/out/` must be a Rison object, beginning with `(`
and ending at its matching `)`:

```text
/out/(asset:movies/big-buck-bunny)/index.mpd
/out/(asset:demo,msl:6000)/master.m3u8
```

| Key | Type | Description |
|---|---|---|
| `asset` | string | **Required.** Path to the descriptor, relative to the storage root, **without** the `.json` extension. |
| `a` | string | Alias for `asset`. |
| `min_segment_length` | integer | Minimum served segment length in milliseconds. Whole fragments are grouped until this length is reached. Defaults to `0`. |
| `msl` | integer | Alias for `min_segment_length`. |

Unknown keys are rejected. Segment boundaries are asset-specific and can only
be supplied by the descriptor.

### The `asset` key

The server appends `.json` to the value, so `asset:demo` reads `demo.json` from
the storage root and `asset:movies/big` reads `movies/big.json`. The value may
contain slashes, letting descriptors sit in nested directories.

It is rejected with `400` when it is empty, starts or ends with `/`, ends with
`.json`, contains a backslash, or contains an empty, `.`, or `..` path
component. That last rule keeps a request from escaping the storage root.

```text
/out/(asset:demo.json)/index.mpd      → 400  invalid asset path: demo.json
/out/(asset:../secrets)/index.mpd     → 400  invalid asset path: ../secrets
```

## Resources

`<resource>` is everything after the options object.

| Resource | Description | Content-Type |
|---|---|---|
| `index.mpd` | The DASH manifest (MPD). | `application/dash+xml` |
| `master.m3u8` | The HLS multivariant playlist. | `application/vnd.apple.mpegurl` |
| `<track-id>.m3u8` | One track's HLS media playlist. | `application/vnd.apple.mpegurl` |
| `<track-id>/init.mp4` | A track's CMAF initialization segment. | `video/mp4`, `audio/mp4`, or `application/mp4` |
| `<track-id>/<time>.m4s` | The media segment starting at presentation `<time>`. | `video/mp4`, `audio/mp4`, or `application/mp4` |

`<track-id>` is a track's `id` exactly as recorded in the descriptor (for
example `video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe`). Because manifests emit
these same relative URLs, players never construct them by hand.

A full set of requests for one asset:

```text
/out/(asset:demo)/index.mpd
/out/(asset:demo)/master.m3u8
/out/(asset:demo)/video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe.m3u8
/out/(asset:demo)/video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe/init.mp4
/out/(asset:demo)/video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe/0.m4s
```

## Segments are protocol-independent

There is no `dash` or `hls` component anywhere in a segment path. Both manifests
reference the same `<track-id>/init.mp4` and `<track-id>/<time>.m4s` URLs, and a
request for one returns the same CMAF bytes regardless of which manifest sent
the player there. Only `index.mpd` and the `.m3u8` resources are
protocol-specific. See
[One source, two protocols](../../explanation/two-protocols.md) for why.

## Segment addressing

A media segment is addressed by its **presentation start time**, an integer in
the track's own timescale, with a `.m4s` extension. The server re-derives the
track's segment list for the request's segmentation options, then walks it
accumulating durations from the track's earliest presentation time until it
finds an exact match.

A track whose `sidx` reports an earliest presentation time of `0` therefore
starts at `<track-id>/0.m4s`, and each subsequent segment starts at the running
sum of the preceding durations. These are exactly the `$Time$` values in the
DASH `SegmentTimeline` and the URIs in the HLS media playlists.

Because segmentation options change where segment edges fall, a `<time>` is only
valid for the same options that produced it. Requesting a segment under
different `min_segment_length` values than the manifest
was generated with will usually `404` — which is why the options live in the
shared path prefix.

## Status codes

| Code | When |
|---|---|
| `200 OK` | The manifest or segment was generated and returned; also the `/health` probe. |
| `400 Bad Request` | The path does not begin with a Rison object, the object is malformed or has an unmatched `)`, it carries an unknown key, `asset` is invalid, or `min_segment_length` is negative. |
| `404 Not Found` | No resource followed the options object; `<track-id>` matches no track; a segment filename is not `<integer>.m4s`; `<time>` is not a segment boundary; or the descriptor does not exist. |
| `500 Internal Server Error` | The descriptor JSON is malformed; a source file is unreadable or is not valid, supported CMAF; or manifest serialization failed. |

The split between `404` and `500` reflects ownership: a **missing** object is
treated as a client addressing error, while a **malformed** descriptor or
**broken** media file is the server's own content problem, because the asset
files are server-owned. Error responses carry a short plain-text message.

## CORS

The server applies a permissive CORS policy — any origin, any method — so
browser-based players can load manifests and segments cross-origin during
development.
