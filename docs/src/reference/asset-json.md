# asset.json descriptor

The `asset.json` descriptor is the shared contract between dyndo's CLI and server. [`dyndo index`](./cli/index.md) writes and updates it; [`dyndo image`](./cli/image.md) and the server read it. It is deliberately small: it records **per-track metadata and, for source tracks, a source path** — no segment list, byte offsets, or timescale. Those are re-derived from each source at read time.

The file is pretty-printed JSON and safe to read, diff, and hand-edit.

## Top-level structure

A descriptor is an object with a `tracks` array and an optional block of segment
options. The array contains CMAF source tracks, raw WebVTT source tracks, and
derived thumbnail tracks:

```json
{
  "segment_options": {
    "min_length": 6000,
    "boundaries": [683640]
  },
  "tracks": [ /* CMAF, VTT, and thumbnail track objects */ ]
}
```

Track order is preserved as written and is **significant**: within the HLS audio
group, the default rendition is the first `main`-role track, or the first audio
track with no role when none is marked `main`. `index` appends tracks in the
order you pass them.

## Thumbnail tracks

Thumbnail tracks describe JPEG sprite sheets derived from the source tracks.
They have no source path or codec: dyndo resolves their source at request time.
Each thumbnail track has a stable `id`:

```json
{
  "id": "preview",
  "type": "thumbnail",
  "tile_size": 4,
  "width": 640,
  "step": 1000
}
```

`tile_size` is the number of thumbnails in each row and column, `width` is the
full sprite width in pixels, and `step` is the interval between adjacent
thumbnails in milliseconds.

## Segmentation

The optional `segment_options` block records how the asset asks to be segmented. It is what the asset *asks for*, not a decision: a server request option overrides any option it names, and the descriptor is never written back. A block equal to the defaults is omitted from the file.

| Field | Type | Default | Description |
|---|---|---|---|
| `min_length` | integer | `0` | Minimum served segment length, in **milliseconds**. Whole fragments are grouped until this length is reached; `0` serves every fragment as its own segment. |
| `text_length` | integer | `0` | Length of each segment of a subtitle track dyndo packages from a `.vtt`. Unlike `min_length` this is exact, since dyndo fragments those tracks itself. `0` asks for no grid, leaving the splice points as the only cuts. |
| `boundaries` | array of integers | `[]` | Splice points, in **milliseconds** from the start of the presentation, e.g. for ad insertion. A served segment never spans one, so a segment edge exists at every splice point. Treated as a set: order and duplicates don't matter. Each point is snapped per track to the first fragment boundary at or after it (audio fragment rasters cannot hit arbitrary millisecond positions), so a segment never opens on content from before the splice point. |

Grouping is applied while generating manifests and serving segments; stored CMAF
files are never modified.

`boundaries` only changes video and audio output when a non-zero `min_length`
groups multiple fragments — with the default of `0`, every fragment is already a
served segment. Subtitle tracks are different: dyndo cuts them at the splice
points as it packages them, whatever `min_length` says.

