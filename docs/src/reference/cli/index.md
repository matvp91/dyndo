# dyndo index

Build or update an `asset.json` descriptor from one or more CMAF files. Each
input becomes one track. When the output already exists, tracks are merged into
it rather than overwriting.

## Synopsis

```text
dyndo index [OPTIONS] <INPUTS>...
```

## Options

| Option | Description | Default |
|---|---|---|
| `<INPUTS>...` | Track descriptor(s), one per track: `<path>[,language=..][,role=..]`. Positional, at least one required. | *(required)* |
| `-o, --output <OUTPUT>` | Output descriptor path. | `asset.json` |
| `-h, --help` | Print help. | |

## Descriptor syntax

Each input is a comma-separated descriptor whose **first field is the file
path**; the remaining fields are `key=value` overrides:

- `language` — ISO-639-2 code; overrides the language probed from the file.
  Audio and text only.
- `role` — the track's purpose; never probed, so this is the only way to set it.
  Audio and text only. Audio: `main`, `alternate`, `commentary`, `dub`,
  `description`, `enhanced-audio-intelligibility`. Text: `subtitle`, `caption`,
  `forced-subtitle`.

A bare `video.mp4` is the zero-override case. A video input with `language`/
`role`, an unknown field, a `role` invalid for the track's type, or a `path=`
first field each abort the run.

## Description

For each input, `index` reads the file's CMAF header region, determines whether
it is a video, audio, or text track from its media handler, extracts the codec
and per-type metadata, applies any `language`/`role` overrides, and records a
track entry. If `--output` already exists it is loaded first and each input is
**upserted by source path** — a new path is appended, an already-listed path is
replaced in place. The descriptor is written to `--output` as pretty-printed
JSON with a summary:

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

Index a multi-rendition asset, tagging the audio:

```bash
dyndo index \
  video_1080.mp4 \
  video_720.mp4 \
  audio_en.mp4,language=eng,role=main \
  -o asset.json
```

Add a track to an existing descriptor (merges by path):

```bash
dyndo index audio_fr.mp4,language=fra,role=dub -o asset.json
```

Write the descriptor into a subdirectory (inputs resolve relative to it):

```bash
dyndo index video.mp4 audio.mp4 -o out/asset.json
```

## See also

- [Index your CMAF sources](../../how-to/index-sources.md) — task-oriented guide.
- [asset.json descriptor](../asset-json.md) — the output format.
