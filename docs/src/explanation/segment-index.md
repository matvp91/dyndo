# Reading a source: headers and the segment index

The [thin-pointer approach](./thin-pointer.md) only works if dyndo can recover a
source's entire segment layout cheaply and repeatedly. This page explains how it
does that: by reading a small header region and re-deriving the segment index
from the `sidx`, without ever touching the media payload.

## The shape of a CMAF track

A CMAF track file is laid out, roughly, as:

```text
┌───────┬─────────────┬────────┬──────┬──────┬──────┬──────┬─────
│ ftyp  │    moov     │  sidx  │ moof │ mdat │ moof │ mdat │ …
└───────┴─────────────┴────────┴──────┴──────┴──────┴──────┴─────
  brand   track header  segment  ─── media fragments (the bulk) ───
          (one track)   index
```

- **`ftyp` + `moov`** — the initialization data: brands, and the single track's
  timescale, codec sample entry, and handler. Together these are the *init
  segment*.
- **`sidx`** — the segment index: one reference per (sub)segment, each giving
  that segment's byte size and duration.
- **`moof` + `mdat` pairs** — the media fragments. The `mdat` boxes hold the
  actual coded samples and account for essentially the whole file.

Everything dyndo needs to describe and address the track lives in the first
three boxes. The `mdat` bodies matter only when a *specific* segment is
requested — and then only that segment's byte range.

## Probing: read the header, stop at the first fragment

To index or serve a track, dyndo streams the file from the start and parses
boxes until it has the `moov`, the `sidx`, and the first `moof` — then stops. The
first `moof` marks the end of the header region; the parser never reads the
`mdat` that follows it. Boxes it doesn't care about are skipped by length.

From those boxes it assembles:

- the **track metadata** (codec and its [RFC 6381](https://datatracker.ietf.org/doc/html/rfc6381)
  parameters, dimensions or sample rate/channels, language) from the `moov`'s
  single track and the first fragment's timing; and
- the **segment index** from the `sidx`.

## Re-deriving the segment map from the `sidx`

The `sidx` is what makes the descriptor able to omit the segment list entirely.
Each of its references gives a segment's size in bytes and duration in the track
timescale. Walking the references in order, dyndo reconstructs, for every
segment:

- its **byte offset** — a running sum of the preceding segment sizes, starting
  just after the `sidx`; and
- its **presentation time** — a running sum of the preceding durations, starting
  at the `sidx`'s earliest presentation time.

Those two running sums are exactly what a manifest needs (the `$Time$` values in
a DASH `SegmentTimeline`, the segment URIs in an HLS media playlist) and what a
segment request needs (the byte range to read for a given `<time>`). The `sidx`
*is* the segment map; dyndo reads it, never copies it.

Per-segment millisecond durations are computed from cumulative timescale
boundaries rather than by rounding each segment independently, so a track's
per-segment durations sum exactly to its total — no accumulated rounding drift.

By default every `sidx` reference becomes one served segment. The descriptor's
optional [segmentation fields](../reference/asset-json.md#segmentation)
(`min_segment_length`, `segment_boundaries`) group consecutive references into
larger served segments at serve time — contiguous byte ranges merge into one —
without touching the source file or the index itself.

## Why an 800 MB file parses like an 8 MB one

The header region — `moov` + `sidx` + first `moof` — is a fixed, small part of
the file regardless of how long the track is. A longer track has a longer
`sidx` (one reference per segment) and more `mdat` bytes, but dyndo reads the
`sidx` and stops before the `mdat`. Parsing cost tracks the number of segments,
not the size of the media, so an 800 MB source is parsed from roughly the same
~10 KB header region as an 8 MB one. The `mdat` body is never fetched during
indexing or manifest generation.

## Reading a segment

When a player later requests `<repr>/<time>.m4s`, dyndo re-derives the index the
same way, finds the segment whose cumulative start time equals `<time>`, and
issues a single byte-range read for that segment's `offset..offset+size`. Init
segments (`init.mp4`) are the `ftyp`+`moov` range at the front of the file. In
both cases dyndo reads only the bytes that segment occupies — the rest of the
file is never transferred.

## See also

- [The thin-pointer approach](./thin-pointer.md) — why the descriptor can be
  this small.
- [asset.json descriptor](../reference/asset-json.md) — what indexing records.
- [HTTP routes](../reference/server/routes.md) — how `<time>` addresses a
  segment.
