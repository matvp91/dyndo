# dyndo Packager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Rust CLI `dyndo -i a.mp4 -i b.mp4 -o asset.json` that reads CMAF files and emits a thin `asset.json` describing their tracks.

**Architecture:** A `dyndo-core` library (model + async CMAF header parsing over a `Source` abstraction) and a minimal `dyndo-cli` binary. Parsing uses the `mp4-atom` crate, reading only `moov` + `sidx` + the first `moof` (~10 KB) and stopping before any `mdat`, so memory stays bounded regardless of file size. `asset.json` stores no segment list — the `sidx` remains the source of truth (see the design spec).

**Tech Stack:** Rust (edition 2021), `mp4-atom`, `serde`/`serde_json`, `thiserror`, `tokio`, `clap`, `anyhow`.

**Spec:** `docs/superpowers/specs/2026-07-08-dyndo-packager-design.md`

## Global Constraints

- **Workspace:** two crates, `crates/dyndo-core` (lib) and `crates/dyndo-cli` (bin named `dyndo`). CLI depends on core; core has no CLI/clap deps.
- **`mod.rs` and `lib.rs` are exports only** — `mod …;` + `pub use …;`, no logic.
- **Self-explanatory names.** Functions say what they do (`read_header`, `describe_track`, `video_track_id`, `avc_codec_string`).
- **Async via `tokio`.** The `Source` trait is async; `main` is `#[tokio::main]`. `mp4-atom` is used through its **sync** API over in-memory buffers (no `mp4-atom` `tokio` feature).
- **Bounded memory.** Never read an `mdat` body. Read box headers, and only the `moov`/`sidx`/first-`moof` bodies. Never use `mp4_atom::Any::read_from` in a blind loop.
- **Strict input contract.** Errors (never silent fallback) when: not parseable, no `moov`, not exactly one track, no `sidx`, unsupported codec.
- **Model field names are snake_case** (no serde `rename_all` on fields). Enum tag uses `rename_all = "lowercase"` for `"video"`/`"audio"`.
- **`codec`** is RFC6381 lowercase hex (`avc1.640028`, `mp4a.40.2`). **`frame_rate`** is a reduced `"num/den"` string. **`bandwidth`** is the average from `sidx`: `round(Σ reference_size * 8 / (duration/timescale))`. **`kbps` for ids** = `(bandwidth + 500) / 1000`.
- **`language`** is the raw `mdhd` ISO-639-2 value (e.g. `nld`); omit from JSON when absent.
- TDD, one behaviour per test, frequent commits.

---

### Task 1: Workspace scaffold, dependencies, fixtures

**Files:**
- Create: `Cargo.toml` (workspace), `crates/dyndo-core/Cargo.toml`, `crates/dyndo-core/src/lib.rs`, `crates/dyndo-cli/Cargo.toml`, `crates/dyndo-cli/src/main.rs`
- Create: `crates/dyndo-core/tests/fixtures/index_video_avc_1080.mp4`, `crates/dyndo-core/tests/fixtures/index_audio_aac_nl_2.mp4`
- Create: `.gitignore`

**Interfaces:**
- Produces: a building workspace; header-only fixtures for later tasks.

- [ ] **Step 1: Initialise git and workspace**

```bash
cd /Users/matvp/Development/dyndo
git init
printf '/target\n' > .gitignore
```

Create `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/dyndo-core", "crates/dyndo-cli"]
```

- [ ] **Step 2: Scaffold `dyndo-core`**

`crates/dyndo-core/Cargo.toml`:

```toml
[package]
name = "dyndo-core"
version = "0.1.0"
edition = "2021"

[dependencies]
mp4-atom = "*"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
tokio = { version = "1", features = ["fs", "io-util", "rt", "macros"] }

[dev-dependencies]
serde_json = "1"
tempfile = "3"
```

`crates/dyndo-core/src/lib.rs` (placeholder, replaced in later tasks):

```rust
```

(Leave `lib.rs` empty for now.)

- [ ] **Step 3: Scaffold `dyndo-cli`**

`crates/dyndo-cli/Cargo.toml`:

```toml
[package]
name = "dyndo-cli"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "dyndo"
path = "src/main.rs"

[dependencies]
dyndo-core = { path = "../dyndo-core" }
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "fs"] }
serde_json = "1"
anyhow = "1"
```

`crates/dyndo-cli/src/main.rs`:

```rust
fn main() {}
```

- [ ] **Step 4: Pin `mp4-atom` to the latest concrete version**

Run: `cargo add mp4-atom -p dyndo-core` (replaces the `"*"`). If `Header`, `Moov`, `Sidx`, `Moof`, `ReadFrom`, `ReadAtom` are not resolvable after building, check the crate docs for the correct version and API.

- [ ] **Step 5: Build the empty workspace**

Run: `cargo build`
Expected: PASS (compiles both crates).

- [ ] **Step 6: Extract header-only fixtures (ftyp+moov+sidx+first moof)**

Run this script (cuts each source file at the first `mdat`, keeping ftyp+moov+sidx+first moof):

