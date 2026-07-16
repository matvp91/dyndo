# Getting started

This tutorial takes you from an empty directory to a **playing adaptive stream**
served by dyndo. Along the way you'll build the tools, create a pair of CMAF
sources, index them into an `asset.json`, and start the server.

You don't need to know anything about CMAF, DASH, or HLS to follow along — we'll
create everything from scratch. By the end you'll have a running server
answering DASH and HLS requests, and you'll understand the two-step
**index-then-serve** workflow that everything else in dyndo builds on.

Set aside about 15 minutes.

## What you'll need

- A stable [Rust toolchain](https://www.rust-lang.org/tools/install) (Rust
  1.97 or newer) with `cargo`.
- [`git`](https://git-scm.com/), to clone the repository.
- [`ffmpeg`](https://ffmpeg.org/), used **only** to create the sample media in
  step 2. If you already have CMAF files, you can skip that step.

## Step 1: Build dyndo

> Only need the `dyndo` CLI? The [one-line installer](./install-cli.md) gives
> you a prebuilt binary. This tutorial builds from source because it also
> needs `dyndo-server`.

Clone the repository and build both binaries:

```bash
git clone https://github.com/matvp91/dyndo.git
cd dyndo
cargo build
```

Install the `dyndo` CLI onto your `PATH` so you can call it by name:

```bash
make install   # installs `dyndo` into ~/.cargo/bin
```

Check that it works — it prints its name and version:

```bash
dyndo --version
```

> If `dyndo` isn't found, make sure `~/.cargo/bin` is on your `PATH`. You can
> always run the CLI without installing it: `cargo run -p dyndo-cli -- --version`.

## Step 2: Create two CMAF sources

dyndo serves **CMAF**: fragmented MP4 files, one media track each, carrying a
single global `sidx` segment index. Let's make one video track and one audio
track.

First, generate a 10-second test clip with a video and an audio stream:

```bash
ffmpeg -f lavfi -i "testsrc=size=1280x720:rate=25:duration=10" \
       -f lavfi -i "sine=frequency=440:duration=10" \
       -c:v libx264 -profile:v high -pix_fmt yuv420p -g 50 -keyint_min 50 \
       -c:a aac -b:a 128k -ar 48000 \
       source.mp4
```

Now repackage each track into its own CMAF file, into an `assets/` directory —
that's where the server looks for descriptors by default. The
`+global_sidx` flag is the important one: it writes a **single** segment index
covering the whole file, which is what dyndo reads.

```bash
mkdir -p assets

# Video track -> assets/video.mp4
ffmpeg -i source.mp4 -map 0:v:0 -c copy \
  -movflags +frag_keyframe+empty_moov+separate_moof+default_base_moof+global_sidx \
  assets/video.mp4

# Audio track -> assets/audio.mp4
ffmpeg -i source.mp4 -map 0:a:0 -c copy \
  -movflags +frag_keyframe+empty_moov+separate_moof+default_base_moof+global_sidx \
  assets/audio.mp4
```

You now have two CMAF sources under `assets/`.

## Step 3: Index the sources

Turn your two files into an `asset.json` descriptor. Each `-i` adds one track;
paths are resolved relative to the output descriptor's directory, so from the
repository root they're just the file names inside `assets/`:

```bash
dyndo index video.mp4 audio.mp4 -o assets/asset.json
```

```text
wrote assets/asset.json (2 tracks)
```

Take a look at what it wrote:

```bash
cat assets/asset.json
```

```json
{
  "tracks": [
    {
      "type": "video",
      "id": "video_avc1_720_126233",
      "path": "video.mp4",
      "fourcc": "avc1",
      "timescale": 12800,
      "width": 1280,
      "height": 720
    },
    {
      "type": "audio",
      "id": "audio_mp4a_und_1_130130",
      "path": "audio.mp4",
      "fourcc": "mp4a",
      "timescale": 48000,
      "sample_rate": 48000,
      "channels": 1,
      "language": "und"
    }
  ]
}
```

That's the whole descriptor: per-track metadata and a source path, nothing more.
Notice there's no segment list and no byte offsets — the server re-derives those
from each source at request time. (Your `id` numbers may differ slightly; they
include the measured bitrate, which depends on your exact encode.)

## Step 4: Start the server

From the repository root:

```bash
make run
```

```text
dyndo-server listening on http://0.0.0.0:8080
```

The server reads descriptors from `./assets` (set in the repository's
`config.yaml`) and exposes each one as **both** a DASH and an HLS stream. Leave
it running and open a second terminal for the next step.

## Step 5: Play the stream

Your asset is `assets/asset.json`, so its path relative to the assets root is
just `asset.json`. First, confirm the DASH manifest is being served:

```bash
curl http://localhost:8080/asset.json/dash/index.mpd
```

You should see an XML document beginning with `<MPD …>` that lists a video and
an audio `AdaptationSet`. The HLS multivariant playlist is served from the same
asset:

```bash
curl http://localhost:8080/asset.json/hls/index.m3u8
```

```text
#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,URI="audio_mp4a_und_1_130130.m3u8",GROUP-ID="mp4a",LANGUAGE="und",NAME="und",DEFAULT=YES,AUTOSELECT=YES,CHANNELS="1"
#EXT-X-STREAM-INF:BANDWIDTH=250184,CODECS="avc1.64001f,mp4a.40.2",RESOLUTION=1280x720,FRAME-RATE=25.000,AUDIO="mp4a"
video_avc1_720_126233.m3u8
#EXT-X-INDEPENDENT-SEGMENTS
```

**That's a working stream.** To watch it, point a player at the manifest URL.
[VLC](https://www.videolan.org/) can open either protocol directly:

```bash
vlc http://localhost:8080/asset.json/dash/index.mpd    # or the .m3u8 for HLS
```

You'll see the ffmpeg test pattern play and hear the tone. dyndo is serving the
manifest and every segment live, reading ranges out of `assets/video.mp4` and
`assets/audio.mp4` as the player requests them.

When you're done, stop the server with `Ctrl-C`.

## What you did

In a few minutes you:

1. built the `dyndo` CLI and `dyndo-server`;
2. produced two CMAF sources;
3. **indexed** them into a tiny `asset.json` descriptor; and
4. **served** that descriptor as a live DASH and HLS stream.

That index-then-serve split is the core of dyndo: index once, serve many
protocols, and never duplicate your media.

## Where to next

- Add subtitles to the stream you just built:
  [Add a subtitle track](../how-to/add-subtitles.md).
- Run the server as a container instead of from source:
  [Deploy with Docker](../how-to/deploy-with-docker.md).
- Serve your media from object storage instead of local disk:
  [Serve media from S3](../how-to/serve-from-s3.md).
- Understand what just happened under the hood:
  [The thin-pointer approach](../explanation/thin-pointer.md).
- Look up every command and option: [dyndo CLI reference](../reference/cli.md).
