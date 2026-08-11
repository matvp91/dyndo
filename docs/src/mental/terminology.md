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
└── derived tracks
    └── thumbnail track
        └── thumbnail sprites (images)
```

## Source tracks

A **source track** is backed by an input stored with the asset. CMAF tracks are
already fragmented ISO-BMFF media. Timed-text tracks are subtitle documents;
WebVTT is supported today and IMSC1 belongs to the same category when it is
added. A source track has a path and can be probed.

## Derived tracks

A **derived track** is created from source tracks when it is requested. It has
no independent source path. A thumbnail track is derived from a suitable video
source by sampling frames along its timeline.

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
source track and not a file written beside the asset.
