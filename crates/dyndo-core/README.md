# dyndo-core

The media layer at the heart of [`dyndo`](../../README.md): CMAF header parsing,
the source-track and derived-thumbnail domain models, the `asset.json` serde
contract, RFC 6381 codec strings, and the grouping of fragments into served
segments. It has no CLI,
HTTP, or manifest concerns, and is shared by
[`dyndo-dash`](../dyndo-dash/README.md), [`dyndo-hls`](../dyndo-hls/README.md),
and both binaries.

A source's **header region only** is read — the `moov`, `sidx`, and first `moof`
— and everything downstream needs is re-derived from it, so an 800 MB source
parses like an 8 MB one and the media body is never loaded. All I/O flows through
an [OpenDAL](https://opendal.apache.org/) operator, so the byte source is
pluggable.

Full documentation lives in the book:
**[Reading a source](https://matvp91.github.io/dyndo/explanation/segment-index.html)**
explains the header parse and the segment index derived from it, and the
**[asset.json reference](https://matvp91.github.io/dyndo/reference/asset-json.html)**
specifies the descriptor this crate reads and writes. The
**[terminology](https://matvp91.github.io/dyndo/mental/terminology.html)** page
defines the source, derived, thumbnail, and image distinctions.
