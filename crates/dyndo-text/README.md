# dyndo-text

Subtitles for [`dyndo`](../../README.md), from source document to CMAF text
track. It knows documents, cues, and boxes and nothing else: storage belongs to
[`dyndo-core`](../dyndo-core/README.md), which wraps this crate in an OpenDAL
layer so that reading a `.vtt` path yields a packed track's bytes, and the
track's language and role belong to the transport
([`dyndo-dash`](../dyndo-dash/README.md), [`dyndo-hls`](../dyndo-hls/README.md)).

Everything a source carries beyond timing and text is dropped — styling,
positioning, cue identifiers — so the model is the same whatever it came from,
and a second parser or container slots in beside VTT and `wvtt`. Fragments are
cut on the asset's clock rather than on where the cues fall, so every text track
of an asset shares one fragment timeline and stays segment-aligned with the video
and audio beside it. [`mp4-atom`](https://crates.io/crates/mp4-atom) supplies the
box model, less the cue boxes that fill a sample, which this crate adds.

Full documentation lives in the book:
**[Add a subtitle track](https://matvp91.github.io/dyndo/how-to/add-subtitles.html)**
walks through indexing and serving one.
