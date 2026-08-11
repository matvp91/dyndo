# Terminology

This page defines the terms used to describe an asset and the work dyndo does
with it. The distinctions are intentional: they tell us which operations a
track supports.

```text
Asset
├── source tracks
│   ├── CMAF
│   └── timed text
│       ├── WebVTT
│       └── IMSC1
└── synthetic tracks
    └── thumbnail track
        └── thumbnail sprites (images)
```

## Source tracks

A **source track** is backed by an input stored with the asset. CMAF tracks are
already fragmented ISO-BMFF media. Timed-text tracks are subtitle documents;
WebVTT is supported today and IMSC1 belongs to the same category when it is
added. A source track has a path and can be probed.

The runtime model keeps those levels separate: `SourceTrack` is either a
`CmafTrack` or a `TimedTextTrack`. Each resolved source track carries its own
kind: `CmafTrackKind` is `Video`, `Audio`, or `Text`; `TimedTextKind` is
`WebVtt` today and can gain `Imsc1` when supported. CMAF's `Text` kind remains
the container-level media category, independent of the timed-text document
format.

## Descriptors and discovery

An **asset descriptor** is persisted configuration. Its track entry names the
source form, gives it a stable identifier, and supplies metadata that is not
necessarily present in the source file.

The stored `TrackDescriptor` separates `SourceTrackDescriptor` from
`SyntheticTrackDescriptor`. Only a source descriptor has a source path, so
operations that read or probe a file accept a source descriptor rather than a
general track descriptor.

When a descriptor is present, its type is authoritative: a `webvtt` descriptor is
resolved as raw WebVTT, and a `video`, `audio`, or `text` descriptor is resolved
as CMAF. The file name does not override that decision.

**Discovery** is the descriptor-less operation used when indexing a new source.
It determines the source form from the input: `.vtt` is discovered as raw
WebVTT; other supported inputs are discovered as CMAF. Discovery creates the
stable source identifier that is then recorded in the descriptor.

The word **type** has two deliberate meanings in the system:

- An asset descriptor type is the source or synthetic form written in
  `asset.json`: `video`, `audio`, `text`, `webvtt`, or `thumbnail`.
- A CMAF media kind is the container category of a CMAF track: `video`,
  `audio`, or `text`.

For example, `vtt` is a timed-text document format, while CMAF `text` is the
container category used when that document is packaged as `wvtt`.

## Synthetic tracks

A **synthetic track** is derived from source tracks when requested and has no
independent source path. Its descriptor is a
`SyntheticTrackDescriptor`, which carries its identifier and kind but no source
location. Resolution creates a `SyntheticTrack` with a `SyntheticTrackKind`;
today that kind is `Thumbnail`. A thumbnail track is derived from a suitable
video source by sampling frames along its timeline.

## Thumbnails and images

A **thumbnail** is a playback purpose: it provides visual navigation through a
presentation. A thumbnail track produces time-addressable **thumbnail sprites**.

An **image** is a payload format, such as the JPEG sprite returned by a
thumbnail request. It is not the kind of track configured by the asset. The
descriptor therefore uses `"type": "thumbnail"`; HLS and DASH still describe
the resulting output as image media where their specifications require it.

## CMAF packages

A timed-text source can be packaged temporarily into CMAF when a CMAF manifest
or segment is needed. That package is a runtime representation, not a new
source track and not a file written beside the asset. Probing resolves a source
track; packaging converts a raw WebVTT source into its temporary CMAF
representation. Packaging is one-way: a CMAF source is not unpacked into a
WebVTT document, so raw WebVTT output is available only from a WebVTT source.

The complete resolution flow is therefore:

```text
asset descriptor
  → source track
  → temporary CMAF package, when a CMAF representation is required
  → manifest or segment representation requested by the client
```
