# dyndo-core

The media layer of [`dyndo`](../../README.md): everything about reading CMAF
sources and describing what is in them, with no knowledge of DASH, HLS, CLI, or
HTTP. The manifest crates ([`dyndo-dash`](../dyndo-dash/README.md),
[`dyndo-hls`](../dyndo-hls/README.md)) and both binaries build on it.

All I/O goes through an [OpenDAL](https://opendal.apache.org/) `Operator`, so
the same code reads from local disk, S3, or anything else OpenDAL supports.

## Modules

| Module | Responsibility |
|---|---|
| `asset_descriptor` | The `asset.json` serde contract: `AssetDescriptor`, `TrackDescriptor`, and the `TrackKind` enum (`Video`/`Audio`/`Text`) that flattens into it. Reading, path resolution, and track lookup. |
| `box_reader` | Streams an MP4 from the start and collects the `moov`, `sidx`, and first `moof`, then validates them. Stops before the first `mdat`. |
| `track_probe` | Derives a track's kind, RFC 6381 codec string, timescale, frame rate, and fragment list from those boxes. |
| `track` | The `Track` model: identity, content and MIME types, duration, and ranged reads. |
| `track_source` | Where a track's bytes come from — stored (ranged reads through OpenDAL) or in memory. |
| `segment` | Groups a track's CMAF fragments into served segments according to `min_segment_length` and `segment_boundaries`. |
| `track_helpers` | Cross-track aggregates the manifest crates need: concurrent probing, presentation duration, peak and average bitrates. |
| `role` | The nine-value `Role` vocabulary, shared by both manifest crates. |

## Bounded-memory parsing

`box_reader` reads only the header region — `moov` + `sidx` + first `moof` —
and skips uninteresting boxes by length, so parsing cost tracks the number of
segments rather than the size of the media. An 800 MB source and an 8 MB source
are parsed from roughly the same ~10 KB of bytes. Media payload is fetched only
when a specific segment is requested, and then only that segment's byte range.

## Segment identity and addressing

A track's `id` is `<content-type>_<uuid>`, where the UUID is version 5 in the URL
namespace over the track's source path. It is therefore deterministic across
machines and independent of the track's metadata, so relabelling a track never
changes the URLs it is served under.

Segments are addressed by presentation start time in the track's own timescale.
`segment` re-derives the mapping from time to byte range on demand, in exact
integer arithmetic, so tracks at different timescales agree on where segment
edges fall.

## Tests

Unit tests live beside the code; `tests/probe.rs` exercises probing against the
committed header-only CMAF fixtures in [`fixtures/`](../../fixtures). Those
fixtures are truncated after the first `moof`, so they are valid for probing and
manifest generation but contain no media payload to read back.

Full documentation lives in the book:
**<https://matvp91.github.io/dyndo/>** — in particular the
[asset.json reference](https://matvp91.github.io/dyndo/reference/asset-json.html)
and
[Reading a source](https://matvp91.github.io/dyndo/explanation/segment-index.html).
