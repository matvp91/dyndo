# HTTP routes

Every stream lives under `/<asset>/<protocol>/<resource>`. All routes are `GET`.

## Route table

| Path | Description | Content-Type |
|---|---|---|
| `/<asset>/dash/index.mpd` | The asset's DASH manifest (MPD). | `application/dash+xml` |
| `/<asset>/hls/index.m3u8` | The asset's HLS multivariant (master) playlist. | `application/vnd.apple.mpegurl` |
| `/<asset>/hls/<repr>.m3u8` | An HLS rendition's media playlist. | `application/vnd.apple.mpegurl` |
| `/<asset>/<protocol>/<repr>/init.mp4` | A representation's initialization segment. | `video/mp4`, `audio/mp4`, or `application/mp4` |
| `/<asset>/<protocol>/<repr>/<time>.m4s` | The media segment starting at presentation `<time>`. | `video/mp4`, `audio/mp4`, or `application/mp4` |

A raw-WebVTT text track answers on the same segment routes with `text/vtt` (its
`init.mp4` is empty — a raw source has no initialization segment), though such
tracks are not yet referenced by the generated manifests.

## Health check

Separate from the streaming routes, the server answers a liveness probe:

| Path | Description | Content-Type |
|---|---|---|
| `/health` | Liveness probe. Returns `200 OK` with an empty body. | *(none)* |

`/health` is a fixed route registered ahead of the catch-all, so it never
shadows a stream — every streaming route carries a `/dash/` or `/hls/` infix.
Use it for container and load-balancer health checks; see
[Deploy with Docker](../../how-to/deploy-with-docker.md).

## Path grammar

- **`<asset>`** — the descriptor's path relative to the storage root. It may
  contain slashes: a descriptor at `assets/movies/big/asset.json` (with
  storage root `assets/`) is addressed as `movies/big/asset.json`. The full
  descriptor filename, including its extension, is part of the path.
- **`<protocol>`** — `dash` or `hls`.
- **`<repr>`** — a representation `id` exactly as recorded in the descriptor
  (for example `video_1080_avc1_4807228`).
- **`<time>`** — the presentation start time of a segment, an integer in the
  track's timescale (see [Segment addressing](#segment-addressing)).

The `<asset>` portion is variable-length and precedes the fixed `<protocol>`
infix, so the server matches on the **rightmost** `/dash/` or `/hls/` in the
path. A descriptor whose directory happens to be named `dash` or `hls` still
resolves correctly.

## Manifests are protocol-specific; segments are not

The `init.mp4` and `<time>.m4s` routes return the same CMAF bytes under either
`<protocol>` prefix — media segments are shared across protocols, and only the
manifest differs. These two requests fetch identical data:

```text
/asset.json/dash/video_1080_avc1_4807228/init.mp4
/asset.json/hls/video_1080_avc1_4807228/init.mp4
```

See [One source, two protocols](../../explanation/two-protocols.md) for why.

## Segment addressing

A media segment is addressed by its **presentation start time**, an integer in
the track's timescale, with a `.m4s` extension. The first segment of a track
whose `sidx` reports an earliest presentation time of `0` is therefore
`<repr>/0.m4s`; subsequent segments start at the cumulative sum of the preceding
segment durations. These are exactly the `$Time$` values written into the DASH
`SegmentTimeline` and the segment URIs in the HLS media playlists, so players
never construct them by hand.

A request whose `<time>` does not fall on a segment boundary returns `404`; a
`<time>` that is not an integer returns `400`.

## Status codes

| Code | When |
|---|---|
| `200 OK` | The manifest or segment was generated and returned; also the `/health` probe. |
| `400 Bad Request` | A segment `<time>` is not a valid integer. |
| `404 Not Found` | The path is not a streaming route; the descriptor or a source file is missing; `<repr>` matches no representation; or `<time>` is not a segment boundary. |
| `500 Internal Server Error` | The descriptor JSON is malformed, or a source file is unreadable or not valid, supported CMAF. |

The split between `404` and `500` reflects ownership: a **missing** object is
treated as a client addressing error (`404`), while a **malformed** descriptor
or **broken** media file is the server's own content problem (`500`), because
the asset files are server-owned. Error responses carry a short plain-text
message describing the failure.

## CORS

The server applies a permissive CORS policy — any origin, any method — so
browser-based players can load manifests and segments cross-origin during
development.
