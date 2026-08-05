# dyndo-text

Subtitles for [`dyndo`](../../README.md), from source document to CMAF track.
`vtt::parse` reads a VTT document into a `Subtitle` — a list of cues, each a time
span and its text. Styling, positioning, and cue identifiers are dropped, so the
model is the same regardless of which format it came from and a second parser can
slot in beside VTT later.

`wvtt::pack` turns that `Subtitle` into a fragmented CMAF `wvtt` track indexed by
a `sidx`. Cues are tiled into samples covering the timeline with no holes, then
cut into fragments at the asset's splice points and on the requested minimum
segment length. Those cut times come from the asset's clock rather than from where
the cues fall, so every text track of an asset carries the same fragment timeline
and stays segment-aligned with the video and audio beside it.

`layer::WvttLayer` is how the rest of dyndo consumes all of this: an
[`opendal`](https://crates.io/crates/opendal) layer that packages subtitle
documents as they are read. A read of a `.vtt` path fetches the document from
whatever storage sits underneath, packs it, and returns the packed track's bytes —
byte ranges and all. Nothing is written back, and every other path passes straight
through, so [`dyndo-core`](../dyndo-core/README.md) reads a subtitle track exactly
as it reads a CMAF one and never learns the difference.

Two things deliberately stay outside: the track's language and role, which belong
to the transport ([`dyndo-dash`](../dyndo-dash/README.md),
[`dyndo-hls`](../dyndo-hls/README.md)), and storage itself — this crate adapts
reads, it does not own where bytes live.
[`mp4-atom`](https://crates.io/crates/mp4-atom) supplies the box model.