Descriptors use only the field names shown above. Server request options accept
additional shorthand and legacy spellings; see the
[server routes reference](server/routes.md#segmentation-options).

## Track objects

Each track is tagged by a `type` discriminator: `"video"`, `"audio"`,
`"text"`, `"vtt"`, or `"thumbnail"`. CMAF tracks (`video`, `audio`, and `text`)
share these fields:

| Field | Type | Description |
|---|---|---|
| `type` | string | Track kind: `video`, `audio`, or `text`. |
| `id` | string | Representation id (see [Representation ids](#representation-ids)). Combined with the track type when written as a representation name or segment URL. |
| `path` | string | Source file path, relative to the descriptor's directory. |
| `codec` | string | The track's [RFC 6381](https://datatracker.ietf.org/doc/html/rfc6381) codec string, probed from the source (e.g. `avc1.640028`, `mp4a.40.2`, `wvtt`). Written into the manifests as-is. |

Unknown fields in a track entry are ignored on read. Type-specific fields follow.

### Video tracks

| Field | Type | Description |
|---|---|---|
| `width` | integer | Visual width, in pixels. |
| `height` | integer | Visual height, in pixels. |
| `frame_rate` | string | Frame rate as a reduced `numerator/denominator` ratio (e.g. `25/1`, `30000/1001`), derived from the track timescale and its first fragment's sample duration. |

Video tracks carry no `language` or `role`.

```json
{
  "id": "6b745be5-2791-5d95-8ce5-8f8bde29e2fe",
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
| `language` | string | Well-formed BCP 47 language tag declared by the source's `mdhd` box, or `und` when the source declares none or the field is omitted. Always written. |
| `role` | string *(optional)* | The track's declared purpose. Omitted when unset. See [Track roles](./roles.md). |

```json
{
  "id": "e7f831b7-7992-5c5b-9b45-428b82d90704",
  "path": "audio_nl.mp4",
  "codec": "mp4a.40.2",
  "type": "audio",
  "sample_rate": 48000,
  "channels": 2,
  "language": "nld",
  "role": "main"
}
```

### Text and VTT tracks

A text track's source is WebVTT in one of two forms: **CMAF `wvtt`** (WebVTT in
ISO-BMFF), which is probed and served like any other CMAF track, or a **raw
`.vtt`** file.

> Both forms are served. A raw `.vtt` is parsed and packaged into a `wvtt` track
> as it is read, so nothing is written beside it and the `.vtt` stays the source
> of truth; its segments are cut where the asset's splice points and
> [`text_length`](#segmentation) say. CMAF WebVTT tracks use `"type": "text"`
> and record their `wvtt` codec. Raw WebVTT tracks use `"type": "vtt"` and do
> not have a codec, because dyndo packages them only for CMAF output. HLS returns
> raw VTT cues directly and unpacks CMAF `wvtt` segments when needed
> unless the request asks for `wvtt`; see
> [Add a subtitle track](../how-to/add-subtitles.md#choose-how-hls-delivers-subtitles).

| Field | Type | Description |
|---|---|---|
| `language` | string | Well-formed BCP 47 language tag from the source's `mdhd` box for CMAF `wvtt`, or `und` for a raw `.vtt` or an omitted field. Always written. |
| `role` | string *(optional)* | The track's declared purpose. Omitted when unset, which DASH renders as `subtitle`. See [Track roles](./roles.md). |

A CMAF `wvtt` track and a raw `.vtt` track:

```json
{
  "id": "3b519953-3963-56be-8c59-ae1cd0e6d5b4",
  "path": "text_wvtt_eng.mp4",
  "codec": "wvtt",
  "type": "text",
  "language": "eng",
  "role": "caption"
}
```

```json
{
  "id": "c9e251a7-4fd1-54f9-abc8-ca86598e1cc5",
  "path": "subtitles_nl.vtt",
  "type": "vtt",
  "language": "nld"
}
```

## Representation ids

An `id` is a [UUID version 5](https://datatracker.ietf.org/doc/html/rfc4122#section-4.3)
generated in the URL namespace from the track's source path **relative to the
storage root**:

```text
id = uuidv5(NAMESPACE_URL, "<path from storage root to the source>")
```

| Source path | Resulting id |
|---|---|
| `video_1080.mp4` | `6b745be5-2791-5d95-8ce5-8f8bde29e2fe` |
| `audio_nl.mp4` | `e7f831b7-7992-5c5b-9b45-428b82d90704` |
| `text_wvtt_eng.mp4` | `3b519953-3963-56be-8c59-ae1cd0e6d5b4` |

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
      "id": "6b745be5-2791-5d95-8ce5-8f8bde29e2fe",
      "path": "video_1080.mp4",
      "codec": "avc1.640028",
      "type": "video",
      "width": 1920,
      "height": 1080,
      "frame_rate": "25/1"
    },
    {
      "id": "0288719a-3994-58b3-ad97-57b9ebe4227d",
      "path": "video_720.mp4",
      "codec": "avc1.64001f",
      "type": "video",
      "width": 1280,
      "height": 720,
      "frame_rate": "25/1"
    },
    {
      "id": "e7f831b7-7992-5c5b-9b45-428b82d90704",
      "path": "audio_nl.mp4",
      "codec": "mp4a.40.2",
      "type": "audio",
      "sample_rate": 48000,
      "channels": 2,
      "language": "nld",
      "role": "main"
    },
    {
      "id": "d1cb4d6f-074b-5a03-b627-265487b4c4ea",
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
