# dyndo index

Build an `asset.json` descriptor from one or more CMAF files. Each input file
becomes one track.

## Synopsis

```text
dyndo index [OPTIONS] --input <INPUT>
```

## Options

| Option | Description | Default |
|---|---|---|
| `-i, --input <INPUT>` | Input CMAF file. Repeatable — pass once per track. | *(required)* |
| `-o, --output <OUTPUT>` | Output descriptor path. | `asset.json` |
| `-h, --help` | Print help. | |

## Description

For each `--input`, `index` reads the file's CMAF header region, determines
whether it is a video, audio, or text track from its media handler, extracts the
codec and per-type metadata, and records a track entry in the descriptor. It
then writes the descriptor to `--output` as pretty-printed JSON and prints a
summary:

```text
wrote asset.json (3 tracks)
```

Input paths are resolved relative to the **output descriptor's directory**, and
the `path` stored for each track is that same descriptor-relative path. See
[path resolution](../cli.md#storage-root).

## Requirements on inputs

Each input must be valid CMAF or the run aborts:

- a fragmented MP4 whose `moov` contains **exactly one track**;
- a single global `sidx` segment index; and
- a [supported codec](../../introduction.md#supported-codecs).

Text tracks are not indexed directly from `.vtt` here — use
[`pack`](./pack.md) to create and add them. `index` *can* read a
`pack`-produced `wvtt` MP4 like any other CMAF track.

## Examples

Index a multi-rendition asset:

```bash
dyndo index \
  -i video_1080.mp4 \
  -i video_720.mp4 \
  -i audio_en.mp4 \
  -o asset.json
```

Write the descriptor into a subdirectory (inputs resolve relative to it):

```bash
dyndo index -i video.mp4 -i audio.mp4 -o out/asset.json
```

## See also

- [Index your CMAF sources](../../how-to/index-sources.md) — task-oriented guide.
- [asset.json descriptor](../asset-json.md) — the output format.
