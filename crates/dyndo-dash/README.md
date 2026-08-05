# dyndo-dash

DASH manifest generation for [`dyndo`](../../README.md). Turns an
`AssetDescriptor` and its probed tracks into a static MPD, using
[`dash-mpd`](https://crates.io/crates/dash-mpd) for the XML model and
[`dyndo-core`](../dyndo-core/README.md) for everything about the media.

This crate knows about DASH and nothing else: no CLI, no HTTP, no CMAF parsing.
Both [`dyndo-cli`](../dyndo-cli/README.md) (for `dyndo dash`) and
[`dyndo-server`](../dyndo-server/README.md) (for the `index.mpd` route) call the
same builder, so offline and on-the-fly manifests are identical.

## API

```rust
use dyndo_dash::builder::generate_mpd;

let mpd = generate_mpd(&operator, &asset).await?;
```

`generate_mpd` probes every track in the descriptor, groups them into adaptation
sets, and returns a `dash_mpd::MPD`. Serializing it to XML is the caller's job —
both binaries use `quick-xml` with two-space indentation.

## What it produces

A `type="static"` manifest on the `urn:mpeg:dash:profile:isoff-live:2011`
profile, with one `Period` containing one `AdaptationSet` per group of
compatible tracks. Each set carries a single `SegmentTemplate` — hoisted to the
set level and shared by its representations — whose `SegmentTimeline` is derived
from the sources' `sidx` boxes, with equal consecutive durations collapsed into
repeat counts.

Tracks group into an adaptation set by everything DASH requires to be uniform
within one: sample entry and timescale for video; additionally language, role,
sample rate, and channel count for audio; language and role for text. Members of
a set must be segment-aligned — the same earliest presentation time and the same
sequence of segment durations — or `generate_mpd` returns
`DashError::SegmentAlignment`.

## Roles

`roles.rs` maps a `dyndo_core::role::Role` onto `Role` and `Accessibility`
descriptors, both under the `urn:mpeg:dash:role:2011` scheme. The two
accessibility audio roles emit an `Accessibility` descriptor *instead of* a
`Role`, and text tracks always carry `Role@value="subtitle"` unless they are
forced subtitles.

The full mapping is documented in the book:
**[Track roles](https://matvp91.github.io/dyndo/reference/roles.html)**. For the
manifest's structure, see
**[`dyndo dash`](https://matvp91.github.io/dyndo/reference/cli/dash.html)**.
