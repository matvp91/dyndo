# dyndo-text

Subtitles for [`dyndo`](../../README.md), from source document to packaged track.
`vtt::parse` reads a VTT document into a `Subtitle` — a list of cues, each a time
span and its text. Styling, positioning, and cue identifiers are dropped, so the
model is the same regardless of which format it came from and a second parser can
slot in beside VTT later.

`wvtt::pack` turns that `Subtitle` into a fragmented CMAF `wvtt` track in a single
file, indexed by a `sidx`: cues are tiled into samples covering the timeline with
no holes, then grouped into fragments at the asset's splice points and minimum
segment length — the same policy [`dyndo-core`](../dyndo-core/README.md) applies
when it groups fragments into segments, so a packed text track can be regrouped to
line up with the asset's video and audio.

The crate knows subtitles and nothing else: no storage, no manifests, and no track
language — that belongs to the transport
([`dyndo-dash`](../dyndo-dash/README.md), [`dyndo-hls`](../dyndo-hls/README.md)).
[`mp4-atom`](https://crates.io/crates/mp4-atom) supplies the box model.
