# dyndo-text

Subtitles for [`dyndo`](../../README.md), from source document to CMAF track.
`vtt::parse` reads a VTT document into a `Subtitle` — a list of cues, each a time
span and its text. Styling, positioning, and cue identifiers are dropped, so the
model is the same regardless of which format it came from and a second parser can
slot in beside VTT later.

`wvtt::pack` turns that `Subtitle` into a fragmented CMAF `wvtt` track indexed by
a `sidx`. Cues are tiled into samples covering the timeline with no holes, then
cut into fragments at the asset's splice points and on the requested text segment
length. Those cut times come from the asset's clock rather than from where
the cues fall, so every text track of an asset carries the same fragment timeline
and stays segment-aligned with the video and audio beside it.

Nothing here touches storage. [`dyndo-core`](../dyndo-core/README.md) wraps
`vtt::parse` and `wvtt::pack` in an opendal layer so that reading a `.vtt` path
yields a packed track's bytes, and reads a subtitle track exactly as it reads a
CMAF one; this crate only knows documents, cues, and boxes.

The track's language and role stay outside too — those belong to the transport
([`dyndo-dash`](../dyndo-dash/README.md),
[`dyndo-hls`](../dyndo-hls/README.md)).
[`mp4-atom`](https://crates.io/crates/mp4-atom) supplies the box model.
