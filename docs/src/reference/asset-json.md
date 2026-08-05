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

Track order is preserved as written and is **significant**: within the HLS audio
group, the default rendition is the first `main`-role track, or the first audio
track with no role when none is marked `main`. `index` appends tracks in the
order you pass them.

## Segmentation

Both fields are optional and control how each track's CMAF fragments are
grouped into served segments. Grouping is applied when manifests and segments
are served — the CMAF files are never modified, so these fields can be edited
at any time. When unset (or `0` / empty), they are omitted from the written
descriptor.

| Field | Type | Description |
|---|---|---|
| `min_segment_length` | integer *(optional)* | Minimum length of a served segment, in **milliseconds**. Whole fragments (for video: GOPs) are grouped until a segment reaches at least this length — fragment boundaries are never split. Omitted or `0`: every fragment is served as its own segment. The last segment before a splice point or the end of the track may be shorter. |
| `segment_boundaries` | array of integers *(optional)* | Splice points, in **milliseconds** from the start of the presentation, e.g. for ad insertion. A served segment never spans one, so a segment edge exists at every splice point. Treated as a set: order and duplicates don't matter. Each point is snapped per track to the nearest fragment boundary (audio fragment rasters cannot hit arbitrary millisecond positions); an exact tie snaps earlier. |

`segment_boundaries` only takes effect when `min_segment_length` is non-zero —
with the default of `0` every fragment is already its own segment, so every
fragment edge is a segment edge.

## Track object

Each track is tagged by a `type` discriminator: `"video"`, `"audio"`, or
`"text"`. All track types share these fields:

