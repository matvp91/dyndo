# dyndo hls

Generate HLS playlists from an `asset.json` — a multivariant (master) playlist
plus one media playlist per track — into an output directory.

## Synopsis

```text
dyndo hls --input <INPUT> [OPTIONS]
```

## Options

| Option | Description | Default |
|---|---|---|
| `-i, --input <INPUT>` | Input `asset.json` path. | Required |
| `-o, --output <OUTPUT>` | Output **directory** for the playlists. | `hls` |
| `--segment-min-length <MILLISECONDS>` | Minimum served segment length. | `0` |
| `--segment-text-length <MILLISECONDS>` | Length of each segment of a subtitle track packaged from a `.vtt`. | `0` |
| `--segment-boundaries <MILLISECONDS,…>` | Splice points a segment may not span. | none |
| `--wvtt` | Point text renditions at packaged `wvtt` segments rather than WebVTT documents. | off |
| `-h, --help` | Print help. | |

## Description

HLS is a set of files, so `--output` is a directory rather than a single file.
The directory is created if it does not exist. `hls` writes:

- `master.m3u8` — the multivariant playlist; and
- `<id>.m3u8` — one media playlist per track in the descriptor, named by track
  `id`, **including text tracks**.

Each file is reported as it is written:

```text
wrote hls/master.m3u8
wrote hls/video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe.m3u8
wrote hls/audio_e7f831b7-7992-5c5b-9b45-428b82d90704.m3u8
wrote hls/text_3b519953-3963-56be-8c59-ae1cd0e6d5b4.m3u8
```

The CLI and the server share one playlist builder, so these files match what
the server's [`master.m3u8` and `<id>.m3u8` routes](../server/routes.md) return
for the same descriptor and segmentation options — pass `--wvtt` here and `wvtt`
there to get the same playlists.

## The multivariant playlist

```text
#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,URI="audio_….m3u8",GROUP-ID="audio",LANGUAGE="nld",NAME="Dutch (Main)",DEFAULT=YES,AUTOSELECT=YES,CHANNELS="2"
#EXT-X-MEDIA:TYPE=SUBTITLES,URI="text_….m3u8",GROUP-ID="subtitles",LANGUAGE="eng",NAME="English (Captions)",AUTOSELECT=YES,CHARACTERISTICS="…"
#EXT-X-STREAM-INF:BANDWIDTH=16818378,AVERAGE-BANDWIDTH=5004734,CODECS="avc1.640028,mp4a.40.2",RESOLUTION=1920x1080,FRAME-RATE=25.000,AUDIO="audio",SUBTITLES="subtitles",CLOSED-CAPTIONS=NONE
video_….m3u8
#EXT-X-INDEPENDENT-SEGMENTS
```

**Renditions.** Every non-video track becomes an `EXT-X-MEDIA` entry. There are
exactly two groups, with fixed ids: all audio tracks share `GROUP-ID="audio"`
and all text tracks share `GROUP-ID="subtitles"`. Group membership does not
depend on codec, so AAC and E-AC-3 renditions sit in the same audio group.

A rendition's `NAME` is its language, qualified by a human-readable role label
when a role is set — `nld`, `nld (Main)`, `eng (Audio Description)`. Two
renditions in the same group may not resolve to the same `NAME`; when they do
the command aborts:

```text
duplicate rendition name: eng (Main)
```

Roles also drive `DEFAULT`, `AUTOSELECT`, `FORCED`, and `CHARACTERISTICS` — see
the [Track roles reference](../roles.md) for the exact rules.

**Variants.** One `EXT-X-STREAM-INF` per **video** track; an asset with no video
track produces a multivariant playlist with renditions but no variants.

| Attribute | Value |
|---|---|
| `BANDWIDTH` | The video track's peak bitrate, plus the peak bitrate of the highest audio rendition, plus that of the highest text rendition. |
| `AVERAGE-BANDWIDTH` | The same sum computed from average bitrates. |
| `CODECS` | The video track's codec followed by every non-video track's codec, de-duplicated in first-seen order. A text rendition served as WebVTT contributes nothing, since WebVTT has no codec identifier; with `--wvtt` it contributes `wvtt`. |
| `RESOLUTION` | `<width>x<height>` from the descriptor. |
| `FRAME-RATE` | The `frame_rate` ratio evaluated and rounded to three decimals. |
| `AUDIO` / `SUBTITLES` | Present only when the asset has audio / text tracks. |
| `CLOSED-CAPTIONS` | Always `NONE` — dyndo does not signal in-band CEA-608/708 captions. |

`#EXT-X-INDEPENDENT-SEGMENTS` is always emitted.

## Media playlists

Every track gets a VOD media playlist:

```text
#EXTM3U
#EXT-X-VERSION:6
#EXT-X-TARGETDURATION:2
#EXT-X-PLAYLIST-TYPE:VOD
#EXT-X-MAP:URI="video_…/init.mp4"
#EXTINF:1.920,
video_…/0.m4s
#EXTINF:1.920,
video_…/172800.m4s
…
#EXT-X-ENDLIST
```

`EXT-X-TARGETDURATION` is the longest segment duration rounded half-up to whole
seconds. `EXTINF` durations are written to three decimal places. `EXT-X-MAP`
appears once, on the first segment. Segment URIs are `<id>/init.mp4` and
`<id>/<time>.m4s` — the same paths the [server](../server/routes.md) and the
[DASH manifest](./dash.md) use, with `<time>` the segment's presentation start
time in the track's timescale.

## Text tracks

Text tracks are advertised as `TYPE=SUBTITLES` renditions and each gets a media
playlist listing real segments. Its segments follow the splice points and
[`segment_options.text_length`](../asset-json.md#segmentation).

By default a text rendition points at **WebVTT documents** rather than packaged
segments, since that is the form HLS players handle most widely:

```text
#EXTM3U
#EXT-X-VERSION:3
#EXT-X-TARGETDURATION:4
#EXT-X-PLAYLIST-TYPE:VOD
#EXTINF:4.000,
text_…/0.vtt
#EXTINF:4.000,
text_…/4000.vtt
…
#EXT-X-ENDLIST
```

There is no `EXT-X-MAP`: a WebVTT rendition needs no initialization segment. Each
`.vtt` holds the cues on screen over its segment, with the absolute timestamps
the source document used, and no `X-TIMESTAMP-MAP` — the times are already on the
presentation's clock.

`--wvtt` points the rendition at `<id>/<time>.m4s` with an `EXT-X-MAP` instead,
which is how the track is served to DASH either way. The two forms describe the
same segments — the same cut points, the same durations — so the choice changes
only how each one is delivered. Reach for `--wvtt` when a client wants the
packaged track, or when a text track came from a `wvtt` file another packager
wrote and so has no document to serve.

## Examples

```bash
dyndo hls -i asset.json -o hls
```

Group fragments into served segments of at least six seconds where possible:

```bash
dyndo hls -i asset.json -o hls --segment-min-length 6000
```

Point text renditions at packaged `wvtt` segments instead of WebVTT documents:

```bash
dyndo hls -i asset.json -o hls --wvtt
```

Resulting layout:

```text
hls/
├── master.m3u8
├── video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe.m3u8
├── video_0288719a-3994-58b3-ad97-57b9ebe4227d.m3u8
├── audio_e7f831b7-7992-5c5b-9b45-428b82d90704.m3u8
└── text_3b519953-3963-56be-8c59-ae1cd0e6d5b4.m3u8
```

## See also

- [Generate manifests without the server](../../how-to/offline-manifests.md).
- [`dyndo dash`](./dash.md) — the DASH equivalent.
