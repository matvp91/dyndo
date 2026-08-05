# dyndo-hls

HLS playlist generation for [`dyndo`](../../README.md): it turns an `asset.json`
descriptor and its probed tracks into a multivariant playlist plus one media
playlist per track. It knows about HLS and nothing else — reading CMAF and
modelling the descriptor belong to [`dyndo-core`](../dyndo-core/README.md), and
[`hls_m3u8`](https://crates.io/crates/hls_m3u8) supplies the playlist model.

Both the `dyndo` CLI ([`dyndo-cli`](../dyndo-cli/README.md)) and
[`dyndo-server`](../dyndo-server/README.md) call the same builders, so playlists
rendered offline are identical to the ones the server generates on the fly.

Full documentation lives in the book: the
**[`dyndo hls` reference](https://matvp91.github.io/dyndo/reference/cli/hls.html)**
specifies the playlists this crate produces — variants, rendition groups, and
segment addressing — and
**[Track roles](https://matvp91.github.io/dyndo/reference/roles.html)** covers
the `DEFAULT`, `AUTOSELECT`, `FORCED`, and `CHARACTERISTICS` attributes it emits.
