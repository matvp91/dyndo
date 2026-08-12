# Terminology

This explanation defines the terms used to describe an asset and the work dyndo
does with it. The distinctions are intentional: they determine which type owns
an operation and which operations a track supports.

```text
Asset
├── source tracks
│   ├── CMAF
│   └── timed text
│       ├── WebVTT
│       └── IMSC1
└── thumbnail tracks
    └── thumbnail sprites (images)
```

## Source tracks

A **source track** is backed by an input stored with the asset. CMAF tracks are
already fragmented ISO-BMFF media. Timed-text tracks are subtitle documents;
WebVTT is supported today and IMSC1 belongs to the same category when it is
added. Every source track has a path and can be resolved from storage.

The stored model keeps source and thumbnail tracks distinct: `Track::Source`
contains a `SourceTrack`, while `Track::Thumbnail` contains a
`ThumbnailTrack`. Only `SourceTrack` has a path. Its variants are `CmafTrack`
and `TimedTextTrack`.

After resolution, `ResolvedTrack` preserves the configured track form. Its
variants are `Cmaf`, `TimedText`, and `Thumbnail`. A `ResolvedAsset` contains
the resolved tracks and splice boundaries of one asset. `CmafKind` remains
`Video`, `Audio`, or `Text`; `TimedTextFormat` is `WebVtt` today and can gain
`Imsc1` when supported. CMAF's `Text` kind is the container-level media
category, independent of the timed-text document format.

Shared sum types such as `Track` and `SourceTrack` live directly in `track`.
Representation-specific types have one public home: CMAF types live in
`track::cmaf`, timed-text types in `track::timed_text`, and thumbnail types in
`track::thumbnail`. Media metadata shared by these representations lives in
`track::metadata`.

## Assets, tracks, and discovery

An **asset** is persisted configuration. `Asset` serializes directly to
`asset.json`; it owns asset-wide splice boundaries and a collection of `Track`
values. Each track names its source form, gives it a stable identifier, and
supplies metadata that is not necessarily present in the source file.

`Track`, `SourceTrack`, and `ThumbnailTrack` implement serialization directly.
Only a source track has a source path, so operations that read a file accept a
source track rather than a general track.

When a track is configured, its type is authoritative: a `webvtt` track is
resolved as raw WebVTT, and a `video`, `audio`, or `text` track is resolved as
CMAF. The file name does not override that decision.

**Discovery** is the configuration-free operation used when indexing a new source.
It determines the source form from the input: `.vtt` is discovered as raw
WebVTT; other supported inputs are discovered as CMAF. Discovery creates the
stable source identifier that is then recorded in the asset.

The `type` property in `asset.json` is a serialized source discriminator. It
selects the resolver: `webvtt` means a raw WebVTT document, whereas `video`,
`audio`, and `text` mean CMAF sources. It is not the playback type used by a
resolved track.

Every `ResolvedTrack` has a playback **type** and a source **format**:

| Resolved track | Type | Format |
|---|---|---|
| CMAF video | `video` | `cmaf` |
| CMAF audio | `audio` | `cmaf` |
| CMAF text | `text` | `cmaf` |
| Raw WebVTT | `text` | `webvtt` |
| Thumbnail | `thumbnail` | `thumbnail` |

`TimedTextFormat` describes a timed-text document format such as WebVTT. It
maps to the broader track format while keeping every timed-text source in the
`text` playback type.

## Resolution and filtering

The server first reads persisted configuration with `Asset::read`. Asset-wide
outputs such as an MPD or HLS multivariant playlist call `Asset::resolve`, then
apply any track filter to the resulting `ResolvedAsset`. Resolving before
filtering lets thumbnails bind to source video even when that video is later
excluded from the output.

Track-specific outputs do not resolve the full asset. An HLS media playlist or
media segment calls `Asset::resolve_track` for the requested identifier. A
thumbnail is the exception: resolving one thumbnail must inspect candidate
video sources because source selection is part of thumbnail resolution.

There is no resolver service type. The loaded `Asset` already owns the asset
path, track configuration, and splice boundaries; the storage operator is the
only additional input needed for resolution.

## Operation ownership

Operations live on the value that has the information needed to perform them:

- `Asset::read` and `Asset::write` persist asset configuration.
- `Asset::resolve` creates a `ResolvedAsset` for asset-wide operations.
- `Asset::resolve_track` resolves one configured track by identifier.
- `SourceTrack::resolve` resolves a configured source using its declared form
  and metadata.
- `ResolvedTrack::discover` identifies an unconfigured input for the indexing
  workflow.
- `TimedTextTrack::resolve`, `CmafTrack::resolve`, and
  `ThumbnailTrack::resolve` implement representation-specific resolution.
- `ResolvedTimedTextTrack::from_web_vtt_text` creates a resolved WebVTT track
  when the document is already in memory.
- `ResolvedTimedTextTrack::package_wvtt` creates the CMAF representation needed
  by HLS or DASH.
- `ResolvedCmafTrack::read_range` reads CMAF bytes without requiring callers to
  know where those bytes are stored.
- `Subtitle::slice` performs subtitle-document slicing independently of track
  and manifest concerns.

There is no separate probe or reader service. Resolution and reading are
capabilities of the track types themselves.

## Thumbnail tracks

A **thumbnail track** is derived from source video when requested and has no
independent source path. `ThumbnailTrack` carries its identifier and sprite
settings but no source location. Resolution selects a suitable video source and
creates a `ResolvedThumbnailTrack`. That track samples frames along the video
timeline to produce thumbnail sprites.

## Thumbnails and images

A **thumbnail** is a playback purpose: it provides visual navigation through a
presentation. A thumbnail track produces time-addressable **thumbnail sprites**.

An **image** is a payload format, such as the JPEG sprite returned by a
thumbnail request. It is not the kind of track configured by the asset. The
track therefore uses `"type": "thumbnail"`; HLS and DASH still describe
the resulting output as image media where their specifications require it.

## CMAF representations

A timed-text source can be packaged temporarily into CMAF when a CMAF manifest
or segment is needed. The result is a `ResolvedCmafTrack`, not a separate
package type, not a new source track, and not a file written beside the asset.
Packaging converts a WebVTT source into a temporary CMAF representation.
Packaging is one-way: a CMAF source is not unpacked into a WebVTT document, so
raw WebVTT output is available only from a WebVTT source.

A `ResolvedCmafTrack` can have either of two byte backings:

- A resolved CMAF source is backed by its stored file.
- CMAF produced from timed text is backed by in-memory bytes.

Both representations expose the same index, metadata, and `read_range`
operation. A stored representation returns its path from `source_path`; an
in-memory representation returns no path. Consequently, an in-memory CMAF
representation cannot be added to an `Asset` as a source track.

The asset-wide resolution flow is:

```text
asset.json
  → Asset::read
  → Asset::resolve
  → ResolvedAsset
  → filter resolved tracks
  → manifest
```

Track-specific resolution avoids resolving unrelated tracks:

```text
asset.json
  → Asset::read
  → Asset::resolve_track
  → ResolvedTrack
  → requested CMAF, timed-text, or thumbnail representation
```