| Field | Type | Description |
|---|---|---|
| `type` | string | Track kind: `video`, `audio`, or `text`. |
| `id` | string | Representation id (see [Representation ids](#representation-ids)). Used verbatim as the representation name in every manifest and as the `<track-id>` component of every segment URL. |
| `path` | string | Source file path, relative to the descriptor's directory. |
| `codec` | string | The track's [RFC 6381](https://datatracker.ietf.org/doc/html/rfc6381) codec string, probed from the source (e.g. `avc1.640028`, `mp4a.40.2`, `wvtt`). Written into the manifests as-is. |

Unknown fields are ignored on read. Type-specific fields follow.

### Video tracks

| Field | Type | Description |
|---|---|---|
| `width` | integer | Visual width, in pixels. |
| `height` | integer | Visual height, in pixels. |
| `frame_rate` | string | Frame rate as a reduced `numerator/denominator` ratio (e.g. `25/1`, `30000/1001`), derived from the track timescale and its first fragment's sample duration. |

Video tracks carry no `language` or `role`.

```json
{
  "id": "video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe",
  "path": "video_1080.mp4",
  "codec": "avc1.640028",
  "type": "video",
  "width": 1920,
  "height": 1080,
  "frame_rate": "25/1"
}
```

### Audio tracks

| Field | Type | Description |
|---|---|---|
| `sample_rate` | integer | Sampling rate, in Hz. |
| `channels` | integer | Number of audio channels (e.g. `2` for stereo, `6` for 5.1). |
| `language` | string | Language code as declared by the source's `mdhd` box, or `und` when the source declares none. Always written. |
| `role` | string *(optional)* | The track's declared purpose. Omitted when unset. See [Track roles](./roles.md). |

```json
{
  "id": "audio_e7f831b7-7992-5c5b-9b45-428b82d90704",
  "path": "audio_nl.mp4",
  "codec": "mp4a.40.2",
  "type": "audio",
  "sample_rate": 48000,
  "channels": 2,
  "language": "nld",
  "role": "main"
}
```

### Text tracks

A text track's source is WebVTT in one of two forms: **CMAF `wvtt`** (WebVTT in
ISO-BMFF), which is probed and served like any other CMAF track, or a **raw
`.vtt`** file.

> Raw `.vtt` sources are accepted by `index` and appear in both manifests, but
> they carry no segments yet: the conversion from raw WebVTT to a servable
> `wvtt` track is a stub. A raw-`.vtt` track's HLS media playlist is empty
> (`#EXT-X-TARGETDURATION:0`, no segments) and its DASH `SegmentTimeline` has no
> entries. Use CMAF `wvtt` sources for subtitles that must actually play. The
> descriptor format below is the same for both and will not change as the
> conversion lands.

| Field | Type | Description |
|---|---|---|
| `language` | string | Language code from the source's `mdhd` box for CMAF `wvtt`, or `und` for a raw `.vtt` (WebVTT declares no language). Always written. |
| `role` | string *(optional)* | The track's declared purpose. Omitted when unset, which DASH renders as `subtitle`. See [Track roles](./roles.md). |

A CMAF `wvtt` track and a raw `.vtt` track:

```json
{
  "id": "text_3b519953-3963-56be-8c59-ae1cd0e6d5b4",
  "path": "text_wvtt_eng.mp4",
  "codec": "wvtt",
  "type": "text",
  "language": "eng",
  "role": "caption"
}
```

```json
{
  "id": "text_c9e251a7-4fd1-54f9-abc8-ca86598e1cc5",
  "path": "subtitles_nl.vtt",
  "codec": "wvtt",
  "type": "text",
  "language": "nld"
}
```

## Representation ids

An `id` is `<type>_<uuid>`, where `<type>` is `video`, `audio`, or `text` and
`<uuid>` is a [UUID version 5](https://datatracker.ietf.org/doc/html/rfc4122#section-4.3)
generated in the URL namespace from the track's source path **relative to the
storage root**:

```text
id = "<type>_" + uuidv5(NAMESPACE_URL, "<path from storage root to the source>")
```

| Source path | Resulting id |
|---|---|
| `video_1080.mp4` | `video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe` |
| `audio_nl.mp4` | `audio_e7f831b7-7992-5c5b-9b45-428b82d90704` |
| `text_wvtt_eng.mp4` | `text_3b519953-3963-56be-8c59-ae1cd0e6d5b4` |

Two consequences follow from the id being a function of the path:

- **It is deterministic.** Indexing the same file at the same path always
  produces the same id, on any machine. Re-creating a descriptor from scratch
  reproduces the ids it had before.
- **It is independent of metadata.** Language, role, resolution, and bitrate do
  not enter into it, so relabelling a track never changes its id — and therefore
  never changes the URLs it is served under.

An id is written once, when the track is first indexed, and copied verbatim on
every later write. Indexing the *same source* through a differently placed
descriptor resolves to a different path and so mints a different id; the
existing entry, matched by path, keeps the id it already has.

Editing an `id` by hand works and is honoured everywhere, but changes every URL
the track is served under.

## Path resolution

`path` is always relative to the **descriptor's own directory**, normalized
(`..` segments are resolved). A descriptor at `assets/asset.json` with
`"path": "video.mp4"` refers to `assets/video.mp4`; `"path": "../shared/a.mp4"`
refers to `shared/a.mp4`. This keeps a descriptor portable: move the descriptor
and its sources together and every path stays valid.

Both the CLI and the server resolve that result against their storage root — for
the CLI, `OPENDAL_FS_ROOT` (default: the current directory); for the server, the
selected backend's root.

## Complete example

An asset with two video renditions, one audio track, and a CMAF `wvtt` subtitle
track:

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
    },
    {
      "id": "video_0288719a-3994-58b3-ad97-57b9ebe4227d",
      "path": "video_720.mp4",
      "codec": "avc1.64001f",
      "type": "video",
      "width": 1280,
      "height": 720,
      "frame_rate": "25/1"
    },
    {
      "id": "audio_e7f831b7-7992-5c5b-9b45-428b82d90704",
      "path": "audio_nl.mp4",
      "codec": "mp4a.40.2",
      "type": "audio",
      "sample_rate": 48000,
      "channels": 2,
      "language": "nld",
      "role": "main"
    },
    {
      "id": "text_d1cb4d6f-074b-5a03-b627-265487b4c4ea",
      "path": "text_wvtt_nld.mp4",
      "codec": "wvtt",
      "type": "text",
      "language": "nld",
      "role": "subtitle"
    }
  ]
}
```

## A note on hand-editing

The descriptor is safe to edit, and re-running [`index`](./cli/index.md) will
not undo your edits: tracks already in the descriptor keep their metadata as-is
on a re-index. The fields intended for hand-editing are `language` and `role` on
audio and text tracks, the top-level [segmentation fields](#segmentation), and
track order (which picks the HLS default audio rendition).

Two things to keep in mind:

- Metadata fields like `width`, `codec`, or `sample_rate` describe the source as
  probed; editing them does not change the media, and a value that contradicts
  the source will produce manifests players cannot use.
- Because `index` leaves known tracks untouched, it will **not** notice that a
  source file's content changed. To re-probe a track, remove its entry from the
  JSON (or delete the descriptor) and index the file again.
