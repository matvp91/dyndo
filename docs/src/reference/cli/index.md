# dyndo index

Build or update an `asset.json` descriptor from one or more track descriptors.
Each input becomes one track. New tracks are probed from their file; tracks
already in the descriptor keep their metadata as-is, with only explicit
overrides applied.

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
- `role` — the track's purpose; never probed, so this is the only way to set it
  apart from editing the JSON. Audio and text only. Audio: `main`, `alternate`,
  `commentary`, `dub`, `description`, `enhanced-audio-intelligibility`. Text:
  `subtitle`, `caption`, `forced-subtitle`.

A bare `video.mp4` is the zero-override case. An **empty value** (`language=`)
means "no override". When a key is repeated, the last occurrence wins. A video
input with `language`/`role`, an unknown key, or a `role` invalid for the
track's type each abort the run.

## Input formats

The file extension (matched case-insensitively) selects how an input is read:

| Extension | Format | Becomes |
|---|---|---|
| `.mp4` | CMAF — fragmented MP4 | A video, audio, or text (`wvtt`) track, by media handler. |
| `.vtt` | Raw WebVTT | A text track served by chunking the file on the fly. |

Any other extension aborts with an unsupported-format error naming the
supported ones.

A CMAF input must be valid CMAF or the run aborts:

- a fragmented MP4 whose `moov` contains **exactly one track**;
- a single global `sidx` segment index; and
- a [supported codec](../../introduction.md#supported-codecs).

## Description

For each input, `index` decides between two cases by looking up the input's
path in the existing descriptor (when `--output` already exists, it is loaded
first):

- **New path** — the file is probed: its header region is read, the track kind
  is determined from its media handler, codec and per-type metadata are
  extracted, any `language`/`role` overrides are applied, and a track entry is
  appended with a freshly generated, thereafter-pinned `id`.
- **Known path** — the descriptor's stored metadata is kept **as-is**; the file
  is not re-probed for metadata, so hand-edits to the JSON survive a re-index.
  Explicit `language=`/`role=` overrides are the only mutation, and the `id`
  never changes.

The descriptor is then written to `--output` as pretty-printed JSON:

```text
wrote asset.json (3 tracks)
```

Input paths are resolved relative to the **output descriptor's directory**, and
the `path` stored for each track is that same descriptor-relative path. See
[path resolution](../cli.md#storage-root).

Note that loading an existing descriptor probes every listed track's header, so
**all sources already in the descriptor must still exist and parse** — a
re-index fails if one of them has gone missing.

## Examples

Index a multi-rendition asset, tagging the audio:

```bash
dyndo index \
  video_1080.mp4 \
  video_720.mp4 \
  audio_en.mp4,language=eng,role=main \
  -o asset.json
```

Add a subtitle from a raw WebVTT file:

```bash
dyndo index subtitles_nl.vtt,language=nld -o asset.json
```

Set a role on a track that is already indexed (updates the entry in place —
nothing else about it changes):

```bash
dyndo index audio_fr.mp4,role=dub -o asset.json
```

Write the descriptor into a subdirectory (inputs resolve relative to it):

```bash
dyndo index video.mp4 audio.mp4 -o out/asset.json
```

## See also

- [Index your CMAF sources](../../how-to/index-sources.md) — task-oriented guide.
- [Add a subtitle track](../../how-to/add-subtitles.md) — text tracks from `.vtt`.
- [asset.json descriptor](../asset-json.md) — the output format.
