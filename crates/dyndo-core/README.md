# dyndo-core

The domain and parsing library at the heart of [`dyndo`](../../README.md). It has
no CLI or HTTP concerns and is shared by both the `dyndo` CLI and `dyndo-server`.

`dyndo-core` reads a CMAF file's **header region only** and re-derives everything
downstream needs — the segment index, timing, codec, and bitrate — from the
`sidx`. All I/O flows through an [OpenDAL](https://opendal.apache.org/) operator,
so the byte source is pluggable (local filesystem today).

## Modules

| Module | Responsibility |
|---|---|
| [`cmaf`](src/cmaf.rs) | Bounded-memory header parse (`probe`). Streams the `moov` / `sidx` / first `moof` boxes (~10 KB) through an async reader and projects them into a `CmafHeader` (timing, init range, and segment map) and per-track `Metadata`. The `mdat` body is never read. |
| [`asset`](src/asset.rs) | The domain `Asset` (typed `video_tracks` / `audio_tracks` plus its source path), the `Segment`, and the `Track` trait implemented by `VideoTrack`, `AudioTrack`, and the runtime-tagged `AnyTrack`. Builds tracks from CMAF, reads init/media segment bytes on demand, and converts to/from the wire model. |
| [`codec`](src/codec.rs) | The `VideoCodec` / `AudioCodec` enums and their RFC 6381 `codecs` strings (e.g. `avc1.640028`, `mp4a.40.2`). |
| [`model`](src/model.rs) | The `asset.json` serde contract: `AssetModel` and the tagged `TrackModel` union. |
| [`dash`](src/dash/mod.rs) | DASH MPD generation from an `Asset`, with an optional compaction pass that hoists `SegmentTemplate` content shared by all `Representation`s up to the `AdaptationSet`. |
| [`hls`](src/hls/mod.rs) | HLS playlist generation from an `Asset`: a multivariant playlist plus one media playlist per track, with demuxed audio grouped by codec. |

## Design notes

- **Bounded memory.** The source is streamed only up to the first `moof`, and
  the media body is never loaded. An 800 MB source is parsed like an 8 MB one.
- **The `sidx` is the segment map.** The init range is `[0, moov_end)`, segment
  offsets are the prefix sum of each reference size, and the timeline is the
  prefix sum of the subsegment durations — all recomputed at read time, never
  stored in `asset.json`.
- **Fail fast.** A source that is not single-track CMAF with a `sidx` and a
  supported codec aborts parsing rather than falling back.

## Dependencies of note

[`mp4-atom`](https://crates.io/crates/mp4-atom) (with its `tokio` feature) for
typed, streamed box decoding over a `tokio` / `tokio-util` async reader,
[`opendal`](https://crates.io/crates/opendal) for ranged reads, and `serde` /
`serde_json` for the descriptor.
[`dash-mpd`](https://crates.io/crates/dash-mpd) and
[`quick-xml`](https://crates.io/crates/quick-xml) back DASH MPD generation, and
[`hls_m3u8`](https://crates.io/crates/hls_m3u8) backs HLS playlists.
