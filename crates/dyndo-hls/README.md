# dyndo-hls

HLS playlist generation for [`dyndo`](../../README.md). Turns an
`AssetDescriptor` and its probed tracks into a multivariant playlist and
per-track media playlists, using
[`hls_m3u8`](https://crates.io/crates/hls_m3u8) for the playlist model and
[`dyndo-core`](../dyndo-core/README.md) for everything about the media.

This crate knows about HLS and nothing else: no CLI, no HTTP, no CMAF parsing.
Both [`dyndo-cli`](../dyndo-cli/README.md) (for `dyndo hls`) and
[`dyndo-server`](../dyndo-server/README.md) (for the `master.m3u8` and
`<id>.m3u8` routes) call the same builders, so offline and on-the-fly playlists
are identical.

## API

```rust
use dyndo_hls::builder::{
    generate_master_playlist, generate_media_playlist, serialize_media_playlist,
};

let master = generate_master_playlist(&operator, &asset).await?;
let media = generate_media_playlist(&operator, &asset, descriptor).await?;
let text = serialize_media_playlist(&media);
```

A `MasterPlaylist` serializes correctly via `Display`. A `MediaPlaylist` must go
through `serialize_media_playlist`, which rewrites `#EXTINF` values to three
decimal places — `hls_m3u8`'s own output renders them at the precision its
`Duration` happens to carry.

## What it produces

**Multivariant playlist.** One `EXT-X-STREAM-INF` per video track, and one
`EXT-X-MEDIA` per non-video track. There are exactly two rendition groups, with
fixed ids: `audio` and `subtitles`. A variant's `BANDWIDTH` is its video track's
peak bitrate plus the peak of the highest audio rendition and the highest text
rendition, with `AVERAGE-BANDWIDTH` computed the same way from averages;
`CODECS` lists the video codec followed by every rendition codec, de-duplicated.
`CLOSED-CAPTIONS=NONE` and `EXT-X-INDEPENDENT-SEGMENTS` are always emitted.

**Media playlists.** One per track, `EXT-X-PLAYLIST-TYPE:VOD`, with an
`EXT-X-MAP` on the first segment, `EXTINF` durations to three decimals, and
`EXT-X-ENDLIST`. `EXT-X-TARGETDURATION` is the longest segment duration rounded
half-up to whole seconds. Segment URIs are `<id>/init.mp4` and
`<id>/<time>.m4s`, matching the DASH manifest and the server's segment routes.

## Roles

`roles.rs` derives a rendition's `NAME` (its language plus a human-readable role
label), its `CHARACTERISTICS`, and its `FORCED` flag. `DEFAULT` goes to the
first `main`-role audio track, or else the first audio track with no role;
`AUTOSELECT` is set when a rendition is that default or is the only one sharing
its language, forced flag, and characteristics. Two renditions in one group may
not resolve to the same `NAME` — `HlsError::DuplicateRenditionName` if they do.

The full mapping is documented in the book:
**[Track roles](https://matvp91.github.io/dyndo/reference/roles.html)**. For the
playlists' structure, see
**[`dyndo hls`](https://matvp91.github.io/dyndo/reference/cli/hls.html)**.
