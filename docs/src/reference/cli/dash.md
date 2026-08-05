# dyndo dash

Generate a DASH MPD from an `asset.json`.

## Synopsis

```text
dyndo dash [OPTIONS]
```

## Options

| Option | Description | Default |
|---|---|---|
| `-i, --input <INPUT>` | Input `asset.json` path. | `asset.json` |
| `-o, --output <OUTPUT>` | Output manifest path. | `stream.mpd` |
| `-h, --help` | Print help. | |

## Description

`dash` reads the descriptor at `--input`, parses each track's CMAF header to
recover its segment index, and writes a static MPD to `--output`:

```text
wrote stream.mpd
```

The CLI and the server share one manifest builder, so the XML written here is
byte-for-byte what the server's [`index.mpd` route](../server/routes.md)
returns for the same descriptor and the same segmentation options.

## Manifest structure

The manifest is `type="static"` (video on demand) with profile
`urn:mpeg:dash:profile:isoff-live:2011`, and contains exactly one `Period`
(`id="0"`, starting at `PT0S`).

| Attribute | Value |
|---|---|
| `mediaPresentationDuration` | The longest **video** track's duration; if the asset has no video, the longest **audio** track's. Text tracks never determine it. |
| `minBufferTime` | The longest single segment duration across the audio and video tracks. |
| `Period@duration` | Same as `mediaPresentationDuration`. |

Each `AdaptationSet` carries `segmentAlignment="true"`, `startWithSAP="1"`, its
`contentType` (`video`, `audio`, or `text`) and the matching `mimeType`
(`video/mp4`, `audio/mp4`, `application/mp4`). Audio and text sets also carry a
`lang` attribute.

`Representation` elements carry the descriptor's `id` and `codec` verbatim, plus
a `bandwidth` equal to the track's **peak** segment bitrate. Video adds `width`,
`height`, and `frameRate` (the descriptor's `frame_rate` ratio, e.g. `25/1`);
audio adds `audioSamplingRate` and an `AudioChannelConfiguration` descriptor
using scheme `urn:mpeg:dash:23003:3:audio_channel_configuration:2011`.

## Adaptation set grouping

Tracks are grouped into an `AdaptationSet` by a key covering everything DASH
requires to be uniform within a set:

| Content type | Grouping key |
|---|---|
| Video | sample entry, timescale |
| Audio | sample entry, timescale, language, role, sample rate, channel count |
| Text | sample entry, timescale, language, role |

The *sample entry* is the codec string up to its first `.` — `avc1.640028` and
`avc1.64001f` both group as `avc1`, while `ac-3` has no parameters and groups
as itself.

Every member of a set must be segment-aligned: the same earliest presentation
time and the same sequence of segment durations. When they are not, the command
aborts:

```text
tracks in an adaptation set are not segment-aligned
```

## Segment addressing

Each set carries one `SegmentTemplate`, written once at the `AdaptationSet`
level and shared by all of its representations:

```xml
<SegmentTemplate media="$RepresentationID$/$Time$.m4s"
                 initialization="$RepresentationID$/init.mp4"
                 timescale="90000" presentationTimeOffset="0">
  <SegmentTimeline>
    <S t="0" d="172800" r="355"/>
    <S d="10800"/>
  </SegmentTimeline>
</SegmentTemplate>
```

`timescale` and `presentationTimeOffset` come from the set's first member. The
`SegmentTimeline` lists one `S` per served segment, with consecutive equal
durations collapsed into a repeat count `r`; only the first `S` carries a `t`.
Segment URLs resolve to `<id>/init.mp4` and `<id>/<time>.m4s`, matching the
[server's segment routes](../server/routes.md).

## Roles

A track's role becomes a `Role` and/or `Accessibility` descriptor on its
`AdaptationSet`, both using scheme `urn:mpeg:dash:role:2011`. Notably, the two
accessibility audio roles emit an `Accessibility` descriptor *instead of* a
`Role`, and a text track always carries `Role@value="subtitle"`. See the
[Track roles reference](../roles.md) for the complete mapping.

## Text tracks

CMAF `wvtt` text tracks are advertised with a full timeline. Raw `.vtt` sources
also appear, but with an **empty** `SegmentTimeline` — converting raw WebVTT
into a servable track is not implemented yet.

## Examples

```bash
dyndo dash -i asset.json -o stream.mpd
```

## See also

- [Generate manifests without the server](../../how-to/offline-manifests.md).
- [`dyndo hls`](./hls.md) — the HLS equivalent.
