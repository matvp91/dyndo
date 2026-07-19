# asset.json descriptor

The `asset.json` descriptor is the contract between the CLI and the server. The
CLI ([`index`](./cli/index.md)) writes it; the server reads it to generate
manifests and locate segments. It is deliberately small: it records **per-track
metadata and a source path, and nothing else** — no segment list, no byte
offsets, no timescale. Those are re-derived from each source at read time.

The file is pretty-printed JSON and safe to read, diff, and hand-edit.

## Top-level structure

A descriptor is an object with a `tracks` array and two optional segmentation
fields:

```json
{
  "min_segment_length": 3000,
  "segment_boundaries": [683640],
  "tracks": [ /* track objects */ ]
}
```

Track order is preserved as written and is **significant**: within an HLS audio
group, the default rendition is the first `main`-role track, or the first track
when none is marked `main`. `index` appends tracks in the order you pass them.

## Segmentation

Both fields are optional and control how each track's CMAF fragments are
grouped into served segments. Grouping is applied when manifests and segments
are served — the CMAF files are never modified, so these fields can be edited
at any time. When unset, they are omitted from the written descriptor.

| Field | Type | Description |
|---|---|---|
| `min_segment_length` | integer *(optional)* | Minimum length of a served segment, in **milliseconds**. Whole fragments (for video: GOPs) are grouped until a segment reaches at least this length — fragment boundaries are never split. Omitted or `0`: every fragment is served as its own segment. The last segment before a splice point or the end of the track may be shorter. |
| `segment_boundaries` | array of integers *(optional)* | Splice points, in **milliseconds** from the start of the presentation, e.g. for ad insertion. A served segment never spans one, so a segment edge exists at every splice point. Treated as a set: order and duplicates don't matter. Each point is snapped per track to the nearest fragment boundary (audio fragment rasters cannot hit arbitrary millisecond positions); an exact tie snaps earlier. |

## Track object

Each track is tagged by a `type` discriminator: `"video"`, `"audio"`, or
`"text"`. All track types share these fields:

