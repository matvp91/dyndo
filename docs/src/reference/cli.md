# dyndo CLI

`dyndo` is the command-line tool for building asset descriptors and extracting video frames. It is the binary produced by the `dyndo-cli` crate.

```text
dyndo <COMMAND>
```

| Command | Purpose |
|---|---|
| [`index`](./cli/index.md) | Build or update an `asset.json` descriptor from CMAF and WebVTT sources. |
| [`image`](./cli/image.md) | Extract a full-resolution JPEG frame from an asset's first video track. |

## Global options

| Option | Description |
|---|---|
| `-h, --help` | Print help. Available on the top-level command and every subcommand. |
| `-V, --version` | Print the version. |

## Storage root

All paths are read and written through an [OpenDAL](https://opendal.apache.org/) filesystem operator rooted at one directory. By default that root is the current working directory; override it with `OPENDAL_FS_ROOT`:

| Variable | Description | Default |
|---|---|---|
| `OPENDAL_FS_ROOT` | Root directory for all CLI reads and writes. | `.` |

Within that root, a track's source path is resolved relative to the descriptor that references it. See [Understand how paths resolve](../how-to/index-sources.md#understand-how-paths-resolve).

## Exit behavior

| Status | When |
|---|---|
| `0` | The command completed successfully. |
| `1` | A runtime error occurred, such as a missing file, malformed descriptor, invalid source, unavailable frame, decode failure, or write failure. The message is written to stderr with the `dyndo:` prefix. |
| `2` | Clap rejected the command line, such as an unknown command or option, a missing required argument, or an invalid track descriptor. Usage is written to stderr. |

Commands do not silently skip invalid inputs. `index` writes its descriptor only after all named inputs have been processed; `image` writes its JPEG only after the requested frame has been decoded.

## Commands

- [`index`](./cli/index.md)
- [`image`](./cli/image.md)
