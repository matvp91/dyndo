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

- `language` — a BCP 47 language tag, such as `en`, `eng`, or `pt-BR`; overrides
  the language probed from the file. Applied to audio and text tracks. Malformed
  tags are rejected.
- `role` — the track's purpose; never probed, so this is the only way to set it
  apart from editing the JSON. Applied to audio and text tracks. One of `main`,
  `alternate`, `commentary`, `dub`, `description`,
  `enhanced-audio-intelligibility`, `subtitle`, `caption`, `forced-subtitle`.

A bare `video.mp4` is the zero-override case. An **empty value** (`language=`,
`role=`) means "no override". When a key is repeated, the last occurrence wins.

Two behaviours are worth knowing because they are permissive rather than strict:

- **Overrides on a video input are accepted and silently ignored.** Video tracks
  have no `language` or `role` field, so `video.mp4,language=eng` indexes the
  video without complaint and without effect.
- **A role is not checked against the track type.** `audio.mp4,role=subtitle` is
  accepted and stored. Only an *unrecognised* role value aborts the run. Picking
  a role that suits the track is up to you — see [Track roles](../roles.md) for
  which roles are meaningful where.

An unknown key (anything other than `language` or `role`), or an unrecognised
role value, is rejected while parsing arguments:

```text
error: invalid value 'audio.mp4,codec=aac' for '<INPUTS>...': expected language=.. or role=.., got "codec=aac"
error: invalid value 'audio.mp4,role=bogus' for '<INPUTS>...': unknown role: bogus
```

Because the path is simply the first field, `path=video.mp4` is read as a file
literally named `path=video.mp4`, not as a `path` key.

## Input formats

The file extension selects how an input is read. Matching is
**case-sensitive** — `.MP4` is not recognised.

| Extension | Format | Becomes |
|---|---|---|
| `.mp4` | CMAF — fragmented MP4 | A video, audio, or text (`wvtt`) track, by media handler. |
| `.vtt` | Raw WebVTT | A text track (see the caveat below). |

Any other extension aborts with `unsupported track format`.

A `.mp4` input must parse as CMAF or the run aborts. The reader streams from the
start of the file until it has the `moov`, the `sidx`, and the first `moof`, and
requires:

- a `moov` containing at least one `trak`, whose **first** track is the one
  described (additional tracks in the file are ignored);
- a sample entry in that track's `stsd`, using a
  [supported codec](../../introduction.md#supported-codecs);
- a single `sidx` with a non-zero timescale, no zero-duration references, and
  every reference a media reference starting with a SAP of type 1.

> A raw `.vtt` input is recorded as a text track with codec `wvtt`, which is what
> it is packaged as when served — dyndo parses and packages it on the way out, so
> nothing is written beside your `.vtt`. Its language is `und` unless you pass
> `language=`, because WebVTT declares none of its own.

## Description

For each input, `index` decides between two cases by looking up the input's
resolved path in the existing descriptor (when `--output` already exists, it is
loaded first):

- **New path** — the file is probed: its header region is read, the track kind
  is determined from its media handler, codec and per-type metadata are
  extracted, any `language`/`role` overrides are applied, and a track entry is
  appended with an id derived from the path.
- **Known path** — the descriptor's stored metadata is kept **as-is**; the file
  is not re-probed, so hand-edits to the JSON survive a re-index. Explicit
  `language=`/`role=` overrides are the only mutation, and the `id` never
  changes.

Loading an existing descriptor only parses its JSON — the sources it already
lists are **not** opened. A re-index therefore succeeds even if a previously
indexed file has since been moved or deleted; only the inputs named on this run
are read.

The descriptor is then written to `--output` as pretty-printed JSON:

```text
wrote asset.json (3 tracks)
```

Input paths are resolved relative to the **output descriptor's directory**, and
the `path` stored for each track is that same descriptor-relative path. See
[path resolution](../cli.md#storage-root).

## Examples

Index a multi-rendition asset, tagging the audio:

```bash
dyndo index \
  video_1080.mp4 \
  video_720.mp4 \
  audio_en.mp4,language=eng,role=main \
  -o asset.json
```

Add a subtitle from a CMAF `wvtt` source:

```bash
dyndo index text_wvtt_nld.mp4,language=nld,role=subtitle -o asset.json
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
- [Add a subtitle track](../../how-to/add-subtitles.md) — text tracks.
- [asset.json descriptor](../asset-json.md) — the output format.
