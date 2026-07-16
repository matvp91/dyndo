# Index your CMAF sources

This guide shows how to build an `asset.json` descriptor from a set of CMAF
files with `dyndo index`. You do this once per asset; the descriptor is what the
server (or the offline manifest commands) reads afterwards.

## Before you start

Each input must be valid CMAF:

- a fragmented MP4 containing a `moov` with **exactly one track**;
- a single global `sidx` segment index; and
- a [supported codec](../introduction.md#supported-codecs).

Any violation aborts the run — there are no silent fallbacks and no
skip-and-continue. If you need to produce conforming files, see the ffmpeg
recipe in the [Getting started tutorial](../tutorial/getting-started.md#step-2-create-two-cmaf-sources).

## Index one track per input

Pass each source as a positional argument. Every file becomes one track — one
video rendition, one audio rendition, and so on:

```bash
dyndo index \
  video_1080.mp4 \
  video_720.mp4 \
  audio_en.mp4 \
  -o asset.json
```

```text
wrote asset.json (3 tracks)
```

The `-o` path defaults to `asset.json` in the current directory; pass it
explicitly to write elsewhere.

## Set a language or role

An input may carry per-track parameters after the path, as comma-separated
`key=value` fields — `language` (an ISO-639-2 code) and `role` (the track's
purpose). Both apply to **audio** and **text** tracks only:

```bash
dyndo index \
  video.mp4 \
  audio_nl.mp4,language=nld,role=main \
  audio_fr.mp4,language=fra,role=dub \
  -o asset.json
```

`language` overrides the code probed from the file; `role` is never probed, so
this is the only way to set it. Valid roles are, for audio, `main`, `alternate`,
`commentary`, `dub`, `description`, `enhanced-audio-intelligibility`; for text,
`subtitle`, `caption`, `forced-subtitle`. A video input takes neither field, an
unknown field is rejected, and a role that does not apply to the track's type
(e.g. `subtitle` on audio) is rejected — the run aborts with a message.

For what each role does to the generated manifests — which rendition a player
defaults to, what it auto-selects, and the accessibility signalling — see
[Label tracks with roles](./label-roles.md).

## Add to or update an existing descriptor

Running `index` against an `asset.json` that already exists **merges** into it
rather than overwriting. A track is matched by its source path: index a new path
to append it, or re-index a path that is already listed to replace that entry —
handy for adding a `role` you left off the first time:

```bash
# start with the video
dyndo index video.mp4 -o asset.json          # wrote asset.json (1 tracks)

# append an audio track
dyndo index audio.mp4 -o asset.json          # wrote asset.json (2 tracks)

# update that same audio track in place — still two tracks
dyndo index audio.mp4,role=main -o asset.json  # wrote asset.json (2 tracks)
```

Renaming a source file on disk is invisible to `index` (identity is the path you
point at): re-indexing the new name appends a second entry, so edit or remove
the stale one in the JSON by hand.

## Understand how paths resolve

Input paths are **relative to the output descriptor's directory**, not to your
shell's working directory. This keeps a descriptor portable: `path` values in
`asset.json` stay valid as long as the sources sit in the same place relative to
it.

For example, writing the descriptor into a subdirectory:

```bash
dyndo index video.mp4 audio.mp4 -o out/asset.json
```

resolves the inputs as `out/video.mp4` and `out/audio.mp4`, and records
`"path": "video.mp4"` and `"path": "audio.mp4"` in the descriptor.

The root that all of this resolves against is dyndo's storage root, which for
the CLI is the current directory. Override it with the `OPENDAL_FS_ROOT`
environment variable:

```bash
OPENDAL_FS_ROOT=/srv/media dyndo index video.mp4 -o asset.json
```

## Inspect the result

The descriptor is small, human-readable JSON — safe to open, diff, and even
hand-edit:

```json
{
  "tracks": [
    {
      "type": "video",
      "id": "video_avc1_1080_4807228",
      "path": "video_1080.mp4",
      "fourcc": "avc1",
      "timescale": 90000,
      "width": 1920,
      "height": 1080
    }
  ]
}
```

Each track's `id` is derived from its codec and properties (for video,
`video_<fourcc>_<height>_<bitrate>`). The server uses these ids as the
representation names in every manifest and segment URL. For the full field list,
see the [asset.json descriptor reference](../reference/asset-json.md).

## Next steps

- Add subtitles to the descriptor: [Add a subtitle track](./add-subtitles.md).
- Control how players present each track:
  [Label tracks with roles](./label-roles.md).
- Serve the descriptor: [Run and configure the server](./run-the-server.md).
- Render a manifest without the server:
  [Generate manifests without the server](./offline-manifests.md).
