# dyndo-dash

DASH MPD generation for [`dyndo`](../../README.md), built on
[`dyndo-core`](../dyndo-core/README.md).

Given an `Asset`, it produces a static (VOD) MPD as a pretty-printed XML string.
Tracks are grouped into one `AdaptationSet` per `(fourcc, language)`, each track
becoming a `Representation`, and each representation's `SegmentTimeline` is built
from the segment durations `dyndo-core` derived from the `sidx`.

## API

```rust
use dyndo_dash::generate_mpd;

// `compact` hoists SegmentTemplate content shared by all representations
// up to the AdaptationSet level.
let xml: String = generate_mpd(&asset, /* compact */ true);
```

## Modules

| Module | Responsibility |
|---|---|
| [`build`](src/build.rs) | Assemble the `MPD`: group tracks into adaptation sets, build each `Representation` and its `SegmentTimeline`, and set presentation-level timing (`minBufferTime`, `mediaPresentationDuration`). |
| [`compact`](src/compact.rs) | The optional compaction pass. When every representation in an adaptation set shares identical `SegmentTemplate` content, it is hoisted to the set level and dropped from the representations. |

## Dependencies of note

[`dash-mpd`](https://crates.io/crates/dash-mpd) for the MPD data model and
[`quick-xml`](https://crates.io/crates/quick-xml) for serialization.
