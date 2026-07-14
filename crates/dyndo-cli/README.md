# dyndo-cli

The `dyndo` command-line tool — the indexing and offline-manifest entry point for
[`dyndo`](../../README.md). It is thin wiring: argument parsing plus calls into
[`dyndo-core`](../dyndo-core/README.md).

## Commands

### `dyndo index`

Build an `asset.json` descriptor from one or more CMAF files. Input paths are
resolved relative to the output descriptor's directory.

```bash
dyndo index \
  -i index_video_avc_1080.mp4 \
  -i index_video_avc_720.mp4 \
  -i index_audio_aac_nl_2.mp4 \
  -o assets/asset.json
```

| Option | Description | Default |
|---|---|---|
| `-i, --input <PATH>` | Input CMAF file, repeatable (one track per file). | *(required)* |
| `-o, --output <PATH>` | Output descriptor path. | `asset.json` |

### `dyndo dash`

Render a static DASH MPD from an `asset.json` — useful for inspection and
debugging without running the server.

```bash
dyndo dash -i assets/asset.json -o assets/stream.mpd --compact
```

| Option | Description | Default |
|---|---|---|
| `-i, --input <PATH>` | Input `asset.json`. | `asset.json` |
| `-o, --output <PATH>` | Output manifest path. | `stream.mpd` |
| `-c, --compact` | Hoist shared `SegmentTemplate` content up to the `AdaptationSet` level. | off |

## Configuration

All I/O is rooted at an [OpenDAL](https://opendal.apache.org/) filesystem
operator. Set `OPENDAL_FS_ROOT` to change the root (defaults to the current
directory).

## Install

```bash
cargo install --path .        # or, from the repo root: make install
```