```bash
mkdir -p crates/dyndo-core/tests/fixtures
python3 - <<'PY'
import struct
def cut(src, dst):
    with open(src,'rb') as f:
        off=0
        while True:
            hdr=f.read(8)
            if len(hdr)<8: raise SystemExit(f"no mdat in {src}")
            size=struct.unpack('>I',hdr[:4])[0]
            typ=hdr[4:8]
            if size==1:
                size=struct.unpack('>Q',f.read(8))[0]
            if typ==b'mdat':
                break
            off+=size; f.seek(off)
        f.seek(0); data=f.read(off)
    open(dst,'wb').write(data)
    print(f"{dst}: {len(data)} bytes")
cut("assets/index_video_avc_1080.mp4","crates/dyndo-core/tests/fixtures/index_video_avc_1080.mp4")
cut("assets/index_audio_aac_nl_2.mp4","crates/dyndo-core/tests/fixtures/index_audio_aac_nl_2.mp4")
PY
```

Expected: video fixture ≈ 10058 bytes, audio fixture ≈ 10478 bytes.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "chore: scaffold dyndo workspace, deps, and header fixtures"
```

---

### Task 2: `error` module

**Files:**
- Create: `crates/dyndo-core/src/error.rs`
- Modify: `crates/dyndo-core/src/lib.rs`

**Interfaces:**
- Produces: `dyndo_core::Error` (enum), `dyndo_core::Result<T>`.

- [ ] **Step 1: Write the failing test**

In `crates/dyndo-core/src/error.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_sidx_message_names_the_file() {
        let e = Error::MissingSidx("a.mp4".into());
        assert!(e.to_string().contains("a.mp4"));
        assert!(e.to_string().contains("sidx"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dyndo-core error::`
Expected: FAIL (compile error — `Error` not defined).

- [ ] **Step 3: Write the implementation**

Prepend to `crates/dyndo-core/src/error.rs`:

```rust
//! Crate-wide error type. Every input-contract violation is a typed error.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

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
    MalformedBox {
        box_type: String,
        path: String,
        reason: String,
    },

    #[error("duplicate track id {0} — inputs are not uniquely distinguishable")]
    DuplicateTrackId(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

Set `crates/dyndo-core/src/lib.rs` to:

```rust
mod error;

pub use error::{Error, Result};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dyndo-core error::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): add strict Error enum and Result alias"
```

---

### Task 3: `model/structure.rs` — serde types

**Files:**
- Create: `crates/dyndo-core/src/model/structure.rs`, `crates/dyndo-core/src/model/mod.rs`
- Modify: `crates/dyndo-core/src/lib.rs`

**Interfaces:**
- Produces: `Asset`, `Track` (enum `Video(VideoTrack)`/`Audio(AudioTrack)`), `VideoTrack`, `AudioTrack`, and `Track::id(&self) -> &str`.

- [ ] **Step 1: Write the failing test**

In `crates/dyndo-core/src/model/structure.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_track_serialises_with_type_tag_and_snake_case() {
        let asset = Asset {
            tracks: vec![Track::Video(VideoTrack {
                id: "video_avc_1080_4807".into(),
                source: "index_video_avc_1080.mp4".into(),
                codec: "avc1.640028".into(),
                timescale: 90000,
                duration: 123328800,
                bandwidth: 4807228,
                width: 1920,
                height: 1080,
                frame_rate: "25/1".into(),
            })],
        };
        let json = serde_json::to_value(&asset).unwrap();
        let t = &json["tracks"][0];
        assert_eq!(t["type"], "video");
        assert_eq!(t["frame_rate"], "25/1");
        assert_eq!(t["width"], 1920);
    }

    #[test]
    fn audio_language_absent_is_omitted_and_round_trips() {
        let track = Track::Audio(AudioTrack {
            id: "audio_aac_und_2_197".into(),
            source: "a.mp4".into(),
            codec: "mp4a.40.2".into(),
            timescale: 48000,
            duration: 65775616,
            bandwidth: 196918,
            sample_rate: 48000,
            channels: 2,
            language: None,
        });
        let json = serde_json::to_value(&track).unwrap();
        assert!(json.get("language").is_none());
        let back: Track = serde_json::from_value(json).unwrap();
        assert_eq!(back, track);
        assert_eq!(back.id(), "audio_aac_und_2_197");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dyndo-core model::structure`
Expected: FAIL (types not defined).

- [ ] **Step 3: Write the implementation**

Prepend to `crates/dyndo-core/src/model/structure.rs`:

```rust
//! The `asset.json` serde contract.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Track {
    Video(VideoTrack),
    Audio(AudioTrack),
}

impl Track {
    pub fn id(&self) -> &str {
        match self {
            Track::Video(t) => &t.id,
            Track::Audio(t) => &t.id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoTrack {
    pub id: String,
    pub source: String,
    pub codec: String,
    pub timescale: u32,
    pub duration: u64,
    pub bandwidth: u32,
    pub width: u32,
    pub height: u32,
    pub frame_rate: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTrack {
    pub id: String,
    pub source: String,
    pub codec: String,
    pub timescale: u32,
    pub duration: u64,
    pub bandwidth: u32,
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}
```

Create `crates/dyndo-core/src/model/mod.rs`:

```rust
mod structure;

pub use structure::{Asset, AudioTrack, Track, VideoTrack};
```

Update `crates/dyndo-core/src/lib.rs`:

```rust
mod error;
mod model;

pub use error::{Error, Result};
pub use model::{Asset, AudioTrack, Track, VideoTrack};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dyndo-core model::structure`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): add serde model types (Asset/Track/Video/AudioTrack)"
```

---

### Task 4: `model/id.rs` — track id generation

**Files:**
- Create: `crates/dyndo-core/src/model/id.rs`
- Modify: `crates/dyndo-core/src/model/mod.rs`

**Interfaces:**
- Produces: `model::id::video_track_id(codec: &str, height: u32, bandwidth: u32) -> String`, `model::id::audio_track_id(codec: &str, language: Option<&str>, channels: u16, bandwidth: u32) -> String`.

- [ ] **Step 1: Write the failing test**

In `crates/dyndo-core/src/model/id.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_id_uses_codec_token_height_and_kbps() {
        assert_eq!(video_track_id("avc1.640028", 1080, 4807228), "video_avc_1080_4807");
    }

    #[test]
    fn audio_id_uses_language_channels_and_kbps() {
        assert_eq!(
            audio_track_id("mp4a.40.2", Some("nld"), 2, 196918),
            "audio_aac_nld_2_197"
        );
    }

    #[test]
    fn audio_id_defaults_absent_language_to_und() {
        assert_eq!(
            audio_track_id("mp4a.40.2", None, 2, 196918),
            "audio_aac_und_2_197"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dyndo-core model::id`
Expected: FAIL (functions not defined).

- [ ] **Step 3: Write the implementation**

Prepend to `crates/dyndo-core/src/model/id.rs`:

```rust
//! Deterministic, collision-proof track ids, derived from parsed fields only.

/// Short codec family token derived from the RFC6381 codec string.
fn codec_token(codec: &str) -> &'static str {
    if codec.starts_with("avc1") || codec.starts_with("avc3") {
        "avc"
    } else if codec.starts_with("hvc1") || codec.starts_with("hev1") {
        "hevc"
    } else if codec.starts_with("mp4a") {
        "aac"
    } else if codec.starts_with("ac-3") {
        "ac3"
    } else if codec.starts_with("ec-3") {
        "ec3"
    } else {
        "unknown"
    }
}

/// Bandwidth in kilobits per second, rounded to nearest.
fn kbps(bandwidth: u32) -> u32 {
    (bandwidth + 500) / 1000
}

pub fn video_track_id(codec: &str, height: u32, bandwidth: u32) -> String {
    format!("video_{}_{}_{}", codec_token(codec), height, kbps(bandwidth))
}

pub fn audio_track_id(codec: &str, language: Option<&str>, channels: u16, bandwidth: u32) -> String {
    format!(
        "audio_{}_{}_{}_{}",
        codec_token(codec),
        language.unwrap_or("und"),
        channels,
        kbps(bandwidth)
    )
}
```

Update `crates/dyndo-core/src/model/mod.rs`:

```rust
pub mod id;
mod structure;

pub use structure::{Asset, AudioTrack, Track, VideoTrack};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dyndo-core model::id`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): add collision-proof track id generation"
```

---

### Task 5: `storage` — async `Source` trait, S3 stub, in-memory test source

**Files:**
- Create: `crates/dyndo-core/src/storage/source.rs`, `crates/dyndo-core/src/storage/s3.rs`, `crates/dyndo-core/src/storage/mod.rs`, `crates/dyndo-core/src/test_support.rs`
- Modify: `crates/dyndo-core/src/lib.rs`

**Interfaces:**
- Produces: `storage::Source` trait (`async fn size`, `async fn read_at`), `storage::S3Source`, and `test_support::BytesSource` (test-only, `pub(crate)`).

- [ ] **Step 1: Write the failing test**

In `crates/dyndo-core/src/test_support.rs`:

```rust
//! Shared test helpers (compiled only under `cfg(test)`).

use crate::error::Result;
use crate::storage::Source;

/// An in-memory `Source` backed by a byte vector.
pub struct BytesSource {
    bytes: Vec<u8>,
}

impl BytesSource {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

impl Source for BytesSource {
    async fn size(&self) -> Result<u64> {
        Ok(self.bytes.len() as u64)
    }

    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let start = offset as usize;
        let end = (start + len).min(self.bytes.len());
        Ok(self.bytes[start..end].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_the_requested_range() {
        let s = BytesSource::new(vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(s.size().await.unwrap(), 6);
        assert_eq!(s.read_at(2, 3).await.unwrap(), vec![2, 3, 4]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dyndo-core test_support`
Expected: FAIL (`storage::Source` not defined).

- [ ] **Step 3: Write the implementation**

`crates/dyndo-core/src/storage/source.rs`:

```rust
//! The backend-agnostic byte source. Its primitive is a ranged read.

use crate::error::Result;

/// A range-addressable blob of bytes: a local file, an S3 object, …
#[allow(async_fn_in_trait)]
pub trait Source {
    /// Total size in bytes (stat / HEAD).
    async fn size(&self) -> Result<u64>;
    /// Read `len` bytes starting at `offset` (pread / ranged GET). May return
    /// fewer bytes only at end-of-source.
    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>>;
}
```

`crates/dyndo-core/src/storage/s3.rs`:

```rust
//! S3-backed source. Stubbed until the S3 SDK is wired in.

use crate::error::{Error, Result};
use crate::storage::Source;

pub struct S3Source {
    pub bucket: String,
    pub key: String,
}

impl Source for S3Source {
    async fn size(&self) -> Result<u64> {
        Err(Error::Backend("s3 unimplemented".into()))
    }

    async fn read_at(&self, _offset: u64, _len: usize) -> Result<Vec<u8>> {
        Err(Error::Backend("s3 unimplemented".into()))
    }
}
```

`crates/dyndo-core/src/storage/mod.rs`:

```rust
mod s3;
mod source;

pub use s3::S3Source;
pub use source::Source;
```

Update `crates/dyndo-core/src/lib.rs`:

```rust
mod error;
mod model;
mod storage;

#[cfg(test)]
mod test_support;

pub use error::{Error, Result};
pub use model::{Asset, AudioTrack, Track, VideoTrack};
pub use storage::{S3Source, Source};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p dyndo-core`
Expected: PASS (test_support range test).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): add async Source trait, S3 stub, in-memory test source"
```

---

### Task 6: `storage/fs.rs` — local file source

**Files:**
- Create: `crates/dyndo-core/src/storage/fs.rs`
- Modify: `crates/dyndo-core/src/storage/mod.rs`

**Interfaces:**
- Produces: `storage::LocalFile` (`LocalFile::new(path)`), implementing `Source` over `tokio::fs`.

- [ ] **Step 1: Write the failing test**

In `crates/dyndo-core/src/storage/fs.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn reads_ranges_and_size_from_a_file() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(&[10, 11, 12, 13, 14]).unwrap();
        let src = LocalFile::new(f.path());
        assert_eq!(src.size().await.unwrap(), 5);
        assert_eq!(src.read_at(1, 3).await.unwrap(), vec![11, 12, 13]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dyndo-core storage::fs`
Expected: FAIL (`LocalFile` not defined).

- [ ] **Step 3: Write the implementation**

Prepend to `crates/dyndo-core/src/storage/fs.rs`:

```rust
//! Local filesystem source, backed by `tokio::fs`.

use std::io::SeekFrom;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::error::{Error, Result};
use crate::storage::Source;

pub struct LocalFile {
    path: PathBuf,
}

impl LocalFile {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    fn io_err(&self, source: std::io::Error) -> Error {
        Error::Io {
            path: self.path.display().to_string(),
            source,
        }
    }
}

impl Source for LocalFile {
    async fn size(&self) -> Result<u64> {
        let meta = tokio::fs::metadata(&self.path)
            .await
            .map_err(|e| self.io_err(e))?;
        Ok(meta.len())
    }

    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let mut file = tokio::fs::File::open(&self.path)
            .await
            .map_err(|e| self.io_err(e))?;
        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| self.io_err(e))?;
        let mut buf = vec![0u8; len];
        let n = file.read(&mut buf).await.map_err(|e| self.io_err(e))?;
        buf.truncate(n);
        Ok(buf)
    }
}
```

Update `crates/dyndo-core/src/storage/mod.rs`:

```rust
mod fs;
mod s3;
mod source;

pub use fs::LocalFile;
pub use s3::S3Source;
pub use source::Source;
```

Export it from `crates/dyndo-core/src/lib.rs` — change the storage `pub use` line to:

```rust
pub use storage::{LocalFile, S3Source, Source};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dyndo-core storage::fs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): add LocalFile source over tokio::fs"
```

---

### Task 7: `cmaf/codec.rs` — RFC6381 codec strings

**Files:**
- Create: `crates/dyndo-core/src/cmaf/codec.rs`, `crates/dyndo-core/src/cmaf/mod.rs`
- Modify: `crates/dyndo-core/src/lib.rs`

**Interfaces:**
- Produces: `cmaf::codec::avc_codec_string(profile: u8, compat: u8, level: u8) -> String`, `cmaf::codec::aac_codec_string(audio_object_type: u8) -> String`.

- [ ] **Step 1: Write the failing test**

In `crates/dyndo-core/src/cmaf/codec.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avc_high_profile_level_4() {
        // profile 0x64, compat 0x00, level 0x28 -> avc1.640028
        assert_eq!(avc_codec_string(0x64, 0x00, 0x28), "avc1.640028");
    }

    #[test]
    fn aac_lc() {
        assert_eq!(aac_codec_string(2), "mp4a.40.2");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dyndo-core cmaf::codec`
Expected: FAIL (functions not defined).

- [ ] **Step 3: Write the implementation**

Prepend to `crates/dyndo-core/src/cmaf/codec.rs`:

```rust
//! RFC6381 codec-string assembly from decoder-config fields.

/// `avc1.PPCCLL` from AVCDecoderConfigurationRecord profile/compat/level bytes.
pub fn avc_codec_string(profile: u8, compat: u8, level: u8) -> String {
    format!("avc1.{:02x}{:02x}{:02x}", profile, compat, level)
}

/// `mp4a.40.<audio_object_type>` (0x40 = MPEG-4 Audio object-type-indication).
pub fn aac_codec_string(audio_object_type: u8) -> String {
    format!("mp4a.40.{}", audio_object_type)
}
```

Create `crates/dyndo-core/src/cmaf/mod.rs`:

```rust
mod codec;
```

Update `crates/dyndo-core/src/lib.rs` to declare the module (add `mod cmaf;` after `mod asset;`/others — for now just add):

```rust
mod cmaf;
```

(Add the `mod cmaf;` line alongside the other `mod` declarations.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dyndo-core cmaf::codec`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): add RFC6381 codec-string assembly"
```

---

### Task 8: `cmaf/header.rs` — async CMAF header parsing

**Files:**
- Create: `crates/dyndo-core/src/cmaf/header.rs`
- Modify: `crates/dyndo-core/src/cmaf/mod.rs`

**Interfaces:**
- Consumes: `storage::Source`, `cmaf::codec::{avc_codec_string, aac_codec_string}`, `mp4-atom`.
- Produces:
  - `cmaf::CmafHeader { timescale: u32, duration: u64, init_range: ByteRange, segments: Vec<Segment>, track: TrackMeta }`
  - `cmaf::ByteRange { start: u64, end: u64 }`
  - `cmaf::Segment { offset: u64, size: u64, duration: u64 }`
  - `cmaf::TrackMeta` enum: `Video { codec, width, height, frame_rate }` / `Audio { codec, sample_rate, channels, language }`
  - `cmaf::read_header<S: Source>(source: &S, path: &str) -> Result<CmafHeader>`

- [ ] **Step 1: Exploration — confirm the `mp4-atom` navigation**

Write a throwaway test that decodes the video fixture's `moov` and `moof` with `mp4-atom` and prints their structure, so the exact field paths (`Mdia`, `Minf`, `Stbl`, `Stsd`, `Hdlr`, `Mdhd`, `Mp4a`, `Esds`, `Traf`, `Tfhd`, `Trun`) are known before writing `read_header`. In `crates/dyndo-core/src/cmaf/header.rs`:

```rust
#[cfg(test)]
mod explore {
    use mp4_atom::{Header, ReadFrom, ReadAtom, Moov, Sidx, Moof};
    use std::io::Cursor;

    #[test]
    #[ignore]
    fn dump_boxes() {
        let bytes = std::fs::read(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/index_video_avc_1080.mp4"),
        )
        .unwrap();
        let mut off = 0usize;
        loop {
            let mut c = Cursor::new(&bytes[off..off + 16.min(bytes.len() - off)]);
            let h = Header::read_from(&mut c).unwrap();
            let hlen = c.position() as usize;
            let blen = h.size.unwrap();
            let body = &bytes[off + hlen..off + hlen + blen];
            match &h.kind {
                k if *k == Moov::KIND => { dbg!(Moov::read_atom(&h, &mut Cursor::new(body)).unwrap()); }
                k if *k == Sidx::KIND => { let s = Sidx::read_atom(&h, &mut Cursor::new(body)).unwrap(); dbg!(s.timescale, s.references.len()); }
                k if *k == Moof::KIND => { dbg!(Moof::read_atom(&h, &mut Cursor::new(body)).unwrap()); break; }
                _ => {}
            }
            off += hlen + blen;
        }
    }
}
```

Run: `cargo test -p dyndo-core cmaf::header::explore::dump_boxes -- --ignored --nocapture`

Read the `dbg!` output and record the exact field paths for: handler type (video=`vide`, audio=`soun`), media `timescale` + `language`, sample-entry `width`/`height` + `Avcc`, audio `channel_count`/`sample_rate` + `Esds` object type, and `Tfhd.default_sample_duration` / `Trun` sample duration. Use those paths in Step 3. If `Moov::KIND`-style constants don't exist, compare `h.kind` against the 4-byte codes directly (e.g. `mp4_atom::Moov::KIND` vs a `FourCC`).

- [ ] **Step 2: Write the failing test**

Append to `crates/dyndo-core/src/cmaf/header.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::BytesSource;

    fn fixture(name: &str) -> BytesSource {
        let bytes = std::fs::read(format!(
            "{}/tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        ))
        .unwrap();
        BytesSource::new(bytes)
    }

    #[tokio::test]
    async fn reads_video_header() {
        let src = fixture("index_video_avc_1080.mp4");
        let h = read_header(&src, "index_video_avc_1080.mp4").await.unwrap();
        assert_eq!(h.timescale, 90000);
        assert_eq!(h.duration, 123328800);
        assert_eq!(h.segments.len(), 715);
        assert_eq!(h.init_range.start, 0);
        match h.track {
            TrackMeta::Video { codec, width, height, frame_rate } => {
                assert_eq!(codec, "avc1.640028");
                assert_eq!((width, height), (1920, 1080));
                assert_eq!(frame_rate, "25/1");
            }
            _ => panic!("expected video"),
        }
    }

    #[tokio::test]
    async fn reads_audio_header() {
        let src = fixture("index_audio_aac_nl_2.mp4");
        let h = read_header(&src, "index_audio_aac_nl_2.mp4").await.unwrap();
        assert_eq!(h.timescale, 48000);
        assert_eq!(h.duration, 65775616);
        assert_eq!(h.segments.len(), 715);
        match h.track {
            TrackMeta::Audio { codec, sample_rate, channels, language } => {
                assert_eq!(codec, "mp4a.40.2");
                assert_eq!(sample_rate, 48000);
                assert_eq!(channels, 2);
                assert_eq!(language.as_deref(), Some("nld"));
            }
            _ => panic!("expected audio"),
        }
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p dyndo-core cmaf::header::tests`
Expected: FAIL (`read_header` not defined).

- [ ] **Step 4: Write the implementation**

Prepend to `crates/dyndo-core/src/cmaf/header.rs`. Field paths marked `// CONFIRM` were validated in Step 1 — adjust to the exact `mp4-atom` names discovered there.

```rust
//! Async, header-first CMAF parsing. Reads moov + sidx + first moof, then stops
//! (never touches an mdat). Returns an internal `CmafHeader`; mapping to the
//! serde model lives in `asset.rs`.

use std::io::Cursor;

use mp4_atom::{Header, Moof, Moov, ReadAtom, ReadFrom, Sidx};

use crate::cmaf::codec::{aac_codec_string, avc_codec_string};
use crate::error::{Error, Result};
use crate::storage::Source;

#[derive(Debug, Clone, PartialEq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub offset: u64,
    pub size: u64,
    pub duration: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrackMeta {
    Video {
        codec: String,
        width: u32,
        height: u32,
        frame_rate: String,
    },
    Audio {
        codec: String,
        sample_rate: u32,
        channels: u16,
        language: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CmafHeader {
    pub timescale: u32,
    pub duration: u64,
    pub init_range: ByteRange,
    pub segments: Vec<Segment>,
    pub track: TrackMeta,
}

fn malformed(path: &str, box_type: &str, reason: impl Into<String>) -> Error {
    Error::MalformedBox {
        box_type: box_type.into(),
        path: path.into(),
        reason: reason.into(),
    }
}

/// Reduce a `num/den` fraction to lowest terms as a `"n/d"` string.
fn frame_rate_string(timescale: u32, sample_duration: u32) -> String {
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    if sample_duration == 0 {
        return format!("{}/1", timescale);
    }
    let g = gcd(timescale, sample_duration).max(1);
    format!("{}/{}", timescale / g, sample_duration / g)
}

pub async fn read_header<S: Source>(source: &S, path: &str) -> Result<CmafHeader> {
    let mut offset = 0u64;
    let mut moov: Option<Moov> = None;
    let mut sidx: Option<Sidx> = None;
    let mut first_moof: Option<Moof> = None;
    let mut moov_end = 0u64;
    let mut sidx_end = 0u64;

    // Header-first scan: read moov, sidx, first moof; skip everything else.
    while moov.is_none() || sidx.is_none() || first_moof.is_none() {
        let head_bytes = source.read_at(offset, 16).await?;
        if head_bytes.len() < 8 {
            break; // reached end without the boxes we need
        }
        let mut cursor = Cursor::new(&head_bytes[..]);
        let header = Header::read_from(&mut cursor)
            .map_err(|e| malformed(path, "box header", e.to_string()))?;
        let header_len = cursor.position();
        let body_len = header
            .size
            .ok_or_else(|| malformed(path, "box", "unbounded box size"))?;
        let body_start = offset + header_len;
        let box_end = body_start + body_len as u64;

        if header.kind == Moov::KIND {
            let body = source.read_at(body_start, body_len).await?;
            moov = Some(
                Moov::read_atom(&header, &mut Cursor::new(&body[..]))
                    .map_err(|e| malformed(path, "moov", e.to_string()))?,
            );
            moov_end = box_end;
        } else if header.kind == Sidx::KIND {
            let body = source.read_at(body_start, body_len).await?;
            sidx = Some(
                Sidx::read_atom(&header, &mut Cursor::new(&body[..]))
                    .map_err(|e| malformed(path, "sidx", e.to_string()))?,
            );
            sidx_end = box_end;
        } else if header.kind == Moof::KIND {
            let body = source.read_at(body_start, body_len).await?;
            first_moof = Some(
                Moof::read_atom(&header, &mut Cursor::new(&body[..]))
                    .map_err(|e| malformed(path, "moof", e.to_string()))?,
            );
        }
        offset = box_end;
    }

    let moov = moov.ok_or_else(|| Error::MissingMoov(path.into()))?;
    let sidx = sidx.ok_or_else(|| Error::MissingSidx(path.into()))?;

    // Exactly one track.
    if moov.trak.len() != 1 {
        return Err(Error::NotSingleTrack {
            path: path.into(),
            count: moov.trak.len(),
        });
    }
    let trak = &moov.trak[0]; // CONFIRM: Moov.trak field (Vec<Trak>)

    // Segment map from sidx.
    let timescale = sidx.timescale;
    let mut seg_offset = sidx_end + sidx.first_offset;
    let mut segments = Vec::with_capacity(sidx.references.len());
    for r in &sidx.references {
        segments.push(Segment {
            offset: seg_offset,
            size: r.reference_size as u64,
            duration: r.subsegment_duration as u64,
        });
        seg_offset += r.reference_size as u64;
    }
    let duration: u64 = segments.iter().map(|s| s.duration).sum();

    // Track metadata. CONFIRM every path below against Step 1's dump.
    let handler = &trak.mdia.hdlr.handler_type; // CONFIRM: FourCC 'vide'/'soun'
    let media_timescale = trak.mdia.mdhd.timescale; // CONFIRM
    let stsd = &trak.mdia.minf.stbl.stsd; // CONFIRM path

    let track = if is_video(handler) {
        let avc1 = video_sample_entry(stsd)
            .ok_or_else(|| malformed(path, "stsd", "no avc1 sample entry"))?;
        let avcc = &avc1.avcc; // CONFIRM
        let codec = avc_codec_string(
            avcc.avc_profile_indication,
            avcc.profile_compatibility,
            avcc.avc_level_indication,
        );
        let sample_duration = first_sample_duration(first_moof.as_ref());
        TrackMeta::Video {
            codec,
            width: avc1.width as u32,   // CONFIRM width/height fields
            height: avc1.height as u32,
            frame_rate: frame_rate_string(media_timescale, sample_duration),
        }
    } else if is_audio(handler) {
        let mp4a = audio_sample_entry(stsd)
            .ok_or_else(|| malformed(path, "stsd", "no mp4a sample entry"))?;
        let aot = audio_object_type(&mp4a.esds); // CONFIRM esds -> object type
        TrackMeta::Audio {
            codec: aac_codec_string(aot),
            sample_rate: mp4a.sample_rate as u32, // CONFIRM
            channels: mp4a.channel_count as u16,  // CONFIRM
            language: language_string(&trak.mdia.mdhd), // CONFIRM
        }
    } else {
        return Err(Error::UnsupportedCodec {
            path: path.into(),
            codec: format!("{:?}", handler),
        });
    };

    Ok(CmafHeader {
        timescale,
        duration,
        init_range: ByteRange { start: 0, end: moov_end },
        segments,
        track,
    })
}
```

Then add the small helper functions used above (`is_video`, `is_audio`, `video_sample_entry`, `audio_sample_entry`, `audio_object_type`, `language_string`, `first_sample_duration`) matching the exact `mp4-atom` types found in Step 1. Guidance:
- `is_video`/`is_audio`: compare the handler FourCC to `b"vide"` / `b"soun"`.
- `video_sample_entry`/`audio_sample_entry`: pull the single entry out of `stsd` (it may be a `Vec` or an enum of sample-entry variants — match the `Avc1` / `Mp4a` variant).
- `audio_object_type`: from `Esds` decoder config; AAC-LC is `2`. If the raw object type isn't directly exposed, read it from the decoder-specific info.
- `language_string`: `mdhd` language is packed ISO-639-2; if `mp4-atom` exposes it as a `String`, use it directly and map `"und"` → `None`. Otherwise unpack the three 5-bit chars (`+ 0x60`).
- `first_sample_duration`: `tfhd.default_sample_duration` if present, else the first `trun` entry's duration; default `0` if no moof.

Iterate compile/test until both `read_header` tests pass. Delete the `explore` module once done.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p dyndo-core cmaf::header::tests`
Expected: PASS (both video and audio).

- [ ] **Step 6: Export and commit**

Update `crates/dyndo-core/src/cmaf/mod.rs`:

```rust
mod codec;
mod header;

pub use header::{read_header, ByteRange, CmafHeader, Segment, TrackMeta};
```

```bash
git add -A
git commit -m "feat(core): add async CMAF header parser (moov+sidx+first moof)"
```

---

### Task 9: `asset.rs` — orchestration (describe_track, build_asset)

**Files:**
- Create: `crates/dyndo-core/src/asset.rs`
- Modify: `crates/dyndo-core/src/lib.rs`

**Interfaces:**
- Consumes: `cmaf::{read_header, CmafHeader, TrackMeta}`, `storage::{Source, LocalFile}`, `model::id`, model types.
- Produces: `describe_track<S: Source>(source: &S, key: String) -> Result<Track>`, `build_asset(inputs: &[PathBuf]) -> Result<Asset>`.

- [ ] **Step 1: Write the failing test**

In `crates/dyndo-core/src/asset.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::BytesSource;

    fn fixture(name: &str) -> BytesSource {
        let bytes = std::fs::read(format!(
            "{}/tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        ))
        .unwrap();
        BytesSource::new(bytes)
    }

    #[tokio::test]
    async fn describes_video_track_with_computed_bandwidth_and_id() {
        let src = fixture("index_video_avc_1080.mp4");
        let track = describe_track(&src, "index_video_avc_1080.mp4".into())
            .await
            .unwrap();
        match track {
            Track::Video(v) => {
                assert_eq!(v.id, "video_avc_1080_4807");
                assert_eq!(v.bandwidth, 4807228);
                assert_eq!(v.codec, "avc1.640028");
                assert_eq!(v.source, "index_video_avc_1080.mp4");
            }
            _ => panic!("expected video"),
        }
    }

    #[tokio::test]
    async fn build_asset_rejects_duplicate_ids() {
        // Two paths to the same fixture -> identical ids -> error.
        let base = format!("{}/tests/fixtures/index_audio_aac_nl_2.mp4", env!("CARGO_MANIFEST_DIR"));
        let err = build_asset(&[base.clone().into(), base.into()]).await.unwrap_err();
        assert!(matches!(err, Error::DuplicateTrackId(_)));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dyndo-core asset::`
Expected: FAIL (`describe_track`/`build_asset` not defined).

- [ ] **Step 3: Write the implementation**

Prepend to `crates/dyndo-core/src/asset.rs`:

```rust
//! Orchestration: parse a source's header, compute derived fields, build the
//! serde `Track`/`Asset`.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::cmaf::{read_header, TrackMeta};
use crate::error::{Error, Result};
use crate::model::id::{audio_track_id, video_track_id};
use crate::model::{Asset, AudioTrack, Track, VideoTrack};
use crate::storage::{LocalFile, Source};

/// Average bitrate in bits/s from the segment sizes and duration.
fn average_bandwidth(total_bytes: u64, duration: u64, timescale: u32) -> u32 {
    if duration == 0 {
        return 0;
    }
    let seconds = duration as f64 / timescale as f64;
    (total_bytes as f64 * 8.0 / seconds).round() as u32
}

pub async fn describe_track<S: Source>(source: &S, key: String) -> Result<Track> {
    let header = read_header(source, &key).await?;
    let total_bytes: u64 = header.segments.iter().map(|s| s.size).sum();
    let bandwidth = average_bandwidth(total_bytes, header.duration, header.timescale);

    let track = match header.track {
        TrackMeta::Video {
            codec,
            width,
            height,
            frame_rate,
        } => Track::Video(VideoTrack {
            id: video_track_id(&codec, height, bandwidth),
            source: key,
            codec,
            timescale: header.timescale,
            duration: header.duration,
            bandwidth,
            width,
            height,
            frame_rate,
        }),
        TrackMeta::Audio {
            codec,
            sample_rate,
            channels,
            language,
        } => Track::Audio(AudioTrack {
            id: audio_track_id(&codec, language.as_deref(), channels, bandwidth),
            source: key,
            codec,
            timescale: header.timescale,
            duration: header.duration,
            bandwidth,
            sample_rate,
            channels,
            language,
        }),
    };
    Ok(track)
}

pub async fn build_asset(inputs: &[PathBuf]) -> Result<Asset> {
    let mut tracks = Vec::with_capacity(inputs.len());
    for path in inputs {
        let key = path.to_string_lossy().into_owned();
        let source = LocalFile::new(path);
        tracks.push(describe_track(&source, key).await?);
    }

    let mut seen = HashSet::new();
    for track in &tracks {
        if !seen.insert(track.id().to_string()) {
            return Err(Error::DuplicateTrackId(track.id().to_string()));
        }
    }

    Ok(Asset { tracks })
}
```

Update `crates/dyndo-core/src/lib.rs` (final form):

```rust
mod asset;
mod cmaf;
mod error;
mod model;
mod storage;

pub use asset::{build_asset, describe_track};
pub use error::{Error, Result};
pub use model::{Asset, AudioTrack, Track, VideoTrack};
pub use storage::{LocalFile, S3Source, Source};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p dyndo-core`
Expected: PASS (all core tests).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): add describe_track/build_asset orchestration"
```

---

### Task 10: `dyndo-cli` — CLI + integration test

**Files:**
- Modify: `crates/dyndo-cli/src/main.rs`
- Create: `crates/dyndo-cli/tests/cli.rs`

**Interfaces:**
- Consumes: `dyndo_core::build_asset`.

- [ ] **Step 1: Write the failing integration test**

`crates/dyndo-cli/tests/cli.rs`:

```rust
use std::process::Command;

#[test]
fn writes_asset_json_for_video_and_audio() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("asset.json");
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/../dyndo-core/tests/fixtures");

    let status = Command::new(env!("CARGO_BIN_EXE_dyndo"))
        .arg("-i").arg(format!("{fixtures}/index_video_avc_1080.mp4"))
        .arg("-i").arg(format!("{fixtures}/index_audio_aac_nl_2.mp4"))
        .arg("-o").arg(&out)
        .status()
        .unwrap();
    assert!(status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(json["tracks"].as_array().unwrap().len(), 2);
    assert_eq!(json["tracks"][0]["type"], "video");
    assert_eq!(json["tracks"][0]["codec"], "avc1.640028");
    assert_eq!(json["tracks"][1]["type"], "audio");
    assert_eq!(json["tracks"][1]["language"], "nld");
}
```

Add `tempfile` and `serde_json` as dev-deps to `crates/dyndo-cli/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
serde_json = "1"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dyndo-cli`
Expected: FAIL (CLI does nothing yet).

- [ ] **Step 3: Write the implementation**

`crates/dyndo-cli/src/main.rs`:

```rust
use std::path::PathBuf;

use clap::Parser;

/// Build an asset.json descriptor from one or more CMAF files.
#[derive(Parser)]
#[command(name = "dyndo", version, about)]
struct Cli {
    /// Input CMAF file (repeatable, one track each).
    #[arg(short, long = "input", required = true)]
    input: Vec<PathBuf>,

    /// Output descriptor path.
    #[arg(short, long = "output", default_value = "asset.json")]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let asset = dyndo_core::build_asset(&cli.input).await?;
    let json = serde_json::to_string_pretty(&asset)?;
    tokio::fs::write(&cli.output, json).await?;
    println!("wrote {} ({} tracks)", cli.output.display(), asset.tracks.len());
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dyndo-cli`
Expected: PASS.

- [ ] **Step 5: Full workspace check**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: PASS (fix any clippy/fmt issues, e.g. add `#[allow(async_fn_in_trait)]` already present).

- [ ] **Step 6: Manual smoke test against the real 800 MB files (bounded memory)**

Run: `cd /Users/matvp/Development/dyndo && cargo run -p dyndo-cli -- -i assets/index_video_avc_1080.mp4 -i assets/index_video_avc_720.mp4 -i assets/index_audio_aac_nl_2.mp4 -o asset.json && cat asset.json`
Expected: three tracks written quickly (no long read of the full files); ids `video_avc_1080_*`, `video_avc_720_*`, `audio_aac_nld_2_*`.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(cli): add dyndo binary and integration test"
```

---

## Notes for the implementer

- **`mp4-atom` API is the main unknown.** Task 8 Step 1 is the de-risking step — run it first and let the printed structure drive the exact field paths. The compiler + the two fixture tests are your ground truth.
- If `Header::size` semantics differ (it is the body size *excluding* the 8/16-byte header), the offset math in `read_header` already accounts for it via `cursor.position()`.
- Keep `mod.rs`/`lib.rs` export-only; put all logic in named files.
