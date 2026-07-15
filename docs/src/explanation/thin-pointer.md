# The thin-pointer approach

This page explains the central design decision in dyndo — serving adaptive
streams from a *thin pointer* to your media rather than a repackaged copy of it —
and the trade-offs that come with it.

## The conventional approach: package ahead of time

A traditional packager runs once, ahead of playback, and writes a full set of
DASH and HLS renditions to disk. For each source it produces init segments,
thousands of media segments, and the manifests that index them. The output is a
complete, self-contained copy of your media, restructured for streaming.

This works, but it has costs:

- **Storage is duplicated.** The packaged output is a second full copy of the
  media, often in two layouts (one for DASH, one for HLS).
- **Storage is coupled to protocol.** Adding or changing a protocol means
  re-running the packager and writing yet more files.
- **The segment layout is frozen.** The manifest and the on-disk segments must
  agree, so they are produced together and kept together.

## dyndo's approach: keep the source, point at it

dyndo inverts this. It never writes media. Instead, indexing records a small
**descriptor** — [`asset.json`](../reference/asset-json.md) — that says only
*what* the tracks are and *where* their source files live:

```json
{
  "type": "video",
  "id": "video_avc1_1080_4807228",
  "path": "video_1080.mp4",
  "fourcc": "avc1",
  "timescale": 90000,
  "width": 1920,
  "height": 1080
}
```

Notice what is *not* here: no list of segments, no byte offsets, no init-segment
range, no per-protocol layout. The descriptor is a pointer, not a copy. At
request time the server reads the source's header and re-derives everything else
(the [next page](./segment-index.md) covers how).

This yields three properties:

- **No duplicated storage.** Your original CMAF files *are* the served media.
  The descriptor adds a few hundred bytes per asset.
- **One source of truth.** The segment map lives in exactly one place — the
  source's own `sidx` — and is never copied into the descriptor, so the two can
  never drift apart.
- **Protocol at the edge.** Because segments are never protocol-specific (they
  are just CMAF), adding HLS alongside DASH costs a manifest generator, not a
  second copy of the media. See [One source, two protocols](./two-protocols.md).

## What you trade for it

The thin pointer moves work from packaging time to request time, so it is not
free:

- **Per-request parsing.** Every manifest request parses the source headers
  afresh rather than reading a finished file. That work is deliberately bounded
  (see [Reading a source](./segment-index.md)), but it is real, and a production
  deployment would cache generated manifests.
- **Sources must already be CMAF.** dyndo indexes and serves CMAF; it does not
  transcode. The media must already be fragmented MP4 with a global `sidx`.
  Producing that is a one-time, out-of-band step (for example with ffmpeg).
- **The source files must stay put.** The descriptor points at them by relative
  path; move a source without moving its descriptor and the pointer dangles.

## When this fits

The thin pointer is a good fit when you already have (or can cheaply produce)
CMAF masters and want to serve them over multiple protocols without maintaining
a second, protocol-shaped copy of your library. It trades a little repeated
computation for a large reduction in stored bytes and a single, canonical
segment map.

## See also

- [Reading a source: headers and the segment index](./segment-index.md) — how
  the descriptor stays this thin.
- [One source, two protocols](./two-protocols.md) — how one set of segments
  serves both DASH and HLS.
