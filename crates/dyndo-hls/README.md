# dyndo-hls

HLS playlist generation for [`dyndo`](../../README.md): it turns an `asset.json`
descriptor and its resolved tracks into a multivariant playlist, media playlists,
and image media playlists for thumbnail sprites. It knows about HLS and nothing
else — reading CMAF and modelling the descriptor belong to
[`dyndo-core`](../dyndo-core/README.md), and
[`m3u8-rs`](https://crates.io/crates/m3u8-rs) supplies the playlist model.

[`dyndo-server`](../dyndo-server/README.md) calls this crate's builders to generate playlists on demand.

Full documentation lives in the book: the
**[Server routes](https://matvp91.github.io/dyndo/reference/server/routes.html)** describes how to request the playlists, and
**[Track roles](https://matvp91.github.io/dyndo/reference/roles.html)** covers
the `DEFAULT`, `AUTOSELECT`, `FORCED`, and `CHARACTERISTICS` attributes it emits.
