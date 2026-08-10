# dyndo-dash

DASH manifest generation for [`dyndo`](../../README.md): it turns an
`asset.json` descriptor and its probed tracks into a static MPD. It knows about
DASH and nothing else — reading CMAF and modelling the descriptor belong to
[`dyndo-core`](../dyndo-core/README.md), and
[`dash-mpd`](https://crates.io/crates/dash-mpd) supplies the XML model.

[`dyndo-server`](../dyndo-server/README.md) calls this crate's builder to generate manifests on demand.

Full documentation lives in the book: the
**[Server routes](https://matvp91.github.io/dyndo/reference/server/routes.html)** describes how to request the manifest, and
**[Track roles](https://matvp91.github.io/dyndo/reference/roles.html)** covers
the `Role` and `Accessibility` descriptors it emits.
