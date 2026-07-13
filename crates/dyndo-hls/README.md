# dyndo-hls

HLS playlist generation for [`dyndo`](../../README.md), built on
[`dyndo-core`](../dyndo-core/README.md).

Given an `Asset`, it produces the two HLS playlist kinds as plain strings — a
**multivariant (master)** playlist and one **media** playlist per track. The
content is demuxed CMAF: video tracks become `EXT-X-STREAM-INF` variants and
audio tracks become `EXT-X-MEDIA` renditions, grouped by codec and linked from
each variant with `AUDIO="…"`. Playlists are static (VOD) and reference the same
CMAF segments, at the same URLs, as the DASH output — only the manifest differs.

## API

```rust
use dyndo_hls::{generate_master, generate_media};

// The multivariant playlist: variants + audio renditions.
let master: String = generate_master(&asset);          // index.m3u8

// One track's media playlist: EXT-X-MAP init + segment list, ends with ENDLIST.
let media: String = generate_media(&asset.tracks[0]);  // {repr}.m3u8
```

Both functions return the playlist as a `String` (via `hls_m3u8`'s `Display`).

## Output

For an asset with two video renditions and one AAC audio track, `generate_master`
emits:

```m3u8
#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,URI="audio_mp4a_nld_2_196918.m3u8",GROUP-ID="mp4a",LANGUAGE="nld",NAME="nld",DEFAULT=YES,AUTOSELECT=YES,CHANNELS="2"
#EXT-X-STREAM-INF:BANDWIDTH=5004146,CODECS="avc1.640028,mp4a.40.2",RESOLUTION=1920x1080,FRAME-RATE=25.000,AUDIO="mp4a"
video_avc1_1080_4807228.m3u8
#EXT-X-INDEPENDENT-SEGMENTS
```

and each `generate_media` emits a VOD playlist:

```m3u8
#EXTM3U
#EXT-X-VERSION:6
#EXT-X-TARGETDURATION:2
#EXT-X-PLAYLIST-TYPE:VOD
#EXT-X-MAP:URI="video_avc1_1080_4807228/init.mp4"
#EXTINF:1.92,
video_avc1_1080_4807228/0.m4s
...
#EXT-X-ENDLIST
```

> [!NOTE]
> Segment names (`{repr}/{time}.m4s`) and the init URI (`{repr}/init.mp4`) are
> byte-for-byte the same resources DASH addresses, where `{repr}` is the track's
> `id` and `{time}` is its running presentation time. A `dyndo-server` serving
> both protocols shares one set of segment routes.

## Modules

| Module | Responsibility |
|---|---|
| [`build`](src/build.rs) | Assemble the `MasterPlaylist` (partition tracks, group audio by codec, expand one variant per audio group) and each `MediaPlaylist` (`EXT-X-MAP` init, one segment per (sub)segment, VOD). |

## Design notes

- **Demuxed audio groups.** Audio renditions are grouped by codec fourcc
  (`GROUP-ID` = `mp4a`, `ec-3`, …); languages and channel layouts become
  renditions within a group. This is the standards-correct shape for separate
  audio/video CMAF and mirrors how DASH splits video and audio adaptation sets.
- **Variant expansion.** When an asset offers audio in more than one codec, each
  video variant is emitted once per audio group — with `CODECS` and `BANDWIDTH`
  adjusted per pairing — so a player selects a codec it can decode. In the common
  single-codec case this collapses to no duplication.
- **Edge cases.** A video-only asset yields variants with no `AUDIO`; an
  audio-only asset lists its audio tracks as plain variants (no `EXT-X-MEDIA`).

## Dependencies of note

[`hls_m3u8`](https://crates.io/crates/hls_m3u8) for the playlist data model and
its `Display`-based serialization.
