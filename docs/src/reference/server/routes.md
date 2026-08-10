# HTTP routes

The server exposes two things: a health probe, and a single output tree under
`/out/`. All routes are `GET`.

```text
GET /health
GET /out/<options>/<resource>[?filter=<expression>]
```

`<options>` is a **Rison** object — a compact, URL-friendly object notation, so
`(asset:demo,sml:6000)` is the equivalent of the JSON
`{"asset":"demo","sml":6000}` — carrying the asset to serve and
how to segment it. `<resource>` names what to return from it.
Because the options travel in the path rather than a query string, a manifest
and every segment it references share one prefix, and the relative URLs inside a
manifest resolve correctly without rewriting.

[`filter`](#filtering-tracks) is the exception, and deliberately so: it travels
in the query string because it shapes a manifest without needing to reach the
resources that manifest references.

## Health check

| Path | Description | Content-Type |
|---|---|---|
| `/health` | Liveness probe. Returns `200 OK` with an empty body. | *(none)* |

`/health` is a fixed route registered ahead of the `/out/` tree, so it can never
be shadowed by an asset. Use it for container and load-balancer health checks;
see [Deploy with Docker](../../how-to/deploy-with-docker.md).

## The options object

The first path segment after `/out/` is a Rison object. The enclosing
parentheses are optional — this is Rison's *o-rison* form, intended for exactly
this place — so both of these name the same options:

```text
/out/(asset:demo,sml:6000)/master.m3u8
/out/asset:demo,sml:6000/master.m3u8
```

```text
/out/(asset:movies%2Fbig-buck-bunny)/index.mpd
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

### Output options

Four options affect DASH output:

| Full key | Shorthand | Type | Default | Description |
|---|---|---|---|---|
| `compact` | `c` | boolean | `false` | Hoist segment-template data shared by DASH representations to their adaptation set. |
| `multi_period` | `mp` | boolean | `false` | Open a `Period` at each [segment boundary](#segmentation-options) rather than describing the asset as one. |
| `thumbnail_tile_size` | `tts` | integer | `0` | Number of thumbnails per sprite row and column. `0` disables thumbnail output. |
| `thumbnail_step` | `ts` | integer | `0` | Milliseconds between adjacent thumbnails. `0` disables thumbnail output. |

One option affects HLS output:

| Full key | Shorthand | Type | Default | Description |
|---|---|---|---|---|
| `wvtt` | — | boolean | `false` | Point text renditions at packaged `wvtt` segments rather than WebVTT documents. |

The supported shorthand map is therefore `asset` → `a`, `min_length` → `sml`,
`text_length` → `stl`, `boundaries` → `sb`, `compact` → `c`, `multi_period` →
`mp`, `thumbnail_tile_size` → `tts`, and `thumbnail_step` → `ts`. `wvtt` has no
shorthand. The forms are equivalent:

```text
/out/(asset:demo,min_length:6000,compact:!t)/index.mpd
/out/(a:demo,sml:6000,c:!t)/index.mpd
```

Unknown keys are rejected on every output route.

## Filtering tracks

A request can narrow which of an asset's tracks are served, so one descriptor
covers every variation you want to offer — a resolution cap, one audio language,
a version without subtitles — instead of one descriptor per variation.

| Parameter | Applies to |
|---|---|
| `filter` | `index.mpd` and `master.m3u8` |

```text
/out/(asset:demo)/index.mpd?filter=type!=video%7C%7Cheight%3C=720
/out/(asset:demo)/master.m3u8?filter=type==audio
```

The `filter` parameter is accepted on all routes handled as manifests, including a per-track HLS media playlist, but it only affects `index.mpd` and `master.m3u8`. Track-file routes do not parse a query. A filter therefore shapes top-level manifests and never gates addressing: a track a filter dropped stays fetchable by its own URL.

The syntax follows [Unified Streaming's URL
filters](https://docs.unified-streaming.com/documentation/vod/player-urls.html),
so filters written for that stack read the same here.

### Expression syntax

A filter is a boolean expression over track attributes:

```text
attribute <operator> value
expression && expression        both must hold
expression || expression        either may hold
( expression )                  grouping
```

`&&` binds tighter than `||`, so `a||b&&c` means `a||(b&&c)`. Wrap the
enclosing expression in parentheses if you prefer — they are accepted but not
required. Whitespace around operators is allowed.

| Operator | Meaning | Applies to |
|---|---|---|
| `==` `!=` | equal, not equal | every attribute |
| `<` `<=` `>` `>=` | ordering | numeric attributes only |

An ordering operator on a textual attribute — `language<nl` — is a `400`.

### Percent-encoding

| Character | Encode as | Why |
|---|---|---|
| `<` `>` | `%3C` `%3E` | Not legal URI characters. Unencoded, the request is rejected by the HTTP layer before dyndo sees it. |
| `&&` | `%26%26` | `&` separates query parameters, so an unencoded `&&` splits the filter in two. |
| `\|\|` | `%7C%7C` | Usually survives unencoded, but RFC 3986 excludes `\|` from a query. |

Any HTTP client that builds a URL properly handles this for you.

An unencoded `&&` would otherwise be the dangerous case:
`?filter=type!=video&&height%3C=720` arrives as `filter=type!=video`, a valid
filter on its own. It is caught because **a manifest request accepts no query
parameter but the filter**, so the two junk fragments are unrecognised and the
request is a `400`. The same rule makes a cache-busting or analytics parameter on
a manifest URL a `400` — put those in the path's
[options object](#the-options-object).

### Attributes

Filtering runs on **resolved** tracks — tracks dyndo has probed — so `bitrate`,
`avg_bitrate` and `duration` are available, and `codec` is the value the source
actually declares rather than the descriptor's claim.

| Attribute | Type | Present on | Description |
|---|---|---|---|
| `type` | text | every track | `video`, `audio` or `text`. |
| `id` | text | every track | The track's `id`, as it appears in manifest URLs. |
| `codec` | text | every track | The probed codec, for example `avc1.640028`. |
| `bitrate` | numeric | every track | Peak segment bitrate in bits per second. |
| `avg_bitrate` | numeric | every track | Mean segment bitrate in bits per second. |
| `duration` | numeric | every track | Track duration in milliseconds. |
| `width` | numeric | video | Frame width in pixels. |
| `height` | numeric | video | Frame height in pixels. |
| `frame_rate` | text | video | The rate as written, for example `25/1`. |
| `sample_rate` | numeric | audio | Samples per second. |
| `channels` | numeric | audio | Channel count. |
| `language` | text | audio, text | The track's language tag, compared exactly as the descriptor spells it. |
| `role` | text | audio, text | One of the [track roles](../roles.md). |

`bitrate` and `avg_bitrate` are derived from segment sizes, so they reflect the
segmentation options the same request asked for.

A textual value is not checked against what exists — `role==narrator` parses
happily and simply matches no track. A numeric attribute does check: `height==tall`
is a `400`.

### What a filter keeps

**A track is served only if the expression is true for that track.** Each track
is judged on its own; nothing is judged as a group.

**A comparison against an attribute the track does not carry is false, whatever
the operator.** An audio track has no `height`, so `height<=720` is false for it
— and so is `height!=720`, which reads backwards until you hold the rule in
mind. The consequence matters:

```text
?filter=height%3C=720   video capped at 720 — and every audio and
                        subtitle track dropped with it
```

Sparing a type is the `type!=…` idiom, which works because `type` is the one
attribute every track carries:

| Goal | Filter |
|---|---|
| Cap video at 720, keep everything else | `type!=video\|\|height<=720` |
| Video only, capped at 720 | `type==video&&height<=720` |
| Everything but the audio | `type!=audio` |
| Video up to 720 plus subtitles, no audio | `type!=audio&&(type!=video\|\|height<=720)` |
| Dutch audio only, video untouched | `type!=audio\|\|language==nld` |
| Drop low-bitrate video renditions | `type!=video\|\|bitrate>=800000` |

Mind `&&` against `||` here: `type!=video&&height<=720` requires *both*, so it
drops the audio it was meant to spare. Use `||` to spare, `&&` to narrow.

A filter that leaves at least one track produces a manifest — dropping all video
but keeping audio is a legitimate audio-only presentation. A filter that matches
**nothing** is a `404`.

### Caching

The filter is part of the URL's query string, so a CDN in front of dyndo must
include the query string in its cache key, or every filter will be served
whichever variant was cached first.

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
| `<track-id>/<time>.vtt` | The same segment of a text track, as a WebVTT document. | `text/vtt` |
| `<track-id>/<time>.jpg` | A JPEG thumbnail sprite advertised by the DASH manifest. | `image/jpeg` |

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

### Thumbnail sprites

Set both `thumbnail_tile_size` and `thumbnail_step` on a DASH request to add an image adaptation set. A tile size of `4` creates a 4-by-4 sprite, and a step of `1000` samples one frame per second:

```text
/out/(asset:demo,tts:4,ts:1000)/index.mpd
```

The MPD addresses each sprite as `<video-track-id>/<time>.jpg`. Use the same options prefix when requesting that URL. If either setting is zero, the MPD has no thumbnail adaptation set and `.jpg` requests return `404`.

## Segments are protocol-independent

There is no `dash` or `hls` component anywhere in a segment path. Both manifests
reference the same `<track-id>/init.mp4` and `<track-id>/<time>.m4s` URLs, and a
request for one returns the same CMAF bytes regardless of which manifest sent
the player there. Only `index.mpd` and the `.m3u8` resources are
protocol-specific. See
[Dynamic packaging without media copies](../../explanation/dynamic-packaging.md)
for why.

## Two spellings of one text segment

A text segment answers to both extensions at once, and they describe the same
segment: the same cut points, the same duration, the same bytes underneath.

```text
/out/(asset:demo)/text_3b519953-…/0.m4s   → packaged wvtt bytes
/out/(asset:demo)/text_3b519953-…/0.vtt   → the WebVTT document those bytes hold
```

A `.vtt` request resolves the segment exactly as a `.m4s` request does, then reads
the cues back out of the packaged bytes. The document carries the absolute
timestamps the source used and no `X-TIMESTAMP-MAP`, since the times are already
on the presentation's clock.

Which one a player asks for is the manifest's business: DASH always references
`.m4s`, while HLS references `.vtt` unless the request passes
[`wvtt`](#output-options). `<track-id>/init.mp4` stays available for the track
either way, and is simply not referenced by a WebVTT rendition — a WebVTT segment
needs no initialization.

A `.vtt` response is reconstructed from the packaged initialization and media segment, whether the original source was raw WebVTT or CMAF `wvtt`.

## Segment addressing

A media segment is addressed by its **presentation start time**, an integer in
the track's own timescale, with a `.m4s` extension — or `.vtt` for a text track
served as a document. The server re-derives the
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
| `400 Bad Request` | The options path segment is malformed Rison or contains an unknown option, a manifest route carries an unrecognised query parameter, or the [filter](#filtering-tracks) is malformed — an unknown attribute, an ordering operator on a textual attribute, or a non-numeric value for a numeric attribute. |
| `404 Not Found` | The path does not contain separate options and resource components; `<track-id>` matches no track; a segment filename has an unsupported extension or a non-integer time; `<time>` is not a segment boundary; a thumbnail is disabled or unavailable; the descriptor does not exist; or the [filter](#filtering-tracks) matched no track. |
| `500 Internal Server Error` | The descriptor JSON is malformed; a source file is unreadable or is not valid, supported CMAF; packaged subtitle cues cannot be parsed; thumbnail generation fails; or manifest serialization fails. |

The split between `404` and `500` reflects ownership: a **missing** object is
treated as a client addressing error, while a **malformed** descriptor or
**broken** media file is the server's own content problem, because the asset
files are server-owned. Error responses carry a short plain-text message.

## CORS

The server applies a permissive CORS policy — any origin, any method — so
browser-based players can load manifests and segments cross-origin during
development.
