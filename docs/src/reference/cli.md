# dyndo CLI

`dyndo` is the command-line front-end for indexing CMAF sources and rendering
manifests offline. It is the binary produced by the `dyndo-cli` crate.

```text
dyndo <COMMAND>
```

| Command | Purpose |
|---|---|
| [`index`](./cli/index.md) | Build an `asset.json` descriptor from one or more CMAF files. |
| [`dash`](./cli/dash.md) | Render a DASH MPD from an `asset.json`. |
| [`hls`](./cli/hls.md) | Render HLS playlists from an `asset.json` into a directory. |
| [`pack`](./cli/pack.md) | Pack a subtitle file into a CMAF text track and add it to an asset. |

## Global options

| Option | Description |
|---|---|
| `-h, --help` | Print help (available on the top-level command and every subcommand). |
| `-V, --version` | Print the version. |

## Storage root

All file paths are read and written through an [OpenDAL](https://opendal.apache.org/)
filesystem operator rooted at a single directory. By default that root is the
current working directory; override it with the `OPENDAL_FS_ROOT` environment
variable:

| Variable | Description | Default |
|---|---|---|
| `OPENDAL_FS_ROOT` | Root directory for all reads and writes. | `.` (current directory) |

Within that root, a track's source path is always resolved **relative to the
descriptor** that references it, not relative to your shell's working directory.
See [Understand how paths resolve](../how-to/index-sources.md#understand-how-paths-resolve).

## Exit behavior

Every command runs to completion or aborts. On any error — a missing file,
malformed descriptor JSON, an input that isn't valid CMAF, or an unsupported
codec — the command prints the error and exits with a non-zero status. There is
no partial success: `index` does not skip a bad input and continue, and a failed
`dash`/`hls` writes nothing.

## Commands

- [`index`](./cli/index.md)
- [`dash`](./cli/dash.md)
- [`hls`](./cli/hls.md)
- [`pack`](./cli/pack.md)
