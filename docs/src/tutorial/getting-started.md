# Getting started

This tutorial takes you from an empty directory to a **playing adaptive stream**
served by dyndo. Along the way you'll install the CLI, create a pair of CMAF
sources, index them into an `asset.json`, and start the server as a container.

You don't need to know anything about CMAF, DASH, or HLS to follow along — we'll
create everything from scratch. By the end you'll have a running server
answering DASH and HLS requests, and you'll understand the two-step
**index-then-serve** workflow that everything else in dyndo builds on.

Set aside about 15 minutes.

## What you'll need

- The **`dyndo` CLI** — you'll install it in step 1, no Rust toolchain required.
- [`ffmpeg`](https://ffmpeg.org/), used **only** to create the sample media in
  step 2. If you already have CMAF files, you can skip that step.
- [Docker](https://docs.docker.com/get-docker/), to run the server in step 4.

Work in a fresh directory of your choice; every command below is run from there.

## Step 1: Install the CLI

Install the prebuilt `dyndo` CLI with the one-line installer:

```bash
curl -fsSL https://matvp91.github.io/dyndo/install.sh | bash
```

Open a new terminal so the updated `PATH` takes effect, then check it works — it
prints its name and version:

```bash
dyndo --version
```

> Prefer not to use the installer, or on a platform it doesn't cover? See
> [Install the CLI](./install-cli.md) for options, or
> [Build from source](../how-to/build-from-source.md).

## Step 2: Create two CMAF sources

dyndo serves **CMAF**: fragmented MP4 files, one media track each, carrying a
single global `sidx` segment index. Let's make one video track and one audio
track.

First, generate a 10-second test clip with a video and an audio stream:

```bash
ffmpeg -f lavfi -i "testsrc=size=1280x720:rate=25:duration=10" \
       -f lavfi -i "sine=frequency=440:duration=10" \
       -c:v libx264 -profile:v high -pix_fmt yuv420p -g 50 -keyint_min 50 \
       -c:a aac -b:a 128k -ar 48000 -ac 2 \
       source.mp4
```

Now repackage each track into its own CMAF file, into an `assets/` directory —
that's the directory you'll hand to the server in step 4. The `+global_sidx`
flag is the important one: it writes a **single** segment index covering the
whole file, which is what dyndo reads.

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

Turn your two files into an `asset.json` descriptor. Each positional argument
adds one track; paths are resolved relative to the output descriptor's
directory, so from your working directory they're just the file names inside
`assets/`. An input can also carry `key=value` parameters after the path —
here we tag the audio track's language:

```bash
dyndo index video.mp4 audio.mp4,language=eng -o assets/asset.json
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
      "id": "video_720_avc1_126233",
      "path": "video.mp4",
      "type": "video",
      "width": 1280,
      "height": 720,
      "fourcc": "avc1"
    },
    {
      "id": "audio_und_2_mp4a_131171",
      "path": "audio.mp4",
      "type": "audio",
      "sample_rate": 48000,
      "channels": 2,
      "language": "eng",
      "fourcc": "mp4a"
    }
  ]
}
```

That's the whole descriptor: per-track metadata and a source path, nothing more.
Notice there's no segment list and no byte offsets — the server re-derives those
from each source at request time. (Your `id` numbers may differ slightly; they
include the measured bitrate, which depends on your exact encode. And the audio
id says `und` even though we set `language=eng`: ids are minted from what the
file itself declares — our ffmpeg test clip has no language — and then never
change, so URLs stay stable however you relabel a track.)

## Step 4: Start the server

Run `dyndo-server` from Docker Hub, mounting your `assets/` directory into the
container:

```bash
docker run --rm -p 8080:8080 \
  -e DYNDO_FS__ROOT=/assets \
  -v "$PWD/assets:/assets:ro" \
  matvp91/dyndo-server
```

```text
dyndo-server listening on http://0.0.0.0:8080
```

- `-v "$PWD/assets:/assets:ro"` mounts your descriptor and its CMAF sources into
  the container, read-only.
- `-e DYNDO_FS__ROOT=/assets` points the server's storage root at that mount.
- `-p 8080:8080` publishes the port to your host.

The server exposes each descriptor it finds as **both** a DASH and an HLS stream.
Leave it running and open a second terminal for the next step. (For everything
Docker can do here — pinning a version, using a config file, serving from S3 —
see [Deploy with Docker](../how-to/deploy-with-docker.md).)

## Step 5: Play the stream

Your descriptor is at `assets/asset.json`, and the storage root is that
`assets/` directory, so its path relative to the root is just `asset.json`.
First, confirm the DASH manifest is being served:

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
#EXT-X-MEDIA:TYPE=AUDIO,URI="audio_und_2_mp4a_131171.m3u8",GROUP-ID="mp4a",LANGUAGE="eng",NAME="eng",DEFAULT=YES,AUTOSELECT=YES,CHANNELS="2"
#EXT-X-STREAM-INF:BANDWIDTH=257404,CODECS="avc1.64001f,mp4a.40.2",RESOLUTION=1280x720,FRAME-RATE=25.000,AUDIO="mp4a"
video_720_avc1_126233.m3u8
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

1. installed the `dyndo` CLI;
2. produced two CMAF sources;
3. **indexed** them into a tiny `asset.json` descriptor; and
4. **served** that descriptor as a live DASH and HLS stream with Docker.

That index-then-serve split is the core of dyndo: index once, serve many
protocols, and never duplicate your media.

## Where to next

- Add subtitles to the stream you just built:
  [Add a subtitle track](../how-to/add-subtitles.md).
- Label audio and subtitle tracks so players present them correctly:
  [Label tracks with roles](../how-to/label-roles.md).
- Everything Docker can do — pin a version, mount a config file, run in
  production: [Deploy with Docker](../how-to/deploy-with-docker.md).
- Serve your media from object storage instead of local disk:
  [Serve media from S3](../how-to/serve-from-s3.md).
- Understand what just happened under the hood:
  [The thin-pointer approach](../explanation/thin-pointer.md).
- Look up every command and option: [dyndo CLI reference](../reference/cli.md).
