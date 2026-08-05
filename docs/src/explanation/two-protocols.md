# One source, two protocols

dyndo serves the same media as both DASH and HLS. This page explains how it does
that from a single set of files, and why the two protocols share everything
except their manifests.

## Two protocols, one disagreement

DASH and HLS are both adaptive-streaming protocols: a player fetches a manifest
that lists the available renditions and their segments, then downloads segments,
switching renditions as bandwidth allows. They differ almost entirely in the
**manifest**:

- **DASH** uses an XML *Media Presentation Description* (`.mpd`) that groups
  representations into adaptation sets and describes segments with a
  `SegmentTemplate` and `SegmentTimeline`.
- **HLS** uses a set of `.m3u8` text playlists — a multivariant playlist that
  lists variants, and one media playlist per rendition listing its segments.

What they do **not** need to disagree on is the media itself. Both can play
CMAF: fragmented MP4 with an init segment and independently addressable media
segments. This is the property dyndo leans on entirely.

## Segments are shared; only the manifest is protocol-specific

Because the segments are the same CMAF for either protocol, dyndo stores and
serves exactly one set of them, and generates two manifests over them:

```text
                         ┌────────────────────────┐
   asset.json ──▶ dyndo  │  DASH manifest  (.mpd) │  protocol-specific
                         │  HLS  manifests (.m3u8)│
                         └───────────┬────────────┘
                                     │  both reference the same URLs:
                                     ▼
                    <track>/init.mp4 , <track>/<time>.m4s   shared CMAF
```

In the server this shows up directly in the [routes](../reference/server/routes.md):
only the manifest resources are protocol-specific (`index.mpd` for DASH,
`master.m3u8` and the per-track `.m3u8` playlists for HLS). The segment
resources — `<track-id>/init.mp4` and `<track-id>/<time>.m4s` — carry no protocol
component at all. There is no `dash/` or `hls/` prefix anywhere in the URL space
to distinguish them, because there would be nothing to distinguish: a segment
request returns the same bytes regardless of which manifest sent the player
there.

## The manifests agree by construction

Both manifests are generated from the same parsed source and the same
[re-derived segment index](./segment-index.md), so they necessarily describe the
same segments with the same timing and the same URLs. There is no separate
packaging step for each protocol that could fall out of sync — the DASH
`SegmentTimeline` and the HLS `EXTINF`/`EXT-X-MAP` entries are two renderings of
one index.

That is why the same `<track-id>/<time>.m4s` value appears in both a DASH
`SegmentTimeline` `$Time$` and an HLS media-playlist URI: they are computed from
the same running sum of segment durations.

## Roles render per protocol, from one source

A track's *role* — its author-declared purpose, such as a commentary audio track
or a forced-subtitle text track — is recorded once in the descriptor and then
rendered into whatever each protocol uses to express it. DASH emits `Role` and
`Accessibility` descriptors; HLS emits `DEFAULT`/`AUTOSELECT` flags and
`CHARACTERISTICS` attributes. As with segments, there is one source of truth —
the descriptor's `role` — and two renderings of it, so the two manifests describe
the same track the same way. See the [DASH](../reference/cli/dash.md) and
[HLS](../reference/cli/hls.md) references for the per-protocol output.

## Consequences

- **Adding a protocol is adding a manifest generator**, not a second copy of the
  media. Everything below the manifest — segment addressing, byte-range reads,
  the descriptor — is reused unchanged.
- **A player picks the protocol; the media is identical.** Serving an
  Apple-ecosystem client over HLS and a browser player over DASH costs nothing
  extra in storage or indexing.
- **Segment routes need no protocol knowledge**, which keeps the hot path (the
  many segment requests during playback) simple and identical for both.

## See also

- [The thin-pointer approach](./thin-pointer.md) — why there's only one copy of
  the media.
- [Reading a source: headers and the segment index](./segment-index.md) — the
  shared index both manifests render.
- [HTTP routes](../reference/server/routes.md) — where the shared/branching
  split lives in the API.
