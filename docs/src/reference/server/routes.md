# HTTP routes

The server exposes two things: a health probe, and a single output tree under
`/out/`. All routes are `GET`.

```text
GET /health
GET /out/<options>/<resource>
```

`<options>` is a **Rison** object — a compact, URL-friendly object notation, so
`(asset:demo,sml:6000)` is the equivalent of the JSON
`{"asset":"demo","sml":6000}` — carrying the asset to serve and
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
/out/(asset:movies%2Fbig-buck-bunny)/index.mpd
/out/(asset:demo,sml:6000)/master.m3u8
```

The options object must occupy one URL path segment. Percent-encode `/` as
`%2F` when an option value contains a nested path. The server decodes the value
before parsing the Rison object.

### Common options

| Full key | Shorthand | Type | Description |
|---|---|---|---|
| `asset` | `a` | string | **Required.** Path to the descriptor, relative to the storage root, **without** the `.json` extension. |

### Segmentation options

| Full key | Shorthand | Type | Default | Description |
|---|---|---|---|---|
| `min_length` | `sml`, `segment_min_length` | integer | `0` | Minimum served segment length in milliseconds. Whole fragments are grouped until this length is reached. |
| `text_length` | `stl`, `segment_text_length` | integer | `0` | Length of each segment of a subtitle track dyndo packages from a `.vtt`, in milliseconds. Exact, not a minimum. `0` cuts one only at the splice points. |
| `boundaries` | `sb`, `segment_boundaries` | array of integers | `[]` | Splice points in milliseconds; a served segment never spans one. |

Each of these overrides the matching option in the descriptor's
[`segment_options`](../asset-json.md#segmentation) block. An option left at zero —
or an empty boundary list — names nothing and leaves the asset's value standing,
since a request cannot express the difference between an absent value and a zero
one.

### Transport options

DASH resources accept one transport-specific option:

| Full key | Shorthand | Type | Default | Description |
|---|---|---|---|---|
| `compact` | `c` | boolean | `false` | Hoist segment-template data shared by DASH representations to their adaptation set. |

HLS currently has no transport-specific options.

The supported shorthand map is therefore `asset` → `a`, `min_length` → `sml`,
`text_length` → `stl`, `boundaries` → `sb`, and `compact` → `c`. The forms are
equivalent:

```text
/out/(asset:demo,min_length:6000,compact:!t)/index.mpd
/out/(a:demo,sml:6000,c:!t)/index.mpd
```

Unknown keys are rejected for DASH and HLS manifest requests.

### The `asset` key

The server appends `.json` to the value, so `asset:demo` reads `demo.json` from
the storage root. The decoded value may contain slashes, letting descriptors sit
in nested directories. For example, `asset:movies%2Fbig` reads
`movies/big.json`. OpenDAL determines how the resulting path is resolved by the
configured storage backend.

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
different `min_length` values than the manifest
was generated with will usually `404` — which is why the options live in the
shared path prefix.

## Status codes

| Code | When |
|---|---|
| `200 OK` | The manifest or segment was generated and returned; also the `/health` probe. |
| `400 Bad Request` | The options path segment is malformed Rison, a DASH or HLS manifest request carries an unknown option, or a segment length is negative. |
| `404 Not Found` | The path does not contain separate options and resource components; `<track-id>` matches no track; a segment filename is not `<integer>.m4s`; `<time>` is not a segment boundary; or the descriptor does not exist. |
| `500 Internal Server Error` | The descriptor JSON is malformed; a source file is unreadable or is not valid, supported CMAF; or manifest serialization failed. |

The split between `404` and `500` reflects ownership: a **missing** object is
treated as a client addressing error, while a **malformed** descriptor or
**broken** media file is the server's own content problem, because the asset
files are server-owned. Error responses carry a short plain-text message.

## CORS

The server applies a permissive CORS policy — any origin, any method — so
browser-based players can load manifests and segments cross-origin during
development.
