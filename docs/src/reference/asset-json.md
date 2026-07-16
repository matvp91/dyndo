# asset.json descriptor

The `asset.json` descriptor is the contract between the CLI and the server. The
CLI ([`index`](./cli/index.md), [`pack`](./cli/pack.md)) writes it; the server
reads it to generate manifests and locate segments. It is deliberately small: it
records **per-track metadata and a source path, and nothing else** — no segment
list, no byte offsets, no init range. Those are re-derived from each source at
read time.

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

Track order is not significant to the server. The CLI groups tracks by type
(video, then audio, then text).

## Segmentation

Both fields are optional and control how each track's CMAF fragments are
grouped into served segments. Grouping is applied when manifests and segments
are served — the CMAF files are never modified, so these fields can be edited
at any time.

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
| `id` | string | Representation id (see [Representation ids](#representation-ids)). |
| `path` | string | Source CMAF file path, relative to the descriptor's directory. |
| `fourcc` | string | Sample-entry four-character code (e.g. `avc1`, `mp4a`, `wvtt`). |
| `timescale` | integer | Units per second for durations in this track. |

Type-specific fields follow.

### Video tracks

| Field | Type | Description |
|---|---|---|
| `width` | integer | Visual width, in pixels. |
| `height` | integer | Visual height, in pixels. |

```json
{
  "type": "video",
  "id": "video_avc1_1080_4807228",
  "path": "video_1080.mp4",
  "fourcc": "avc1",
  "timescale": 90000,
  "width": 1920,
  "height": 1080
}
```

### Audio tracks

| Field | Type | Description |
|---|---|---|
| `sample_rate` | integer | Sampling rate, in Hz. |
| `channels` | integer | Number of audio channels (e.g. `2` for stereo, `6` for 5.1). |
| `language` | string *(optional)* | ISO 639-2 language code. Omitted if absent; `index` always writes it, defaulting to `und`. |

```json
{
  "type": "audio",
  "id": "audio_mp4a_nld_2_196918",
  "path": "audio_nl.mp4",
  "fourcc": "mp4a",
  "timescale": 48000,
  "sample_rate": 48000,
  "channels": 2,
  "language": "nld"
}
```

### Text tracks

| Field | Type | Description |
|---|---|---|
| `language` | string | ISO 639-2 language code (`und` when unspecified). |

```json
{
  "type": "text",
  "id": "text_wvtt_eng",
  "path": "text_wvtt_eng.mp4",
  "fourcc": "wvtt",
  "timescale": 1000,
  "language": "eng"
}
```

The descriptor's `language` is **authoritative** for text tracks: it overrides
the language recorded inside the CMAF file. Editing this field relabels the
track in every generated manifest without re-packing. Setting it to an empty
string falls back to the language declared in the file, and if that too is
absent, to `und`. Because the track's `id` incorporates the effective language,
changing the language changes the `id` the manifests use.

## Representation ids

The `id` is derived from the track's codec and salient properties:

| Type | Pattern | Example |
|---|---|---|
| Video | `video_<fourcc>_<height>_<bitrate>` | `video_avc1_1080_4807228` |
| Audio | `audio_<fourcc>_<language>_<channels>_<bitrate>` | `audio_mp4a_nld_2_196918` |
| Text | `text_<fourcc>_<language>` | `text_wvtt_eng` |

`<bitrate>` is the average bitrate in bits per second, computed from the source's
segment sizes and total duration. The id is the representation name in every
manifest and the `<repr>` component of every segment URL.

## Path resolution

`path` is always relative to the **descriptor's own directory**, normalized
(`..` segments are resolved). A descriptor at `assets/asset.json` with
`"path": "video.mp4"` refers to `assets/video.mp4`; `"path": "../shared/a.mp4"`
refers to `assets/../shared/a.mp4` → `shared/a.mp4`. This keeps a descriptor
portable: move the descriptor and its sources together and every path stays
valid.

## Complete example

An asset with one video, one audio, and one subtitle track:

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
    },
    {
      "type": "audio",
      "id": "audio_mp4a_nld_2_196918",
      "path": "audio_nl.mp4",
      "fourcc": "mp4a",
      "timescale": 48000,
      "sample_rate": 48000,
      "channels": 2,
      "language": "nld"
    },
    {
      "type": "text",
      "id": "text_wvtt_eng",
      "path": "text_wvtt_eng.mp4",
      "fourcc": "wvtt",
      "timescale": 1000,
      "language": "eng"
    }
  ]
}
```

## A note on hand-editing

The descriptor is safe to edit, but `id` and most fields are read from the
source at index time and describe it accurately. The fields intended for
hand-editing are the `language` override on text tracks and the top-level
[segmentation fields](#segmentation) (`min_segment_length`,
`segment_boundaries`), which only shape serving and never contradict the media.
If you change a source file, re-run [`index`](./cli/index.md) rather than
editing metadata by hand, so the recorded values continue to match the media.
