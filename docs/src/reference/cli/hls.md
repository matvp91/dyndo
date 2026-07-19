# dyndo hls

Generate HLS playlists from an `asset.json` — a multivariant (master) playlist
plus one media playlist per advertised track — into an output directory.

## Synopsis

```text
dyndo hls [OPTIONS]
```

## Options

| Option | Description | Default |
|---|---|---|
| `-i, --input <INPUT>` | Input `asset.json` path. | `asset.json` |
| `-o, --output <OUTPUT>` | Output **directory** for the playlists. | `hls` |
| `-h, --help` | Print help. | |

## Description

HLS is a set of files, so `--output` is a directory rather than a single file.
`hls` writes:

- `index.m3u8` — the multivariant playlist, listing every video variant and
  advertising the audio renditions via `EXT-X-MEDIA`; and
- `<id>.m3u8` — one media playlist per advertised track, named by track `id`.

```text
wrote hls/ (1 master + 3 media)
```

The advertised tracks are the asset's **video and audio** tracks. Text tracks
are not yet advertised in HLS — subtitle renditions (chunked on the fly from
raw `.vtt`, or served from CMAF `wvtt`) are still being wired up and currently
get neither an `EXT-X-MEDIA` entry nor a media playlist.

Each media playlist is `EXT-X-PLAYLIST-TYPE:VOD`, begins with an `EXT-X-MAP`
pointing at the init segment, lists each segment with its `EXTINF` duration, and
ends with `EXT-X-ENDLIST`. Segment URLs are `<id>/init.mp4` and
`<id>/<time>.m4s`, the same paths the [server](../server/routes.md) and the
[DASH manifest](./dash.md) use.

## Examples

```bash
dyndo hls -i asset.json -o hls
```

Resulting layout:

```text
hls/
├── index.m3u8
├── video_1080_avc1_4807228.m3u8
├── video_720_avc1_3205265.m3u8
└── audio_nld_2_mp4a_196918.m3u8
```

## Notes

- Audio renditions are grouped by sample-entry code (`GROUP-ID`, e.g. `mp4a`,
  `ec-3`). Two audio tracks sharing a sample entry but differing in codec
  profile (for example AAC-LC vs HE-AAC) collapse into one group whose `CODECS`
  reflects the first track seen.
- Track roles drive rendition selection and accessibility signalling: the
  default audio rendition is the first `main`-role track (or the first audio
  track when none is `main`), opt-in audio roles are not auto-selected, and
  accessibility roles carry `CHARACTERISTICS` attributes. A rendition's `NAME`
  is its language, qualified by its role when one is set (e.g. `nld (main)`).
  See the [Track roles reference](../roles.md).
- Every variant's `BANDWIDTH` is the video track's bandwidth plus the
  highest-bandwidth rendition of its audio group.
- The `#EXT-X-INDEPENDENT-SEGMENTS` tag is emitted in the multivariant playlist.

## See also

- [Generate manifests without the server](../../how-to/offline-manifests.md).
- [`dyndo dash`](./dash.md) — the DASH equivalent.
