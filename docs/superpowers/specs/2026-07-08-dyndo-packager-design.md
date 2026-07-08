# dyndo — dynamic packager (design)

**Date:** 2026-07-08
**Status:** Approved for planning

## 1. Purpose & scope

`dyndo` is a CLI that reads one or more CMAF files and emits a single
`asset.json` describing their tracks. That descriptor is consumed by a
**dynamic packaging server** which, at request time, generates DASH (and later
HLS) manifests and serves media segments via byte-range reads from the
*original* CMAF files.

`dyndo` **only** produces `asset.json`. It does **not** emit DASH/HLS — that is
the downstream server's job. "DASH-first, HLS-ready" therefore governs only
*which fields* the model carries, not any output `dyndo` generates.

Invocation:

```
dyndo -i index_video_avc_1080.mp4 -i index_video_avc_720.mp4 -i index_audio_aac_nl_2.mp4 -o asset.json
```

`-i` is repeatable (one track per input file); `-o` defaults to `asset.json`
in the current working directory.

### Input contract (strict)

Every input file MUST:

- parse as an MP4/CMAF file,
- contain a `moov` with **exactly one** track,
- contain a `sidx` (segment index),
- use a codec we understand (AVC video / AAC audio for the first cut).

Any violation is a typed error and aborts the run — no silent fallbacks, no
skip-and-continue.

### Guiding principle — least code, bounded memory

Use the `mp4-atom` crate for CMAF box parsing rather than writing our own. It
exposes typed `sidx`/`moov`/fragmented boxes and lets us decode boxes
header-first, so we read only the front `moov` + `sidx` + first `moof` (~10 KB)
and stop before any `mdat` — never loading the 800 MB body into memory. This is
the least-code path that *also* keeps memory bounded. Our only parsing code is
the thin adapter in `cmaf/` (walk the decoded boxes; assemble the codec string).
See §5, §4, §6, §10.

## 2. Segment-addressing strategy — thin pointer (Strategy 1)

`asset.json` stores **no** segment list, byte offsets, or init-segment range.
Per track it carries only descriptive metadata plus the source key. The
packaging server, at asset load, reads each file's self-describing front boxes
(`ftyp` + `moov` + `sidx`, ~8.6 KB) and re-derives the full index:

- init-segment range = `[0, moov_end)`
- segment byte offsets = prefix-sum of each `reference_size` from
  `moov_end + sidx_size`
- segment timeline = prefix-sum of `sidx.subsegment_duration`

**The `sidx` is the segment map; we never copy it.** One source of truth, tiny
human-readable JSON, and the same `dyndo-core` parser serves both the CLI (to
summarise + validate) and the server (to build the runtime index).

## 3. The model (serde contract)

Top level:

```json
{ "tracks": [ … ] }
```

Discriminated union via serde internally-tagged enum (`#[serde(tag = "type")]`).

**Video track:**

```json
{
  "type": "video",
  "id": "video_avc_1080_4807",
  "source": "index_video_avc_1080.mp4",
  "codec": "avc1.640028",
  "timescale": 90000,
  "duration": 123328800,
  "bandwidth": 4807228,
  "width": 1920,
  "height": 1080,
  "frame_rate": "25/1"
}
```

**Audio track:**

```json
{
  "type": "audio",
  "id": "audio_aac_nld_2_197",
  "source": "index_audio_aac_nl_2.mp4",
  "codec": "mp4a.40.2",
  "timescale": 48000,
  "duration": 65775616,
  "bandwidth": 196918,
  "sample_rate": 48000,
  "channels": 2,
  "language": "nld"
}
```

### Fields

| Field | Type | Notes |
|---|---|---|
| `type` | `"video"` \| `"audio"` | serde tag |
| `id` | string | generated uniqueness key (§4) |
| `source` | string | backend key: local path now, `s3://…` later (§6) |
| `codec` | string | RFC6381 (`avc1.640028`, `mp4a.40.2`) |
| `timescale` | u32 | from `sidx` (90000 / 48000) |
| `duration` | u64 | ticks; sum of `sidx.subsegment_duration` |
| `bandwidth` | u32 | bits/s; average (§4) |
| `width`,`height` | u32 | video only |
| `frame_rate` | string | video only; DASH `@frameRate` form `"25/1"` (§4) |
| `sample_rate` | u32 | audio only |
| `channels` | u16 | audio only |
| `language` | string \| absent | audio only; from `mdhd` (ISO-639-2, e.g. `nld`) |

**Deliberately absent:** `sar`, any segment list, byte offsets, or init range.

## 4. Derivations

We decode `moov` and `sidx` with `mp4-atom`. `moov` gives track metadata
(resolution, sample rate, channels, language, codec config); `sidx` gives
segment timing. Derived fields:

