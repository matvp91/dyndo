# Generate manifests without the server

The server renders manifests on the fly, but the CLI can also write them to
disk from an `asset.json`. This is useful for inspecting the exact XML or
playlist a descriptor produces, diffing manifests across changes, or validating
output without starting a server.

These commands only *render* manifests — they don't serve media. Players still
need the CMAF segments, which in production the server provides.

## Render a DASH manifest

```bash
dyndo dash -i asset.json -o stream.mpd
```

```text
wrote stream.mpd
```

The output is a static MPD describing every representation in the asset:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="urn:mpeg:dash:profile:isoff-live:2011" type="static" mediaPresentationDuration="PT22M50.325S" minBufferTime="PT1.962S" …>
  <Period id="0" start="PT0S">
    <AdaptationSet id="0" contentType="video" segmentAlignment="true" mimeType="video/mp4" startWithSAP="1">
      <Representation id="video_1080_avc1_4807228" bandwidth="4807228" width="1920" height="1080" frameRate="25" codecs="avc1.640028">
        <SegmentTemplate media="$RepresentationID$/$Time$.m4s" initialization="$RepresentationID$/init.mp4" timescale="90000" presentationTimeOffset="0">
          <SegmentTimeline>
            <S t="0" d="172800" r="355"/>
            …
          </SegmentTimeline>
        </SegmentTemplate>
      </Representation>
    </AdaptationSet>
    …
  </Period>
</MPD>
```

### Compact the manifest

By default each `Representation` carries its own `SegmentTemplate`. The
`-c/--compact` flag hoists a `SegmentTemplate` shared by all representations up
to the `AdaptationSet` level, producing a smaller manifest:

```bash
dyndo dash -i asset.json -o stream.mpd --compact
```

In compact output the `SegmentTemplate` precedes the `Representation` elements
within each set:

```xml
<AdaptationSet id="0" contentType="video" segmentAlignment="true" mimeType="video/mp4" startWithSAP="1">
  <SegmentTemplate media="$RepresentationID$/$Time$.m4s" initialization="$RepresentationID$/init.mp4" timescale="90000" presentationTimeOffset="0">
    <SegmentTimeline>
      <S t="0" d="172800" r="355"/>
      …
    </SegmentTimeline>
  </SegmentTemplate>
  <Representation id="video_1080_avc1_4807228" bandwidth="4807228" width="1920" height="1080" frameRate="25" codecs="avc1.640028"/>
</AdaptationSet>
```

> The server always renders DASH in compact form. Use `--compact` here when you
> want the CLI output to match what the server would serve.

## Render HLS playlists

HLS is a *set* of files — a multivariant playlist plus one media playlist per
advertised track — so `dyndo hls` writes to a **directory** rather than a
single file:

```bash
dyndo hls -i asset.json -o hls
```

```text
wrote hls/ (1 master + 3 media)
```

The directory contains the multivariant playlist and one media playlist per
video and audio track, named by track `id` (text tracks are not yet advertised
in HLS — see the [`dyndo hls` reference](../reference/cli/hls.md)):

```text
hls/
├── index.m3u8                        # multivariant (master) playlist
├── video_1080_avc1_4807228.m3u8      # one media playlist per track
├── video_720_avc1_3205265.m3u8
└── audio_nld_2_mp4a_196918.m3u8
```

Each media playlist references the segments by the same relative URLs the server
uses (`<id>/init.mp4`, `<id>/<time>.m4s`):

```text
#EXTM3U
#EXT-X-VERSION:6
#EXT-X-TARGETDURATION:2
#EXT-X-PLAYLIST-TYPE:VOD
#EXT-X-MAP:URI="video_1080_avc1_4807228/init.mp4"
#EXTINF:1.92,
video_1080_avc1_4807228/0.m4s
#EXTINF:1.92,
video_1080_avc1_4807228/172800.m4s
…
#EXT-X-ENDLIST
```

## Next steps

- Serve manifests dynamically instead:
  [Run and configure the server](./run-the-server.md).
- Full options: [`dyndo dash`](../reference/cli/dash.md) and
  [`dyndo hls`](../reference/cli/hls.md).
- Why the segment URLs are identical across protocols:
  [One source, two protocols](../explanation/two-protocols.md).
