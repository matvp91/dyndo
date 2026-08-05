# dyndo-cli

The `dyndo` command-line tool — the indexing and offline-manifest entry point
for [`dyndo`](../../README.md). It is thin wiring: argument parsing plus calls
into [`dyndo-core`](../dyndo-core/README.md),
[`dyndo-dash`](../dyndo-dash/README.md), and
[`dyndo-hls`](../dyndo-hls/README.md).

| Command | Purpose |
|---|---|
| `index` | Build or update an `asset.json` descriptor from CMAF and WebVTT sources. |
| `dash` | Render a DASH MPD from an `asset.json`. |
| `hls` | Render HLS playlists from an `asset.json` into a directory. |

All paths are read and written through an OpenDAL filesystem operator rooted at
`OPENDAL_FS_ROOT` (default: the current directory).

Full documentation lives in the book: the
**[dyndo CLI reference](https://matvp91.github.io/dyndo/reference/cli.html)**
covers every command, option, and default, and the
[how-to guides](https://matvp91.github.io/dyndo/how-to/index-sources.html) walk
through the tasks they serve.

## Install

```bash
cargo install --path .        # or, from the repo root: make install
```
