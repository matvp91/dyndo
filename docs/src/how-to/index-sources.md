# Index your CMAF sources

This guide shows how to build an `asset.json` descriptor from a set of media
files with `dyndo index`. You do this once per asset; the CLI and server read the descriptor afterwards.

## Before you start

`index` accepts two kinds of input, selected by file extension
(case-sensitively — `.MP4` is not recognised):

- **`.mp4`** — a CMAF track: a fragmented MP4 with a single global `sidx`
  segment index and a [supported codec](../introduction.md#supported-codecs).
  The **first** track in the `moov` is the one described; any others are ignored,
  so give each track its own file.
- **`.vtt`** — a raw WebVTT subtitle file (see
  [Add a subtitle track](./add-subtitles.md)).

An unreadable or non-conforming source aborts the run — there are no silent
fallbacks and no skip-and-continue. If you need to produce conforming CMAF
files, see the ffmpeg recipe in the
[Getting started tutorial](../tutorial/getting-started.md#step-2-create-two-cmaf-sources).

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
this is the only way to set it apart from editing the JSON. The nine valid role
values are `main`, `alternate`, `commentary`, `dub`, `description`,
`enhanced-audio-intelligibility`, `subtitle`, `caption`, and `forced-subtitle`.

Validation here is deliberately thin: an **unknown key** or an **unrecognised
role value** aborts the run, but a role is not checked against the track's type
(`audio.mp4,role=subtitle` is stored as given), and both fields on a video input
are accepted and silently ignored. Choosing a role that suits the track is up to
you — [Track roles](../reference/roles.md) shows which roles are meaningful
where.

For what each role does to the generated manifests — which rendition a player
defaults to, what it auto-selects, and the accessibility signalling — see
[Label tracks with roles](./label-roles.md).

## Add to or update an existing descriptor

Running `index` against an `asset.json` that already exists **merges** into it
rather than overwriting, keyed by each input's source path:

- a **new path** is probed from its file and appended;
- a path **already in the descriptor** keeps its entry exactly as it stands —
  the file's metadata is not re-probed, so anything you've hand-edited in the
  JSON survives. The only thing a re-index changes is what you explicitly ask
  for with `language=`/`role=` overrides.

```bash
# start with the video
dyndo index video.mp4 -o asset.json           # wrote asset.json (1 tracks)

# append an audio track
dyndo index audio.mp4 -o asset.json           # wrote asset.json (2 tracks)

# set a role on that same audio track — still two tracks, nothing else changes
dyndo index audio.mp4,role=main -o asset.json # wrote asset.json (2 tracks)
```

Two consequences of this merge model are worth knowing:

- Only the inputs you name on this run are opened. Loading the existing
  descriptor just parses its JSON, so a re-index succeeds even if a
  previously indexed source has since moved or been deleted.
- If a source file's **content** changed, `index` won't notice: remove the
  track's entry from the JSON (or delete the descriptor) and index the file
  afresh. Likewise, renaming a source on disk and indexing the new name appends
  a second entry (with a new id, since the id follows the path) — remove the
  stale one by hand.

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
      "id": "video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe",
      "path": "video_1080.mp4",
      "codec": "avc1.640028",
      "type": "video",
      "width": 1920,
      "height": 1080,
      "frame_rate": "25/1"
    }
  ]
}
```

Each track's `id` is `<type>_<uuid>`, where the UUID is derived from the source's
path relative to the storage root. It is therefore deterministic — the same file
at the same path always yields the same id — and unaffected by metadata, so
relabelling a track never changes it. The server uses these ids as the
representation names in every manifest and segment URL. For the full field list,
see the [asset.json descriptor reference](../reference/asset-json.md).

## Next steps

- Add subtitles to the descriptor: [Add a subtitle track](./add-subtitles.md).
- Control how players present each track:
  [Label tracks with roles](./label-roles.md).
- Serve the descriptor: [Run and configure the server](./run-the-server.md).
- Extract a frame from the indexed video: [Extract a video frame](./extract-image.md).
