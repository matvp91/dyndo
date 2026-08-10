# dyndo dash

Generate a DASH MPD from an `asset.json`.

## Synopsis

```text
dyndo dash --input <INPUT> [OPTIONS]
```

## Options

| Option | Description | Default |
|---|---|---|
| `-i, --input <INPUT>` | Input `asset.json` path. | Required |
| `-o, --output <OUTPUT>` | Output manifest path. | `stream.mpd` |
| `--segment-min-length <MILLISECONDS>` | Minimum served segment length. | `0` |
| `--segment-text-length <MILLISECONDS>` | Length of each segment of a subtitle track packaged from a `.vtt`. | `0` |
| `--segment-boundaries <MILLISECONDS,…>` | Splice points a segment may not span. | none |
| `--compact` | Hoist common segment information in the MPD. | `false` |
| `--multi-period` | Open a `Period` at each segment boundary. | `false` |
| `--filter <EXPRESSION>` | Describe only the tracks the expression keeps. | none |
| `-h, --help` | Print help. | |

## Description

`dash` reads the descriptor at `--input`, parses each track's CMAF header to
recover its segment index, and writes a static MPD to `--output`:

```text
wrote stream.mpd
```

The CLI and the server share one manifest builder. They produce the same MPD
model for the same descriptor, segmentation options, and DASH manifest
options. The surrounding HTTP response and XML-writing context are specific to
each interface.

## Filtering tracks

`--filter` narrows which tracks the manifest describes, in the same language as
the server's [`filter` parameter](../server/routes.md#filtering-tracks):

```bash
dyndo dash -i asset.json --filter 'type!=video||height<=720'
```

Quote the expression so the shell leaves `<`, `|` and `&` alone; nothing needs
percent-encoding here. A filter matching no track is an error.

## Manifest structure

The manifest is `type="static"` (video on demand) with profile
`urn:mpeg:dash:profile:isoff-live:2011`, and contains one `Period` (`id="0"`,
starting at `PT0S`) unless [`--multi-period`](#periods) asks for more.

| Attribute | Value |
|---|---|
| `mediaPresentationDuration` | The longest **video** track's duration; if the asset has no video, the longest **audio** track's. Text tracks never determine it. |
| `minBufferTime` | The longest single segment duration across the audio and video tracks. |
| `Period@duration` | The span the period covers; the whole presentation when there is only one. |

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

By default, each `Representation` carries its own complete `SegmentTemplate`.
With `--compact`, fields shared by every representation are hoisted to the
parent `AdaptationSet`. If the templates are identical, the complete template
is written once at adaptation-set level; otherwise only identical fields are
hoisted and representation-specific fields remain below.

A fully hoisted template looks like this:

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

## Periods

`--multi-period` opens a `Period` at each
[`segment_options.boundaries`](../asset-json.md#segmentation) entry instead of
describing the asset as one. Boundaries at or beyond the presentation are
ignored, since a period of no length holds nothing.

A period is anchored on the boundary itself, never on the cut a track snapped
to. `presentationTimeOffset` moves by the boundary in each track's timescale,
while `$Time$` values remain unchanged. A segment that crosses the boundary is
referenced from both adjacent periods, allowing clients to present the part in
each period without re-timing the track against its siblings.

Each AdaptationSet after the first period refers to its predecessor. If the
boundary is an exact segment boundary, it declares
`urn:mpeg:dash:period-continuity:2015`; otherwise it declares
`urn:mpeg:dash:period-connectivity:2015`. A segment that straddles a boundary
appears in both periods, so a client can present only the portion belonging to
each period without a gap or a duplicated sample.

## Roles

A track's role becomes a `Role` and/or `Accessibility` descriptor on its
`AdaptationSet`, both using scheme `urn:mpeg:dash:role:2011`. Notably, the two
accessibility audio roles emit an `Accessibility` descriptor *instead of* a
`Role`, and a text track always carries `Role@value="subtitle"`. See the
[Track roles reference](../roles.md) for the complete mapping.

## Text tracks

Both text-track sources are advertised with a full timeline. A CMAF `wvtt` track
is indexed from its own `sidx`; a raw `.vtt` is parsed and packaged into `wvtt` as
it is read, and its timeline follows the splice points and
[`segment_options.text_length`](../asset-json.md#segmentation) rather than a
stored index.

## Examples

```bash
dyndo dash -i asset.json -o stream.mpd
```

Generate longer served segments and compact the MPD:

```bash
dyndo dash -i asset.json -o stream.mpd \
  --segment-min-length 6000 \
  --compact
```

## See also

- [Generate manifests without the server](../../how-to/offline-manifests.md).
- [`dyndo hls`](./hls.md) — the HLS equivalent.
