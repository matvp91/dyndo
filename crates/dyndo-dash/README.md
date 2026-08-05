# dyndo-dash

DASH manifest generation for [`dyndo`](../../README.md): it turns an
`asset.json` descriptor and its probed tracks into a static MPD. It knows about
DASH and nothing else — reading CMAF and modelling the descriptor belong to
[`dyndo-core`](../dyndo-core/README.md), and
[`dash-mpd`](https://crates.io/crates/dash-mpd) supplies the XML model.

Both the `dyndo` CLI ([`dyndo-cli`](../dyndo-cli/README.md)) and
[`dyndo-server`](../dyndo-server/README.md) call the same builder, so a manifest
rendered offline is identical to one the server generates on the fly.

Full documentation lives in the book: the
**[`dyndo dash` reference](https://matvp91.github.io/dyndo/reference/cli/dash.html)**
specifies the manifest this crate produces — adaptation-set grouping, segment
templates, and timelines — and
**[Track roles](https://matvp91.github.io/dyndo/reference/roles.html)** covers
the `Role` and `Accessibility` descriptors it emits.