| Field | Type | Description |
|---|---|---|
| `type` | string | Track kind: `video`, `audio`, or `text`. |
| `id` | string | Representation id (see [Representation ids](#representation-ids)). The key must be present; an **empty string** is filled with a freshly generated id whenever the descriptor is read. |
| `path` | string | Source file path, relative to the descriptor's directory. |
| `fourcc` | string *(written, ignored on read)* | Sample-entry four-character code (e.g. `avc1`, `mp4a`, `wvtt`), written as a debugging aid. It is recomputed from the probed source on every write and ignored when the descriptor is read. Raw `.vtt` tracks have no sample entry, so the field is omitted for them. |

Unknown fields are ignored on read. Type-specific fields follow.

### Video tracks

| Field | Type | Description |
|---|---|---|
| `width` | integer | Visual width, in pixels. |
| `height` | integer | Visual height, in pixels. |

```json
{
  "id": "video_1080_avc1_4807228",
  "path": "video_1080.mp4",
  "type": "video",
  "width": 1920,
  "height": 1080,
  "fourcc": "avc1"
}
```

### Audio tracks

| Field | Type | Description |
|---|---|---|
| `sample_rate` | integer | Sampling rate, in Hz. |
| `channels` | integer | Number of audio channels (e.g. `2` for stereo, `6` for 5.1). |
| `language` | string | ISO 639-2 language code. Defaults to `und` when absent and is always written. |
| `role` | string *(optional)* | The track's declared purpose. Omitted when unset. One of `main`, `alternate`, `commentary`, `dub`, `description`, `enhanced-audio-intelligibility`. See [Track roles](./roles.md). |

```json
{
  "id": "audio_nld_2_mp4a_196918",
  "path": "audio_nl.mp4",
  "type": "audio",
  "sample_rate": 48000,
  "channels": 2,
  "language": "nld",
  "role": "main",
  "fourcc": "mp4a"
}
```

### Text tracks

A text track's source is WebVTT in one of two forms, and dyndo does the rest at
serve time: a **raw `.vtt`** file is chunked, and **CMAF `wvtt`** (WebVTT in
ISO-BMFF) is served like any other CMAF track. Both forms sit in the descriptor
the same way; a raw `.vtt` entry simply has no `fourcc`.

> Text-track serving is still being completed: CMAF `wvtt` tracks are advertised
> in DASH manifests today, while HLS subtitle renditions and the on-the-fly
> chunking of raw `.vtt` sources are not wired up yet. The descriptor format
> below is stable either way.

| Field | Type | Description |
|---|---|---|
| `language` | string | ISO 639-2 language code. Defaults to `und` when absent and is always written. |
| `role` | string *(optional)* | The track's declared purpose. Omitted when unset (rendered as `subtitle`). One of `subtitle`, `caption`, `forced-subtitle`. See [Track roles](./roles.md). |

A CMAF `wvtt` track and a raw `.vtt` track:

```json
{
  "id": "text_eng_wvtt_586",
  "path": "text_wvtt_eng.mp4",
  "type": "text",
  "language": "eng",
  "role": "caption",
  "fourcc": "wvtt"
}
```

```json
{
  "id": "text_und",
  "path": "subtitles_nl.vtt",
  "type": "text",
  "language": "nld"
}
```

## Representation ids

The `id` is derived from the track's properties when the track is first probed:

| Type | Pattern | Example |
|---|---|---|
| Video | `video_<height>_<fourcc>_<bandwidth>` | `video_1080_avc1_4807228` |
| Audio | `audio_<language>_<channels>_<fourcc>_<bandwidth>` | `audio_nld_2_mp4a_196918` |
| Text (CMAF `wvtt`) | `text_<language>_<fourcc>_<bandwidth>` | `text_eng_wvtt_586` |
| Text (raw `.vtt`) | `text_<language>` | `text_und` |

`<bandwidth>` is the average bitrate in bits per second, computed from the
source's segment sizes and total duration. The id is the representation name in
every manifest and the `<repr>` component of every segment URL.

**Ids are pinned.** An id is generated once, when the track is first indexed,
from the values probed out of the source at that moment — and written verbatim
ever after. Later edits to the descriptor (a corrected `language`, an added
`role`) deliberately do **not** re-derive the id, so segment URLs stay stable
for anything already consuming the stream. The parts of an id are a naming
convention, not live metadata. Note that a raw `.vtt` id takes its language
from the probe — WebVTT files declare none, so it is `und` even when you set
`language=` at index time.

## Path resolution

`path` is always relative to the **descriptor's own directory**, normalized
(`..` segments are resolved). A descriptor at `assets/asset.json` with
`"path": "video.mp4"` refers to `assets/video.mp4`; `"path": "../shared/a.mp4"`
refers to `assets/../shared/a.mp4` → `shared/a.mp4`. This keeps a descriptor
portable: move the descriptor and its sources together and every path stays
valid.

## Complete example

An asset with two video renditions, one audio track, and a subtitle track:

```json
{
  "tracks": [
    {
      "id": "video_1080_avc1_4807228",
      "path": "video_1080.mp4",
      "type": "video",
      "width": 1920,
      "height": 1080,
      "fourcc": "avc1"
    },
    {
      "id": "video_720_avc1_3205265",
      "path": "video_720.mp4",
      "type": "video",
      "width": 1280,
      "height": 720,
      "fourcc": "avc1"
    },
    {
      "id": "audio_nld_2_mp4a_196918",
      "path": "audio_nl.mp4",
      "type": "audio",
      "sample_rate": 48000,
      "channels": 2,
      "language": "nld",
      "role": "main",
      "fourcc": "mp4a"
    },
    {
      "id": "text_und",
      "path": "subtitles_nl.vtt",
      "type": "text",
      "language": "nld"
    }
  ]
}
```

## A note on hand-editing

The descriptor is safe to edit, and re-running [`index`](./cli/index.md) will
not undo your edits: tracks already in the descriptor keep their metadata as-is
on a re-index. The fields intended for hand-editing are the `language` and
`role` on audio and text tracks, the top-level
[segmentation fields](#segmentation), and track order (which picks the HLS
default rendition). Editing an `id` also works but changes every URL under
which the track is served.

Two things to keep in mind:

- Metadata fields like `width` or `sample_rate` describe the source as probed;
  editing them does not change the media.
- Because `index` leaves known tracks untouched, it will **not** notice that a
  source file's content changed. To re-probe a track, remove its entry from the
  JSON (or delete the descriptor) and index the file again.
