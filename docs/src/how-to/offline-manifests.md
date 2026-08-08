# Generate manifests without the server

The server renders manifests on the fly, but the CLI can also write them to
disk from an `asset.json`. This is useful for inspecting the exact XML or
playlist a descriptor produces, diffing manifests across changes, or validating
output without starting a server.

The CLI and the server share the same manifest builders, so they produce the
same manifest model for the same descriptor, segmentation options, and
manifest options. These commands simply do not serve the media alongside it.
Players still need the CMAF segments, which the server provides in production.

## Render a DASH manifest

```bash
dyndo dash -i asset.json -o stream.mpd --compact
```

```text
wrote stream.mpd
```

The output is a static MPD describing every representation in the asset. The
`--compact` flag hoists segment-template data shared by the representations in
an adaptation set. Without it, each representation carries its own complete
template.

When all template fields are shared, the compact output contains one
`SegmentTemplate` on the `AdaptationSet`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="urn:mpeg:dash:profile:isoff-live:2011" type="static" mediaPresentationDuration="PT22M50.32S" minBufferTime="PT1.963S">
  <Period id="0" start="PT0S" duration="PT22M50.32S">
    <AdaptationSet id="0" contentType="video" segmentAlignment="true" mimeType="video/mp4" startWithSAP="1">
      <SegmentTemplate media="$RepresentationID$/$Time$.m4s" initialization="$RepresentationID$/init.mp4" timescale="90000" presentationTimeOffset="0">
        <SegmentTimeline>
          <S t="0" d="172800" r="355"/>
          <S d="122400"/>
        </SegmentTimeline>
      </SegmentTemplate>
      <Representation id="video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe" bandwidth="16595200" width="1920" height="1080" frameRate="25/1" codecs="avc1.640028"/>
    </AdaptationSet>
    <AdaptationSet id="1" contentType="audio" lang="nld" segmentAlignment="true" mimeType="audio/mp4" startWithSAP="1">
      <Role schemeIdUri="urn:mpeg:dash:role:2011" value="main"/>
      <SegmentTemplate media="$RepresentationID$/$Time$.m4s" initialization="$RepresentationID$/init.mp4" timescale="48000" presentationTimeOffset="0">
        <SegmentTimeline>
          <S t="0" d="94208"/>
          <S d="92160" r="354"/>
        </SegmentTimeline>
      </SegmentTemplate>
      <Representation id="audio_e7f831b7-7992-5c5b-9b45-428b82d90704" bandwidth="213844" audioSamplingRate="48000" codecs="mp4a.40.2">
        <AudioChannelConfiguration schemeIdUri="urn:mpeg:dash:23003:3:audio_channel_configuration:2011" value="2"/>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>
```

The real output also carries the `xsi`, `cenc`, `mspr`, `xlink`, and `dvb`
namespace declarations on `<MPD>`, elided here for readability.

## Render HLS playlists

HLS is a *set* of files — a multivariant playlist plus one media playlist per
track — so `dyndo hls` writes to a **directory** rather than a single file. It
reports each file as it goes:

```bash
dyndo hls -i asset.json -o hls --segment-min-length 6000
```

```text
wrote hls/master.m3u8
wrote hls/video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe.m3u8
wrote hls/audio_e7f831b7-7992-5c5b-9b45-428b82d90704.m3u8
wrote hls/text_d1cb4d6f-074b-5a03-b627-265487b4c4ea.m3u8
```

Every track in the descriptor gets a media playlist, named by its `id` — text
tracks included:

```text
hls/
├── master.m3u8
├── video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe.m3u8
├── audio_e7f831b7-7992-5c5b-9b45-428b82d90704.m3u8
└── text_d1cb4d6f-074b-5a03-b627-265487b4c4ea.m3u8
```

Each media playlist references the segments by the same relative URLs the server
uses (`<id>/init.mp4`, `<id>/<time>.m4s`):

```text
#EXTM3U
#EXT-X-VERSION:6
#EXT-X-TARGETDURATION:2
#EXT-X-PLAYLIST-TYPE:VOD
#EXT-X-MAP:URI="video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe/init.mp4"
#EXTINF:1.920,
video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe/0.m4s
#EXTINF:1.920,
video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe/172800.m4s
…
#EXT-X-ENDLIST
```

> A text track gets a playlist like any other, but in HLS its segments are plain
> WebVTT documents rather than `.m4s` — `--wvtt` asks for the packaged form.
> Use `--segment-text-length` to choose how it is cut — see
> [Add a subtitle track](./add-subtitles.md).

`--segment-min-length` is shared by the DASH and HLS commands. It groups whole
fragments until the requested duration is reached, while respecting the
descriptor's [`segment_options.boundaries`](../reference/asset-json.md#segmentation).
Every segment flag overrides the matching option in that block, and
`--segment-boundaries` and `--segment-text-length` are accepted alongside it.
DASH also takes `--compact` and `--multi-period`, the latter opening a `Period`
at each boundary; HLS takes `--wvtt`.

## Next steps

- Serve manifests dynamically instead:
  [Run and configure the server](./run-the-server.md).
- Full options: [`dyndo dash`](../reference/cli/dash.md) and
  [`dyndo hls`](../reference/cli/hls.md).
- Why the segment URLs are identical across protocols:
  [One source, two protocols](../explanation/two-protocols.md).