- **`timescale`** = `Sidx.timescale`.
- **`duration`** = Σ `subsegment_duration` (authoritative in a fragmented file,
  where `moov`'s sample tables are empty).
- **`bandwidth`** = `Σ reference_size * 8 / (duration / timescale)`, rounded —
  the average bitrate, computed from the `sidx` references.
- **`codec`** (RFC6381) assembled by `cmaf/codec.rs` from decoded config: `Avcc`
  profile / compatibility / level → `avc1.PPCCLL`; `Esds` object type →
  `mp4a.40.<ot>`.
- **`frame_rate`** — a `"num/den"` string (DASH `@frameRate` form): `num` = media
  timescale, `den` = `default_sample_duration` from `mvex`/`Trex` (or the first
  `Moof`'s `Tfhd`/`Trun`); reduced, e.g. `90000/3600` → `"25/1"`.
- **`id`** — deterministic, collision-proof, generated in `model::id` from
  already-parsed fields (never from the filename). A short codec token maps the
  sample-entry fourcc: `avc1`/`avc3`→`avc`, `hvc1`/`hev1`→`hevc`, `mp4a`→`aac`,
  `ac-3`→`ac3`, `ec-3`→`ec3`. `kbps` = `round(bandwidth / 1000)`.
  - **Video:** `video_{codec}_{height}_{kbps}` → `video_avc_1080_4807`
  - **Audio:** `audio_{codec}_{language}_{channels}_{kbps}` → `audio_aac_nld_2_197`
    (`language` token is the `mdhd` value, `und` when absent)
  - After building all tracks, `dyndo` asserts every `id` is distinct and errors
    (`DuplicateTrackId`) otherwise, so a bad asset is never written.

## 5. Crate & module layout

```
dyndo/
├── Cargo.toml                # [workspace]
├── crates/
│   ├── dyndo-core/           # lib: model + parsing; no clap
│   └── dyndo-cli/            # bin "dyndo": clap wiring only
└── assets/                   # sample CMAF (not committed)
```

`dyndo-cli` depends on `dyndo-core`. Core has no CLI concerns and is reusable by
the future packaging server.

### `dyndo-core` modules

```
src/
├── lib.rs           # exports only: `mod …;` + `pub use …;`
├── asset.rs         # async orchestration: read_header → construct Track → Asset
├── error.rs         # Error (thiserror) + Result alias
├── model/
│   ├── mod.rs       # exports only
│   ├── structure.rs # serde types: Asset, Track, Video/AudioTrack
│   └── id.rs        # track-id generation (pure)
├── storage/
│   ├── mod.rs       # exports only
│   ├── source.rs    # async `Source` trait (size, read_at)
│   ├── fs.rs        # LocalFile: Source via `tokio::fs`
│   └── s3.rs        # S3Source: Source — stubbed (unimplemented) for now
└── cmaf/
    ├── mod.rs       # exports only
    ├── header.rs    # async read_header(&impl Source) -> CmafHeader { track, init_range, segments }
    └── codec.rs     # rfc6381(entry) -> String  (pure)
```

**Dependency direction (acyclic):**

- `error` — leaf.
- `model` — serde types (`structure`) + pure `id` generation; no internal deps.
- `storage` — `source.rs` defines the async `Source` trait, `fs.rs`/`s3.rs`
  implement it; depends on `error`.
- `cmaf` — depends on `error`, `storage::Source`, and `mp4-atom`; `read_header`
  is `async` and pulls bytes via `Source::read_at`.
- `asset` — orchestration: open a `Source` → `cmaf::read_header`, compute
  `bandwidth`, construct the `model::Track` (setting `id` via `model::id`), and
  collect tracks into an `Asset` with a duplicate-id guard.
- `lib` — exports only (`pub use` from the modules above).

**Seam principle:** `storage` is the only module that touches the OS / a backend
(`tokio::fs`, later the S3 SDK). `cmaf` reads bytes solely through the async
`Source` trait, so it is backend-agnostic and unit-tests against an in-memory
`Source` over a header fixture. `cmaf` returns its own `CmafHeader`/`Segment`
types, not `model::Track` — the server wants `segments`, the CLI wants a
summarised `Track`; that mapping lives in `asset.rs`. `storage` is read-only —
the CLI writes `asset.json`, not core.

## 6. `storage` — the async `Source` trait (S3-ready)

A byte source is a file or an S3 object; its native primitive is a ranged read
(`pread` / ranged GET), which is also exactly how the future server serves a
segment. The trait is async (we use `tokio`) and modelled around ranged reads:

```rust
// storage/source.rs
pub trait Source {
    async fn size(&self) -> Result<u64>;                                 // stat / HEAD
    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>>; // pread / ranged GET
}
```

- `storage/fs.rs` — `LocalFile`, implemented over `tokio::fs` (seek + read).
- `storage/s3.rs` — `S3Source`, **stubbed** now (methods return
  `Error::Backend("s3 unimplemented")`); wired and compiling, filled in later.

`cmaf::read_header` reads header-first through `Source::read_at`: read each box
header (~16 B); for `moov`, `sidx`, and the first `moof` read exactly that box's
body and decode it with `mp4-atom`'s sync API over the in-memory buffer; skip
every other box by advancing the offset without reading its body. The first
`moof` supplies `frame_rate` (its `trun` sample duration). Because these boxes
precede the first `mdat`, this fetches ~10 KB and stops — an `mdat` is never
read. No `Read`/`Seek` adapter is needed. Backend errors fold into `Error`
(`Io` for local; `Backend(String)` for S3).

## 7. Errors (`thiserror`, strict)

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error on {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("backend error: {0}")]
    Backend(String),
    #[error("no sidx box found in {0} — input must be CMAF with a segment index")]
    MissingSidx(String),
    #[error("no moov/track found in {0}")]
    MissingMoov(String),
    #[error("expected exactly one track in {path}, found {count}")]
    NotSingleTrack { path: String, count: usize },
    #[error("unsupported codec {codec:?} in {path}")]
    UnsupportedCodec { path: String, codec: String },
    #[error("malformed {box_type} box in {path}: {reason}")]
    MalformedBox { box_type: String, path: String, reason: String },
    #[error("duplicate track id {0} — inputs are not uniquely distinguishable")]
    DuplicateTrackId(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

The CLI turns any `Error` into a clean message + non-zero exit (via `anyhow` at
the top level).

## 8. CLI (`clap` derive) — minimal

Kept as thin as possible: argument parsing + one call into `dyndo-core` + write
the file. No parsing or model logic in the binary.

- `-i/--input <PATH>` — repeatable, `Vec<PathBuf>`, at least one required.
- `-o/--output <PATH>` — defaults to `asset.json` in CWD.
- `#[tokio::main] async fn main()` (the core API is async).
- Flow: parse args → `dyndo_core::build_asset(&inputs).await` (opens a
  `LocalFile` per input, calls `describe_track`, guards unique ids) →
  `serde_json` pretty-print → write to `-o`.

## 9. Testing

- **Header-only fixtures:** extract `ftyp` + `moov` + `sidx` + first `moof`
  (~10 KB) from each sample file and commit those — enough to exercise
  `read_header` end to end (including `frame_rate`) without shipping ~800 MB.
- **In-memory `Source`:** a test `Source` backed by a `Vec<u8>` (the fixture),
  so `cmaf` tests run without the filesystem (`#[tokio::test]`).
- **`cmaf::header` test:** `read_header` on a fixture yields the expected track
  metadata, segment count, and total duration.
- **`cmaf::codec` unit tests:** `rfc6381` maps `Avcc`/`Esds` fields to
  `avc1.640028` / `mp4a.40.2` (pure, no I/O).
- **`storage::fs` test:** `LocalFile::read_at` returns correct ranges from a
  temp file.
- **`model::id` unit tests:** id formatting + duplicate detection.
- **CLI integration test:** run against fixtures, assert emitted JSON shape.

## 10. Library choice & risks

- **Parsing via `mp4-atom`.** It exposes typed `Sidx` (with `reference_size` +
  `subsegment_duration` per `SegmentReference`), `Moov`, `Trak`, `Stsd`,
  `Avc1`/`Avcc`, `Mp4a`/`Esds`, and the fragmented boxes. Deps added:
  `mp4-atom` (+ its `bytes`). We only hand-write the RFC6381 codec string.
- **Bounded memory.** `read_header` fetches each box header via the async
  `Source::read_at`, then reads and decodes only the `moov`, `sidx`, and first
  `moof` bodies (sync `mp4-atom` over the in-memory buffer), skipping all other
  bodies. Since those boxes precede the first `mdat`, we read ~10 KB and stop —
  an `mdat` is never read, regardless of file size. We must NOT use
  `Any::read_from` in a blind loop (it would decode `mdat` into memory).
- **Async via `tokio`.** The runtime and local reads use `tokio` (`tokio::fs`);
  `mp4-atom` is used through its *sync* API over the small fetched buffers, so
  its `tokio` feature isn't needed. The async `Source` is what makes the S3
  backend and a future async server drop-in.
- **Frame-rate source** — `Trex`'s `default_sample_duration` is `0` in our
  inputs, so `frame_rate` comes from the first `Moof`: `Tfhd`'s
  `default_sample_duration` if set, else the first `Trun` sample duration
  (`3600` → reduced `90000/3600` = `"25/1"`).

## 11. Explicitly deferred (YAGNI)

- DASH/HLS manifest emission (server's job).
- Multi-track-per-file inputs.
- Adaptation-set / rendition-group modelling (server groups by type + language
  at manifest time).
- S3 `Source` implementation — `storage/s3.rs` stubbed now, filled in later.
- `sar` / anamorphic content (not modelled; assumed square pixels).
- Codecs beyond AVC + AAC.
